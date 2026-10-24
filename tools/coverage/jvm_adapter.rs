//! Merge `JaCoCo` analysis with configured source ownership.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use roxmltree::{Document, ParsingOptions};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::build_event::{ArtifactPolicy, BuildEvents, lexical_path};
use crate::{Result, invalid};

#[derive(Deserialize)]
struct Inventory {
    coverage_roots: Vec<String>,
    eligible: Vec<Source>,
    #[serde(default)]
    jvm_source_declarations: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    jvm_tests: BTreeSet<String>,
}
#[derive(Deserialize)]
struct Source {
    path: String,
    language: String,
    owners: Vec<Owner>,
}
#[derive(Deserialize)]
struct Owner {
    label: String,
    kind: String,
}
#[derive(Deserialize)]
struct Inputs {
    sources: Vec<String>,
    jars: Vec<String>,
    #[serde(default)]
    children: Vec<String>,
}
struct Declaration {
    data: Value,
    inputs: Inputs,
    baseline_path: PathBuf,
    baseline: Xml,
    order: usize,
    jars: Vec<PathBuf>,
}

// Own the parsed tree so declarations do not borrow temporary input buffers.
struct Xml {
    tag: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Self>,
}
impl Xml {
    fn parse(path: &Path) -> Result<Self> {
        let text = read_text(path)?;
        let document = Document::parse_with_options(
            &text,
            ParsingOptions {
                allow_dtd: true,
                ..ParsingOptions::default()
            },
        )
        .map_err(|error| invalid(error.to_string()))?;
        Ok(Self::from_node(document.root_element()))
    }
    fn from_node(node: roxmltree::Node<'_, '_>) -> Self {
        let qualified = |namespace: Option<&str>, name: &str| {
            namespace.map_or_else(
                || name.to_owned(),
                |namespace| format!("{{{namespace}}}{name}"),
            )
        };
        Self {
            tag: qualified(node.tag_name().namespace(), node.tag_name().name()),
            attributes: node
                .attributes()
                .map(|a| (qualified(a.namespace(), a.name()), a.value().to_owned()))
                .collect(),
            children: node
                .children()
                .filter(roxmltree::Node::is_element)
                .map(Self::from_node)
                .collect(),
        }
    }
    fn children(&self, tag: &str) -> impl Iterator<Item = &Self> {
        self.children.iter().filter(move |node| node.tag == tag)
    }
    fn classes(&self) -> impl Iterator<Item = &Self> {
        self.children("package")
            .flat_map(|package| package.children("class"))
    }
    fn attribute(&self, name: &str) -> Result<&str> {
        self.attributes
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| invalid(format!("Missing JaCoCo attribute: {name}")))
    }
    fn source_lines(&self) -> Result<BTreeMap<(String, String), BTreeSet<i64>>> {
        let mut lines = BTreeMap::new();
        for package in self.children("package") {
            for source in package.children("sourcefile") {
                lines.insert(
                    (
                        package.attribute("name")?.to_owned(),
                        source.attribute("name")?.to_owned(),
                    ),
                    source
                        .children("line")
                        .map(|line| integer(line.attribute("nr")?))
                        .collect::<Result<_>>()?,
                );
            }
        }
        Ok(lines)
    }
}
fn integer(text: &str) -> Result<i64> {
    text.trim()
        .parse()
        .map_err(|error: std::num::ParseIntError| invalid(error.to_string()))
}
fn filename(name: &str) -> &str {
    Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
}
fn read_text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_str(&read_text(path)?)?)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    Ok(fs::write(
        path,
        serde_json::to_string_pretty(value)? + "\n",
    )?)
}
fn canonical(path: &Path) -> Result<String> {
    Ok(fs::canonicalize(path)?.to_string_lossy().into_owned())
}
fn source_candidates(inputs: &Inputs, node: &Xml) -> Vec<String> {
    inputs
        .sources
        .iter()
        .filter(|name| {
            node.attributes
                .get("sourcefilename")
                .is_some_and(|source| filename(name) == source)
        })
        .cloned()
        .collect()
}

#[derive(Default)]
struct ClassIdentities {
    owners: BTreeMap<String, BTreeSet<String>>,
    sources: BTreeMap<(String, String), BTreeSet<String>>,
    fingerprints: BTreeMap<(String, String), String>,
    artifacts: BTreeMap<(String, String), BTreeSet<String>>,
    uncertain: BTreeSet<(String, String)>,
    identity_owners: BTreeMap<(String, String), BTreeSet<String>>,
}
impl ClassIdentities {
    fn collect(declarations: &BTreeMap<String, Declaration>) -> Result<Self> {
        let mut result = Self::default();
        for (label, declaration) in declarations {
            for node in declaration.baseline.classes() {
                result
                    .owners
                    .entry(node.attribute("name")?.to_owned())
                    .or_default()
                    .insert(label.clone());
            }
            for jar in &declaration.jars {
                let mut archive = ZipArchive::new(fs::File::open(jar)?)
                    .map_err(|error| invalid(error.to_string()))?;
                let names: BTreeSet<_> = archive
                    .file_names()
                    .map(|name| {
                        name.map(std::borrow::Cow::into_owned)
                            .map_err(|error| invalid(error.to_string()))
                    })
                    .collect::<Result<_>>()?;
                for node in declaration.baseline.classes() {
                    let name = node.attribute("name")?;
                    let mut entry = format!("{name}.class");
                    if names.contains(&(entry.clone() + ".uninstrumented")) {
                        entry += ".uninstrumented";
                    }
                    if !names.contains(&entry) {
                        continue;
                    }
                    let mut bytes = Vec::new();
                    archive
                        .by_name(&entry)
                        .map_err(|error| invalid(error.to_string()))?
                        .read_to_end(&mut bytes)?;
                    let mut digest = String::with_capacity(64);
                    for byte in Sha256::digest(&bytes) {
                        write!(&mut digest, "{byte:02x}")
                            .expect("Writing to a String cannot fail.");
                    }
                    let owner_key = (label.clone(), name.to_owned());
                    result
                        .fingerprints
                        .insert(owner_key.clone(), digest.clone());
                    result
                        .artifacts
                        .entry(owner_key)
                        .or_default()
                        .insert(canonical(jar)?);
                    let candidates = source_candidates(&declaration.inputs, node);
                    let key = (name.to_owned(), digest);
                    result
                        .identity_owners
                        .entry(key.clone())
                        .or_default()
                        .insert(label.clone());
                    result
                        .sources
                        .entry(key.clone())
                        .or_default()
                        .extend(candidates.iter().cloned());
                    if candidates.len() != 1 {
                        result.uncertain.insert(key);
                    }
                }
            }
        }
        Ok(result)
    }
}

struct OwnerSources<'a> {
    declaration: &'a Declaration,
    identities: &'a ClassIdentities,
    eligible: &'a BTreeSet<String>,
    root: &'a Path,
    label: &'a str,
    report: &'a Path,
}

type BranchLines = BTreeMap<i64, (i64, i64)>;
#[derive(Default)]
struct Analysis {
    limits: Vec<Value>,
    observed: BTreeMap<String, BTreeMap<i64, u8>>,
    branches: BTreeMap<String, BTreeMap<String, BranchLines>>,
    branch_order: Vec<String>,
    profile_info: BTreeMap<String, Value>,
    matched_classes: Vec<Value>,
    mappings: Vec<Value>,
    empty_sources: Vec<Value>,
    ambiguous_sources: BTreeSet<String>,
    unmatched: BTreeMap<String, BTreeSet<String>>,
}

/// Generate the JVM report through the existing command contract.
///
/// # Errors
/// Return an error for invalid required evidence or a failed analyzer.
pub fn run(
    inventory_path: &Path,
    events_path: &Path,
    report_path: &Path,
    evidence_path: &Path,
    analyzer: &Path,
) -> Result<()> {
    let root = std::env::var_os("BUILD_WORKSPACE_DIRECTORY")
        .map_or_else(std::env::current_dir, |path| Ok(PathBuf::from(path)))?;
    let root = lexical_path(&root);
    let evidence = lexical_path(&root.join(evidence_path));
    match fs::create_dir(&evidence) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && evidence.is_dir() => (),
        Err(error) => return Err(error.into()),
    }
    match fs::remove_file(evidence.join("receipt.json")) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let inventory: Inventory = serde_json::from_str(&read_text(root.join(inventory_path))?)?;
    let events = BuildEvents::parse(&read_text(root.join(events_path))?)?;
    let roots: BTreeSet<_> = inventory
        .coverage_roots
        .iter()
        .filter(|label| label.ends_with(".jvm_coverage"))
        .cloned()
        .collect();
    let declarations = read_declarations(&events, &roots, &root)?;
    let eligible: BTreeSet<_> = inventory
        .eligible
        .iter()
        .filter(|source| source.language == "jvm")
        .map(|s| s.path.clone())
        .collect();
    let mut analysis = Analysis {
        limits: source_limits(&inventory, &declarations, &root, &evidence, inventory_path)?,
        ..Analysis::default()
    };
    let identities = ClassIdentities::collect(&declarations)?;
    let test_inputs = configured_tests(&events, &root, &evidence, &inventory)?;
    let executed: BTreeSet<_> = test_inputs
        .iter()
        .filter(|(_, data)| {
            data["sources"].as_array().is_some_and(|sources| {
                sources
                    .iter()
                    .any(|source| source.as_str().is_some_and(|s| eligible.contains(s)))
            })
        })
        .map(|(label, _)| label.clone())
        .collect();
    let (profiles, execution) = execution_profiles(&events, &root, &executed)?;
    let retained_profiles = retain_profiles(&profiles, &evidence)?;
    for (index, (label, declaration)) in declarations.iter().enumerate() {
        analyze_owner(
            &mut analysis,
            &identities,
            &inventory,
            &eligible,
            &root,
            &evidence,
            analyzer,
            &profiles,
            index,
            label,
            declaration,
        )?;
    }
    for (name, owners) in &analysis.unmatched {
        if identities.owners.get(name) == Some(owners) {
            return Err(invalid(format!(
                "No configured class matches JaCoCo execution data: {name}"
            )));
        }
    }
    validate_profiles(&analysis, &execution, &test_inputs, &executed, &root)?;
    for source in &analysis.ambiguous_sources {
        analysis.observed.remove(source);
        analysis.branches.remove(source);
    }
    let (branch_totals, owner_branch_totals) = analysis.branch_totals();
    let report = lexical_path(&root.join(report_path));
    let combined = analysis.combined_report(&read_text(&report)?)?;
    write_json(
        &evidence.join("receipt.json"),
        &json!({
            "roots": roots,
            "sources": analysis.observed.iter().map(|(name,lines)| (name.clone(),lines.keys().copied().collect::<Vec<_>>())).collect::<BTreeMap<_, _>>(),
            "limits": analysis.limits,
            "mappings": analysis.mappings,
            "execution": execution,
            "test_inputs": test_inputs,
            "profiles": retained_profiles,
            "empty_sources": analysis.empty_sources,
            "branch_totals": branch_totals,
            "owner_branch_totals": owner_branch_totals,
            "profile_info": analysis.profile_info,
            "matched_classes": analysis.matched_classes
        }),
    )?;
    fs::write(report, combined)?;
    Ok(())
}

fn retain_profiles(profiles: &[PathBuf], evidence: &Path) -> Result<Vec<Value>> {
    let mut retained_profiles = Vec::new();
    for (index, profile) in profiles.iter().enumerate() {
        let retained = evidence.join(format!("profile-{index}.exec"));
        fs::copy(profile, &retained)?;
        retained_profiles.push(json!({"original":profile,"retained":retained}));
    }
    Ok(retained_profiles)
}

fn read_declarations(
    events: &BuildEvents,
    roots: &BTreeSet<String>,
    root: &Path,
) -> Result<BTreeMap<String, Declaration>> {
    let mut declarations = BTreeMap::new();
    for selected in events.baselines(
        roots,
        "jvm_coverage_baselines",
        root,
        "JVM",
        ArtifactPolicy::JvmLocal,
    )? {
        let label = selected.identity.label;
        let metadata: Vec<_> = selected
            .paths
            .iter()
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .collect();
        let baselines: Vec<_> = selected
            .paths
            .iter()
            .filter(|path| path.extension().is_some_and(|e| e == "xml"))
            .collect();
        if metadata.len() != 1 || baselines.len() != 1 {
            return Err(invalid(format!(
                "Missing required JVM baseline artifacts: {label}"
            )));
        }
        let data = read_json(metadata[0])?;
        let inputs: Inputs = serde_json::from_value(data.clone())?;
        let jars: Vec<_> = inputs
            .jars
            .iter()
            .map(|path| lexical_path(&root.join(path)))
            .collect();
        if (!inputs.sources.is_empty() && jars.is_empty())
            || jars
                .iter()
                .any(|jar| !jar.is_file() || jar.metadata().map_or(true, |m| m.len() == 0))
        {
            return Err(invalid(format!(
                "Missing required JVM class input: {label}"
            )));
        }
        let baseline = Xml::parse(baselines[0])?;
        if baseline.tag != "report" {
            return Err(invalid(format!(
                "Invalid required JaCoCo baseline: {label}"
            )));
        }
        let order = declarations
            .get(&label)
            .map_or(declarations.len(), |declaration: &Declaration| {
                declaration.order
            });
        declarations.insert(
            label,
            Declaration {
                order,
                data,
                inputs,
                baseline_path: baselines[0].clone(),
                baseline,
                jars,
            },
        );
    }
    Ok(declarations)
}

fn source_limits(
    inventory: &Inventory,
    declarations: &BTreeMap<String, Declaration>,
    root: &Path,
    evidence: &Path,
    inventory_path: &Path,
) -> Result<Vec<Value>> {
    let mut limits = Vec::new();
    for source in &inventory.eligible {
        if source.language != "jvm" {
            continue;
        }
        let mut configured: Vec<_> = declarations
            .iter()
            .enumerate()
            .filter(|(_, (label, _))| {
                inventory
                    .jvm_source_declarations
                    .get(*label)
                    .is_some_and(|sources| sources.contains(&source.path))
            })
            .collect();
        if configured.is_empty()
            && !declarations
                .values()
                .any(|d| d.inputs.sources.contains(&source.path))
        {
            return Err(invalid(format!(
                "JVM source has no configured analysis root: {}",
                source.path
            )));
        }
        configured.sort_by_key(|(_, (_, declaration))| declaration.order);
        for (index, (label, declaration)) in configured {
            if !declaration.inputs.sources.contains(&source.path) {
                limits.push(json!({
                    "source": source.path,
                    "rule": label,
                    "evidence": [lexical_path(&root.join(inventory_path)), evidence.join(format!("inputs-{index}.json"))],
                    "record_absence": true,
                    "missing_guarantee": "The configured source selection excludes this inventoried source."
                }));
            }
        }
    }
    Ok(limits)
}

fn validate_profiles(
    analysis: &Analysis,
    execution: &[Value],
    test_inputs: &BTreeMap<String, Value>,
    executed: &BTreeSet<String>,
    root: &Path,
) -> Result<()> {
    for result in execution {
        let Some(label) = result["rule"]
            .as_str()
            .filter(|label| executed.contains(*label))
        else {
            continue;
        };
        for profile in result["profiles"]
            .as_array()
            .ok_or_else(|| invalid("Invalid JVM profiles."))?
        {
            let profile = profile
                .as_str()
                .ok_or_else(|| invalid("Invalid JVM profile."))?;
            let info = analysis
                .profile_info
                .get(profile)
                .ok_or_else(|| invalid(format!("Missing JaCoCo profile metadata: {profile}")))?;
            if info["sessions"] == 0 && info["classes"] == 0 {
                return Err(invalid(format!(
                    "Incomplete required JVM profile: {label}: {profile}"
                )));
            }
            if info["classes"] == 0 {
                continue;
            }
            let mut jars = BTreeSet::new();
            for jar in test_inputs[label]["jars"]
                .as_array()
                .ok_or_else(|| invalid("Invalid configured JVM test jars."))?
            {
                // A missing jar cannot match an existing class artifact.
                match fs::canonicalize(root.join(jar.as_str().unwrap_or(""))) {
                    Ok(path) => {
                        jars.insert(path.to_string_lossy().into_owned());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
            if !analysis.matched_classes.iter().any(|item| {
                item["profile"] == profile
                    && item["jars"].as_array().is_some_and(|values| {
                        values
                            .iter()
                            .any(|jar| jar.as_str().is_some_and(|jar| jars.contains(jar)))
                    })
            }) {
                return Err(invalid(format!(
                    "No required JVM profile matches a configured class: {label}: {profile}"
                )));
            }
        }
    }
    Ok(())
}

fn configured_tests(
    events: &BuildEvents,
    root: &Path,
    evidence: &Path,
    inventory: &Inventory,
) -> Result<BTreeMap<String, Value>> {
    let mut candidates = inventory.jvm_tests.clone();
    for source in &inventory.eligible {
        if source.language == "jvm" {
            candidates.extend(
                source
                    .owners
                    .iter()
                    .filter(|owner| matches!(owner.kind.as_str(), "java_test" | "kt_jvm_test"))
                    .map(|owner| owner.label.clone()),
            );
        }
    }
    let mut inputs = BTreeMap::new();
    for selected in events.jvm_inputs(root)? {
        let label = selected.identity.label;
        if selected.paths.len() != 1 {
            return Err(invalid(format!(
                "Missing configured JVM test inputs: {label}"
            )));
        }
        let mut data = read_json(&selected.paths[0])?;
        let decoded = serde_json::from_value::<Inputs>(data.clone());
        if data["rule"] != label
            || decoded
                .as_ref()
                .map_or(true, |d| !d.sources.is_empty() && d.jars.is_empty())
        {
            return Err(invalid(format!(
                "Invalid configured JVM test inputs: {label}"
            )));
        }
        let retained = evidence.join(format!("test-inputs-{}.json", inputs.len()));
        fs::copy(&selected.paths[0], &retained)?;
        data["evidence"] = json!(retained);
        inputs.insert(label, data);
    }
    let present: BTreeSet<_> = inputs.keys().cloned().collect();
    let required: BTreeSet<_> = events
        .test_labels()
        .intersection(&candidates)
        .cloned()
        .collect();
    let missing: Vec<_> = required.difference(&present).collect();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "Missing required configured JVM test inputs: {missing:?}"
        )));
    }
    Ok(inputs)
}

fn execution_profiles(
    events: &BuildEvents,
    root: &Path,
    executed: &BTreeSet<String>,
) -> Result<(Vec<PathBuf>, Vec<Value>)> {
    let mut profiles = BTreeSet::new();
    let mut execution = Vec::new();
    for result in events.jvm_test_results(root)? {
        let required = result
            .label
            .as_str()
            .is_some_and(|label| executed.contains(label));
        let label = result.label.as_str().unwrap_or("None");
        if result.logs.len() != 1 {
            if required {
                return Err(invalid(format!("Missing JVM execution outputs: {label}")));
            }
            continue;
        }
        let directory = result.logs[0]
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("_coverage");
        let found: Vec<_> = if directory.is_dir() {
            fs::read_dir(directory)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e == "exec"))
                .collect()
        } else {
            Vec::new()
        };
        let mut nonempty = Vec::new();
        let mut empty = Vec::new();
        for profile in found {
            if profile.metadata()?.len() > 0 {
                nonempty.push(profile);
            } else {
                empty.push(profile);
            }
        }
        if nonempty.is_empty() {
            if required {
                return Err(invalid(format!(
                    "Missing or empty required JVM profiles: {label}"
                )));
            }
            continue;
        }
        profiles.extend(nonempty.iter().cloned());
        execution.push(json!({"rule":result.label,"profiles":nonempty,"empty_profiles":empty,"cached":result.cached,"execution":result.execution}));
    }
    Ok((profiles.into_iter().collect(), execution))
}

#[expect(
    clippy::too_many_arguments,
    reason = "The analyzer needs the configured owner and retained evidence context."
)]
fn analyze_owner(
    analysis: &mut Analysis,
    identities: &ClassIdentities,
    inventory: &Inventory,
    eligible: &BTreeSet<String>,
    root: &Path,
    evidence: &Path,
    analyzer: &Path,
    profiles: &[PathBuf],
    index: usize,
    label: &str,
    declaration: &Declaration,
) -> Result<()> {
    let report = evidence.join(format!("analysis-{index}.xml"));
    for child in &declaration.inputs.children {
        for source in &inventory.eligible {
            if source.language == "jvm" && source.owners.iter().any(|o| &o.label == child) {
                analysis.limits.push(json!({
                    "source": source.path,
                    "rule": child,
                    "collection_rule": label,
                    "evidence": evidence.join(format!("inputs-{index}.json")),
                    "missing_guarantee": "JaCoCo profiles cannot distinguish an unexecuted child JVM from a child whose complete profile disappeared."
                }));
            }
        }
    }
    fs::copy(
        &declaration.baseline_path,
        evidence.join(format!("baseline-{index}.xml")),
    )?;
    write_json(
        &evidence.join(format!("inputs-{index}.json")),
        &declaration.data,
    )?;
    for (jar_index, jar) in declaration.jars.iter().enumerate() {
        fs::copy(jar, evidence.join(format!("class-{index}-{jar_index}.jar")))?;
    }
    let mut command = Command::new(fs::canonicalize(root.join(analyzer))?);
    command.arg(&report).current_dir(root);
    for jar in &declaration.jars {
        command.arg("--classfiles").arg(jar);
    }
    for profile in profiles {
        command.arg("--exec").arg(profile);
    }
    for (name, owners) in &identities.owners {
        if owners.len() > 1 {
            command.arg("--allow-class-name").arg(name);
        }
    }
    let result = command.output()?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    fs::write(
        evidence.join(format!("analysis-{index}.log")),
        [result.stdout.as_slice(), result.stderr.as_slice()].concat(),
    )?;
    if !result.status.success() {
        return Err(invalid(format!(
            "JaCoCo analyzer failed: {}",
            result.status
        )));
    }
    analysis.analyzer_metadata(&stdout, profiles, label, declaration, identities)?;
    let document = Xml::parse(&report)?;
    if document.tag != "report" {
        return Err(invalid(format!(
            "Invalid required JaCoCo analysis: {label}"
        )));
    }
    let class_names = |xml: &Xml| {
        xml.classes()
            .map(|n| n.attribute("name").map(str::to_owned))
            .collect::<Result<BTreeSet<_>>>()
    };
    if class_names(&declaration.baseline)? != class_names(&document)? {
        return Err(invalid(format!(
            "Incomplete JaCoCo class analysis: {label}"
        )));
    }
    if declaration.baseline.source_lines()? != document.source_lines()? {
        return Err(invalid(format!(
            "Incomplete JaCoCo source analysis: {label}"
        )));
    }
    analysis.map_owner(
        &document,
        &OwnerSources {
            declaration,
            identities,
            eligible,
            root,
            label,
            report: &report,
        },
    )
}

impl Analysis {
    fn analyzer_metadata(
        &mut self,
        stdout: &str,
        profiles: &[PathBuf],
        label: &str,
        declaration: &Declaration,
        identities: &ClassIdentities,
    ) -> Result<()> {
        for line in stdout.lines() {
            if let Some(fields) = line.strip_prefix("JACOCO_PROFILE ") {
                let fields: Vec<_> = fields.split_whitespace().collect();
                if fields.len() != 3 {
                    return Err(invalid("Invalid JaCoCo profile metadata."));
                }
                let index = fields[0]
                    .parse::<usize>()
                    .map_err(|e| invalid(e.to_string()))?;
                let profile = profiles
                    .get(index)
                    .ok_or_else(|| invalid("Invalid JaCoCo profile index."))?
                    .to_string_lossy()
                    .into_owned();
                let info = json!({"sessions":integer(fields[1])?,"classes":integer(fields[2])?});
                if self
                    .profile_info
                    .get(&profile)
                    .is_some_and(|previous| previous != &info)
                {
                    return Err(invalid(format!(
                        "Conflicting JaCoCo profile metadata: {profile}"
                    )));
                }
                self.profile_info.insert(profile, info);
            } else if let Some(fields) = line.strip_prefix("JACOCO_MATCH ") {
                let fields: Vec<_> = fields.splitn(3, ' ').collect();
                if fields.len() != 3 {
                    return Err(invalid("Invalid JaCoCo class match."));
                }
                let index = fields[0]
                    .parse::<usize>()
                    .map_err(|e| invalid(e.to_string()))?;
                let name = fields[2];
                let node = declaration
                    .baseline
                    .classes()
                    .find(|n| n.attributes.get("name").is_some_and(|n| n == name))
                    .ok_or_else(|| invalid("Missing matched JaCoCo class."))?;
                self.matched_classes.push(json!({
                    "profile": profiles.get(index).ok_or_else(|| invalid("Invalid JaCoCo profile index."))?,
                    "rule": label,
                    "class": name,
                    "id": fields[1],
                    "jars": identities.artifacts.get(&(label.to_owned(),name.to_owned())).cloned().unwrap_or_default(),
                    "sources": source_candidates(&declaration.inputs,node)
                }));
            } else if let Some(name) = line.strip_prefix("JACOCO_NO_MATCH ") {
                self.unmatched
                    .entry(name.to_owned())
                    .or_default()
                    .insert(label.to_owned());
            }
        }
        Ok(())
    }

    fn class_ownership_limits(
        &mut self,
        document: &Xml,
        identities: &ClassIdentities,
        label: &str,
        report: &Path,
    ) -> Result<()> {
        for node in document.classes() {
            let name = node.attribute("name")?;
            let digest = identities
                .fingerprints
                .get(&(label.to_owned(), name.to_owned()));
            let key = (name.to_owned(), digest.cloned().unwrap_or_default());
            let candidates = identities.sources.get(&key).cloned().unwrap_or_default();
            let ambiguous = candidates.len() > 1
                || (identities.uncertain.contains(&key)
                    && identities
                        .identity_owners
                        .get(&key)
                        .is_some_and(|owners| owners.len() > 1));
            let mut covered = false;
            for counter in node.children("counter") {
                if counter
                    .attributes
                    .get("type")
                    .is_some_and(|kind| kind == "INSTRUCTION")
                    && integer(
                        counter
                            .attributes
                            .get("covered")
                            .map_or("0", String::as_str),
                    )? > 0
                {
                    covered = true;
                }
            }
            if ambiguous && covered {
                self.ambiguous_sources.extend(candidates.iter().cloned());
                for source in candidates {
                    self.limits.push(json!({
                        "source": source,
                        "rule": label,
                        "class": name,
                        "sha256": digest,
                        "evidence": report,
                        "record_absence": true,
                        "missing_guarantee": "A JVM class identity is shared by distinct original sources. JaCoCo cannot identify the executed owner."
                    }));
                }
            }
        }
        Ok(())
    }

    fn map_owner(&mut self, document: &Xml, owner: &OwnerSources<'_>) -> Result<()> {
        let OwnerSources {
            declaration,
            identities,
            eligible,
            label,
            report,
            ..
        } = *owner;
        self.class_ownership_limits(document, identities, label, report)?;
        let mut by_name: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for name in &declaration.inputs.sources {
            by_name.entry(filename(name)).or_default().push(name);
        }
        let mut mapped = BTreeSet::new();
        for package in document.children("package") {
            for source in package.children("sourcefile") {
                let candidates = by_name
                    .get(source.attribute("name")?)
                    .cloned()
                    .unwrap_or_default();
                mapped.extend(candidates.iter().map(|s| (*s).to_owned()));
                if candidates.len() != 1 {
                    for name in candidates {
                        self.limits.push(json!({
                            "source": name,
                            "rule": label,
                            "evidence": report,
                            "record_absence": true,
                            "missing_guarantee": "JaCoCo source identity is ambiguous within this configured owner."
                        }));
                    }
                    continue;
                }
                let name = candidates[0];
                if !eligible.contains(name) {
                    continue;
                }
                self.record_source(source, package, name, owner)?;
            }
        }
        for name in declaration
            .inputs
            .sources
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .difference(&mapped)
        {
            if !eligible.contains(name) {
                continue;
            }
            if declaration
                .baseline
                .classes()
                .all(|node| node.attributes.contains_key("sourcefilename"))
            {
                self.observed.entry(name.clone()).or_default();
                self.empty_sources.push(json!({"source":name,"rule":label,"evidence":declaration.baseline_path,"diagnostic":"The configured compiler output contains no class for this source."}));
            } else {
                self.limits.push(json!({
                    "source": name,
                    "rule": label,
                    "evidence": declaration.baseline_path,
                    "record_absence": true,
                    "missing_guarantee": "The configured class analysis supplies no unique source record. This is not measured zero coverage."
                }));
            }
        }
        Ok(())
    }

    fn record_source(
        &mut self,
        source: &Xml,
        package: &Xml,
        name: &str,
        owner: &OwnerSources<'_>,
    ) -> Result<()> {
        let OwnerSources {
            declaration,
            root,
            label,
            report,
            ..
        } = *owner;
        self.observed.entry(name.to_owned()).or_default();
        if !self.branches.contains_key(name) {
            self.branch_order.push(name.to_owned());
        }
        self.branches
            .entry(name.to_owned())
            .or_default()
            .entry(label.to_owned())
            .or_default();
        let count = i64::try_from(
            read_text(root.join(name))?
                .split_terminator([
                    '\n', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
                    '\u{2028}', '\u{2029}',
                ])
                .count(),
        )
        .map_err(|error| invalid(error.to_string()))?;
        for line in source.children("line") {
            let number = integer(line.attribute("nr")?)?;
            let missed = integer(line.attribute("mi")?)?;
            let covered = integer(line.attribute("ci")?)?;
            let mb = integer(line.attribute("mb")?)?;
            let cb = integer(line.attribute("cb")?)?;
            if number <= 0 || [missed, covered, mb, cb].into_iter().any(|n| n < 0) {
                return Err(invalid(format!("Invalid JaCoCo line: {name}")));
            }
            if number > count {
                self.limits.push(json!({"source":name,"rule":label,"evidence":report,"missing_guarantee":format!("JaCoCo reports compiler-mapped line {number} beyond the original source.")}));
                continue;
            }
            if missed + covered > 0 {
                let hit = self
                    .observed
                    .entry(name.to_owned())
                    .or_default()
                    .entry(number)
                    .or_default();
                *hit = (*hit).max(u8::from(covered > 0));
            }
            if mb + cb > 0 {
                self.branches
                    .entry(name.to_owned())
                    .or_default()
                    .entry(label.to_owned())
                    .or_default()
                    .insert(number, (mb, cb));
            }
        }
        let classes: Vec<_> = package
            .children("class")
            .filter(|class| class.attributes.get("sourcefilename") == source.attributes.get("name"))
            .map(|class| class.attribute("name").map(str::to_owned))
            .collect::<Result<_>>()?;
        self.mappings.push(json!({"source":name,"rule":label,"classes":classes,"baseline":declaration.baseline_path,"analysis":report}));
        if Path::new(name)
            .extension()
            .is_some_and(|extension| extension == "kt")
        {
            self.limits.push(json!({
                        "source": name,
                        "rule": label,
                        "evidence": report,
                        "missing_guarantee": "Standard JaCoCo Kotlin filters and compiler mappings do not establish original inline branch identities."
                    }));
        }
        Ok(())
    }

    fn branch_totals(&mut self) -> (Value, Value) {
        let mut totals = serde_json::Map::new();
        let mut owners = BTreeMap::<String, serde_json::Map<String, Value>>::new();
        for name in &self.branch_order {
            let Some(states) = self.branches.get(name) else {
                continue;
            };
            let total = |lines: &BranchLines| json!({"missed":lines.values().map(|v| v.0).sum::<i64>(),"covered":lines.values().map(|v| v.1).sum::<i64>()});
            for (label, lines) in states {
                owners
                    .entry(label.clone())
                    .or_default()
                    .insert(name.clone(), total(lines));
            }
            if let Some(first) = states.values().next() {
                if states.values().all(|lines| lines == first) {
                    totals.insert(name.clone(), total(first));
                } else {
                    self.limits.push(json!({
                        "source": name,
                        "rules": states.keys().collect::<Vec<_>>(),
                        "evidence": self.mappings.iter().filter(|m| m["source"]==*name).map(|m| m["analysis"].clone()).collect::<Vec<_>>(),
                        "missing_guarantee": "Configured owners report different branch states. Per-owner totals remain separate because JaCoCo supplies no common original branch identities."
                    }));
                }
            }
        }
        (Value::Object(totals), json!(owners))
    }

    fn combined_report(&mut self, original: &str) -> Result<String> {
        let records: Vec<_> = original.split("end_of_record").collect();
        if records.len() < 2 || !records.last().unwrap_or(&"").trim().is_empty() {
            return Err(invalid(
                "The combined coverage report is empty or incomplete.",
            ));
        }
        let mut retained = String::new();
        for record in &records[..records.len() - 1] {
            let sources: Vec<_> = record
                .lines()
                .filter_map(|line| line.strip_prefix("SF:"))
                .collect();
            if sources.len() != 1 {
                return Err(invalid("A combined record must identify one source."));
            }
            if ![".java", ".kt", ".kts", ".clj"]
                .iter()
                .any(|suffix| sources[0].ends_with(suffix))
            {
                retained.push_str(record);
                retained.push_str("end_of_record\n");
            }
        }
        for (name, lines) in &self.observed {
            writeln!(retained, "SF:{name}").expect("Writing to a String cannot fail.");
            for (number, hit) in lines {
                writeln!(retained, "DA:{number},{hit}").expect("Writing to a String cannot fail.");
            }
            writeln!(
                retained,
                "LF:{}\nLH:{}",
                lines.len(),
                lines.values().map(|hit| usize::from(*hit)).sum::<usize>()
            )
            .expect("Writing to a String cannot fail.");
            if self
                .branches
                .get(name)
                .is_some_and(|owners| owners.values().any(|lines| !lines.is_empty()))
            {
                self.limits.push(json!({
                    "source": name,
                    "evidence": self.mappings.iter().filter(|m| m["source"]==*name).map(|m| m["analysis"].clone()).collect::<Vec<_>>(),
                    "rules": self.mappings.iter().filter(|m| m["source"]==*name).filter_map(|m| m["rule"].as_str()).collect::<BTreeSet<_>>(),
                    "missing_guarantee": "Standard JaCoCo reports branch totals for each line without individual branch identities or execution counts."
                }));
            }
            retained.push_str("end_of_record\n");
        }
        Ok(retained)
    }
}
