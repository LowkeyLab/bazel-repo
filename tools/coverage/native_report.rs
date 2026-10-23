//! Apply Go and Rust baseline requirements to a combined LCOV report.
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Value, json};

use crate::build_event::{ArtifactPolicy, BuildEvents};
use crate::{Result, invalid};

/// LCOV integers have no machine-word limit in the original command.
#[derive(Debug, Clone, Eq, PartialEq)]
struct Natural(String);

impl Natural {
    fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let (negative, digits) = match text.chars().next() {
            Some('-') => (true, &text[1..]),
            Some('+') => (false, &text[1..]),
            _ => (false, text),
        };
        let bytes = digits.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.iter().enumerate().all(|(index, byte)| {
                byte.is_ascii_digit()
                    || (*byte == b'_'
                        && index > 0
                        && index + 1 < bytes.len()
                        && bytes[index - 1].is_ascii_digit()
                        && bytes[index + 1].is_ascii_digit())
            });
        let digits = digits.replace('_', "");
        let digits = digits.trim_start_matches('0');
        if !valid || (negative && !digits.is_empty()) {
            return Err(invalid(format!(
                "invalid literal for nonnegative int: '{text}'"
            )));
        }
        Ok(Self(if digits.is_empty() {
            "0".to_owned()
        } else {
            digits.to_owned()
        }))
    }
    fn count(value: usize) -> Self {
        Self(value.to_string())
    }
    fn is_zero(&self) -> bool {
        self.0 == "0"
    }
}
impl Ord for Natural {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}
impl PartialOrd for Natural {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Serialize for Natural {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        // RawValue preserves the original command's arbitrary-size integer representation.
        serde_json::value::RawValue::from_string(self.0.clone())
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

type Lines = BTreeSet<Natural>;

#[derive(Deserialize)]
struct Inventory {
    coverage_roots: Vec<String>,
    excluded: Vec<Excluded>,
    #[serde(default)]
    eligible: Vec<Source>,
}
#[derive(Deserialize)]
struct Excluded {
    path: String,
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
}
#[derive(Deserialize)]
struct RustObject {
    object: String,
    no_mapping: bool,
    diagnostic: String,
    generated_sources: Vec<String>,
    children: Vec<[String; 3]>,
    llvm_cov: String,
    llvm_profdata: String,
}
impl RustObject {
    fn read(path: &Path) -> Result<Self> {
        let text = read_text(path)?;
        let data: Self = serde_json::from_str(&text)
            .map_err(|_| invalid(format!("Invalid Rust object evidence: {}", path.display())))?;
        if data.object.is_empty()
            || data.llvm_cov.is_empty()
            || data.llvm_profdata.is_empty()
            || data.children.iter().flatten().any(String::is_empty)
            || (data.no_mapping && !data.diagnostic.contains("no coverage data found"))
        {
            return Err(invalid(format!(
                "Invalid Rust object evidence: {}",
                path.display()
            )));
        }
        Ok(data)
    }
}

fn read_text(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn lcov_lines(lines: &[&str], name: &str, family: &str, check_summary: bool) -> Result<Lines> {
    let mut counts = BTreeMap::new();
    let mut found = Vec::new();
    let mut hit = Vec::new();
    for line in lines {
        if let Some(fields) = line.strip_prefix("DA:") {
            let fields: Vec<_> = fields.split(',').collect();
            if !(2..=3).contains(&fields.len()) {
                return Err(invalid(format!("Invalid {family} line record: {name}")));
            }
            let number = Natural::parse(fields[0])
                .map_err(|_| invalid(format!("Invalid {family} line record: {name}")))?;
            let count = Natural::parse(fields[1])
                .map_err(|_| invalid(format!("Invalid {family} line record: {name}")))?;
            if number.is_zero() || counts.insert(number, count).is_some() {
                return Err(invalid(format!("Invalid {family} line record: {name}")));
            }
        } else if let Some(number) = line.strip_prefix("LF:") {
            found.push(Natural::parse(number)?);
        } else if let Some(number) = line.strip_prefix("LH:") {
            hit.push(Natural::parse(number)?);
        }
    }
    let count = Natural::count(counts.len());
    if found.len() != 1
        || hit.len() != 1
        || found[0] < count
        || hit[0] > found[0]
        || (check_summary
            && (found[0] != count
                || hit[0]
                    != Natural::count(counts.values().filter(|count| !count.is_zero()).count())))
    {
        return Err(invalid(format!(
            "Incomplete {family} coverage record: {name}"
        )));
    }
    Ok(counts.into_keys().collect())
}

struct Baselines {
    expected: BTreeMap<String, Lines>,
    excluded: BTreeSet<String>,
    limits: Vec<Value>,
    roots: BTreeSet<String>,
}

impl Baselines {
    fn rust_metadata(
        &mut self,
        label: &str,
        paths: &mut Vec<PathBuf>,
    ) -> Result<(PathBuf, RustObject)> {
        let metadata_paths: Vec<_> = paths
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        if metadata_paths.len() != 1 {
            return Err(invalid(format!("Missing Rust object evidence: {label}")));
        }
        let path = metadata_paths[0].clone();
        let data = RustObject::read(&path)?;
        self.excluded.extend(data.generated_sources.iter().cloned());
        for [source, child_rule, child_object] in &data.children {
            self.limits.push(json!({
                        "source":source,"rule":child_rule,"object":child_object,"collection_rule":label,
                        "missing_guarantee":"LLVM profiles cannot distinguish an unexecuted child from a child whose complete profile disappeared.",
                        "evidence":path,"diagnostic":"The declared child object does not prove execution. Available profile IDs establish only observed executions."
                    }));
        }
        paths.retain(|path| path.extension().is_some_and(|ext| ext == "lcov"));
        if paths.len() != 1 {
            return Err(invalid(format!("Missing Rust baseline artifact: {label}")));
        }
        Ok((path, data))
    }

    fn read_artifact(
        &mut self,
        path: &Path,
        label: &str,
        language: &str,
        metadata: Option<&RustObject>,
        mapped: &mut BTreeSet<String>,
    ) -> Result<()> {
        let family = if language == "rust" { "Rust" } else { "Go" };
        let extension = if language == "rust" { ".rs" } else { ".go" };
        let text = read_text(path)?;
        if text.trim().is_empty() {
            if metadata.is_some_and(|data| !data.no_mapping) {
                return Err(invalid(format!("Empty required Rust baseline: {label}")));
            }
            return Ok(());
        }
        if metadata.is_some_and(|data| data.no_mapping) {
            return Err(invalid(format!(
                "Conflicting Rust mapping evidence: {label}"
            )));
        }
        let records: Vec<_> = text.split("end_of_record").collect();
        if records.len() < 2 || !records[records.len() - 1].trim().is_empty() {
            return Err(invalid(format!(
                "Incomplete {family} baseline artifact: {}",
                path.display()
            )));
        }
        for record in &records[..records.len() - 1] {
            let lines: Vec<_> = record.lines().collect();
            let sources: Vec<_> = lines
                .iter()
                .filter_map(|line| line.strip_prefix("SF:"))
                .collect();
            if sources.len() != 1 || !sources[0].ends_with(extension) {
                return Err(invalid(format!(
                    "Invalid {family} baseline source: {}",
                    path.display()
                )));
            }
            let name = sources[0];
            mapped.insert(name.to_owned());
            if metadata.is_some() && self.excluded.contains(name) {
                continue;
            }
            let numbers = lcov_lines(&lines, name, family, language != "rust")?;
            if let Some(data) = metadata {
                let found = lines
                    .iter()
                    .find_map(|line| line.strip_prefix("LF:"))
                    .ok_or_else(|| invalid("Missing LF record."))?;
                if Natural::parse(found)? > Natural::count(numbers.len()) {
                    self.limits.push(json!({"source":name,"rule":label,"object":data.object,"line_summary":true,
                                "missing_guarantee":"LLVM LCOV does not identify every summarized executable line with a DA record.",
                                "evidence":path,"diagnostic":"The LLVM LF summary exceeds its exported DA records."}));
                }
            }
            if !self.excluded.contains(name) {
                self.expected
                    .entry(name.to_owned())
                    .or_default()
                    .extend(numbers);
            }
        }
        Ok(())
    }

    fn read(
        inventory: &Inventory,
        events_path: &Path,
        root: &Path,
        language: &str,
    ) -> Result<Self> {
        let family = if language == "rust" { "Rust" } else { "Go" };
        let extension = if language == "rust" { ".rs" } else { ".go" };
        let mut baseline = Self {
            expected: BTreeMap::new(),
            excluded: inventory
                .excluded
                .iter()
                .filter(|source| source.path.ends_with(extension))
                .map(|source| source.path.clone())
                .collect(),
            limits: Vec::new(),
            roots: inventory
                .coverage_roots
                .iter()
                .filter(|label| label.ends_with(&format!(".{language}_coverage")))
                .cloned()
                .collect(),
        };
        let events = BuildEvents::parse(&read_text(events_path)?)?;
        for artifacts in events.baselines(
            &baseline.roots,
            &format!("{language}_coverage_baselines"),
            root,
            family,
            ArtifactPolicy::WorkspaceIntegrity,
        )? {
            let label = artifacts.identity.label;
            let mut paths = artifacts.paths;
            let metadata = if language == "rust" {
                let (path, data) = baseline.rust_metadata(&label, &mut paths)?;
                Some((path, data))
            } else {
                None
            };
            let mut mapped = BTreeSet::new();
            for path in &paths {
                baseline.read_artifact(
                    path,
                    &label,
                    language,
                    metadata.as_ref().map(|(_, data)| data),
                    &mut mapped,
                )?;
            }
            let declarations: BTreeSet<_> = inventory
                .eligible
                .iter()
                .filter(|source| {
                    source.language == language
                        && source.owners.iter().any(|owner| owner.label == label)
                })
                .map(|source| source.path.clone())
                .collect();
            for source in declarations.difference(&mapped) {
                if let Some((path, data)) = &metadata {
                    baseline.limits.push(json!({"source":source,"rule":label,"object":data.object,"record_absence":true,
                        "missing_guarantee":"LLVM emitted no source mapping in this configured object.","evidence":path,"diagnostic":data.diagnostic}));
                } else {
                    baseline.limits.push(json!({"source":source,"rule":label,"record_absence":true,"evidence":paths,
                        "missing_guarantee":"The configured Go source selection omits this declaration. The native builder emits no baseline record for this configuration."}));
                }
            }
        }
        if language == "rust" {
            baseline
                .expected
                .retain(|name, _| !baseline.excluded.contains(name));
            baseline.limits.retain(|limit| {
                !baseline
                    .excluded
                    .contains(limit["source"].as_str().unwrap_or_default())
            });
        }
        Ok(baseline)
    }
}

/// Validate all required evidence before changing the receipt or combined report.
/// Receipt publication still precedes report replacement, as in the original command.
///
/// # Errors
/// Return an error when input evidence is invalid or a file operation fails.
pub fn run(
    language: &str,
    events: &Path,
    inventory: &Path,
    report: &Path,
    expected_output: Option<&Path>,
) -> Result<()> {
    let family = if language == "rust" { "Rust" } else { "Go" };
    let extension = if language == "rust" { ".rs" } else { ".go" };
    let root = std::env::var_os("BUILD_WORKSPACE_DIRECTORY")
        .map_or(std::env::current_dir()?, PathBuf::from);
    let inventory: Inventory = serde_json::from_str(&read_text(&root.join(inventory))?)?;
    let baseline = Baselines::read(&inventory, &root.join(events), &root, language)?;
    let report = root.join(report);
    let original = read_text(&report)?;
    let records: Vec<_> = original.split("end_of_record").collect();
    if records.len() < 2 || !records[records.len() - 1].trim().is_empty() {
        return Err(invalid("The combined report is empty or incomplete."));
    }
    let mut retained = String::new();
    let mut observed = BTreeMap::new();
    for record in &records[..records.len() - 1] {
        let lines: Vec<_> = record.lines().collect();
        let sources: Vec<_> = lines
            .iter()
            .filter_map(|line| line.strip_prefix("SF:"))
            .collect();
        if sources.len() != 1 {
            return Err(invalid("A coverage record must identify one source."));
        }
        let name = sources[0];
        if baseline.excluded.contains(name) {
            continue;
        }
        if name.ends_with(extension) {
            if observed.contains_key(name) {
                return Err(invalid(format!(
                    "Duplicate {family} coverage source: {name}"
                )));
            }
            observed.insert(name.to_owned(), lcov_lines(&lines, name, family, true)?);
        }
        retained.push_str(record);
        retained.push_str("end_of_record");
    }
    for (name, numbers) in &baseline.expected {
        if !observed
            .get(name)
            .is_some_and(|lines| numbers.is_subset(lines))
        {
            return Err(invalid(format!(
                "Missing {family} baseline source or lines: {name}"
            )));
        }
    }
    if let Some(path) = expected_output {
        #[derive(Serialize)]
        struct Receipt<'a> {
            limits: &'a [Value],
            sources: &'a BTreeMap<String, Lines>,
            roots: &'a BTreeSet<String>,
        }
        let receipt = Receipt {
            limits: &baseline.limits,
            sources: &baseline.expected,
            roots: &baseline.roots,
        };
        std::fs::write(
            root.join(path),
            serde_json::to_string_pretty(&receipt)? + "\n",
        )?;
    }
    retained.push_str(records[records.len() - 1]);
    std::fs::write(report, retained)?;
    Ok(())
}
