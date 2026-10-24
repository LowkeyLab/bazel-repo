//! Validate JavaScript collector evidence and merge original-source reports.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::build_event::{BuildEvents, JavascriptExecution, lexical_path, optional_array, string};
use crate::{Result, invalid};

type Records = Vec<(String, Vec<String>)>;
const EXTENSIONS: &[&str] = &[
    "js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx", "astro",
];
const ORIGINALS: &[&str] = &["ts", "mts", "cts", "tsx", "jsx"];

fn extension(name: &str) -> &str {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
}
fn read_text(path: impl AsRef<Path>) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn read_json(path: impl AsRef<Path>) -> Result<Value> {
    Ok(serde_json::from_str(&read_text(path)?)?)
}
fn directory(path: &Path) -> Result<()> {
    Ok(fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?)
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    Ok(file.write_all(bytes.as_ref())?)
}
fn copy(from: &Path, to: &Path) -> Result<()> {
    write(to, fs::read(from)?)
}
fn json_file(path: &Path, value: &Value) -> Result<()> {
    write(path, serde_json::to_string_pretty(value)? + "\n")
}
fn absolute(root: &Path, path: &Path) -> PathBuf {
    let path = root.join(path);
    if let Ok(resolved) = fs::canonicalize(&path) {
        return resolved;
    }
    // Python Path.resolve also accepts an output that does not exist yet.
    let mut normalized = PathBuf::new();
    for part in path.components() {
        if part == Component::ParentDir {
            normalized.pop();
        } else {
            normalized.push(part);
            if let Ok(resolved) = fs::canonicalize(&normalized) {
                normalized = resolved;
            }
        }
    }
    fs::canonicalize(&normalized).unwrap_or(normalized)
}
fn record_text(records: &Records) -> String {
    records
        .iter()
        .map(|(_, lines)| lines.join("\n") + "\nend_of_record\n")
        .collect()
}
fn contains(records: &Records, name: &str) -> bool {
    records.iter().any(|(key, _)| key == name)
}
fn has_hits(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        let count = match line.split_once(':') {
            Some(("DA", data)) => data.split(',').nth(1),
            Some(("FNDA", data)) => data.split(',').next(),
            Some(("BRDA", data)) => data.rsplit(',').next(),
            _ => None,
        };
        count.is_some_and(|s| s != "-" && s.bytes().any(|b| b.is_ascii_digit() && b != b'0'))
    })
}
fn records(path: &Path, all: bool) -> Result<Records> {
    let text = read_text(path)?;
    let mut parts: Vec<_> = text.split("end_of_record").collect();
    if parts.len() == 1 || !parts.pop().unwrap_or("").trim().is_empty() {
        return Err(invalid(format!(
            "Missing or incomplete JavaScript LCOV: {}",
            path.display()
        )));
    }
    let mut result = Vec::new();
    for part in parts {
        let lines: Vec<String> = part.trim().lines().map(str::to_owned).collect();
        let names: Vec<_> = lines
            .iter()
            .filter_map(|line| line.strip_prefix("SF:"))
            .collect();
        if names.len() != 1 || contains(&result, names[0]) {
            return Err(invalid(format!(
                "Invalid JavaScript source record: {}",
                path.display()
            )));
        }
        let name = names[0].to_owned();
        if all || EXTENSIONS.contains(&extension(&name)) {
            validate_record(path, &name, &lines)?;
        }
        result.push((name, lines));
    }
    Ok(result)
}

#[derive(Deserialize)]
struct NativeProfile {
    result: Vec<Box<serde_json::value::RawValue>>,
}
#[derive(Deserialize)]
struct NativeScript {
    #[serde(rename = "url")]
    _url: String,
    functions: Vec<Box<serde_json::value::RawValue>>,
}
#[derive(Deserialize)]
struct NativeFunction {
    #[serde(rename = "functionName")]
    _name: String,
    ranges: Vec<Box<serde_json::value::RawValue>>,
}
#[derive(Deserialize)]
struct ProfileRange {
    #[serde(rename = "startOffset")]
    start: Box<serde_json::value::RawValue>,
    #[serde(rename = "endOffset")]
    end: Box<serde_json::value::RawValue>,
    count: Box<serde_json::value::RawValue>,
}
fn profile_integer(raw: &serde_json::value::RawValue, message: &str) -> Result<String> {
    let raw = raw.get();
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(message));
    }
    let normalized = digits.trim_start_matches('0');
    if raw.starts_with('-') && !normalized.is_empty() {
        return Err(invalid(message));
    }
    Ok(if normalized.is_empty() {
        "0"
    } else {
        normalized
    }
    .to_owned())
}
#[derive(Deserialize)]
struct BrowserFunction {
    ranges: Vec<Box<serde_json::value::RawValue>>,
}
fn validate_range(
    raw: &serde_json::value::RawValue,
    length: Option<usize>,
    message: &str,
) -> Result<()> {
    let interval: ProfileRange = serde_json::from_str(raw.get()).map_err(|_| invalid(message))?;
    let start = profile_integer(&interval.start, message)?;
    let end = profile_integer(&interval.end, message)?;
    profile_integer(&interval.count, message)?;
    if (start.len(), &start) > (end.len(), &end)
        || length.is_some_and(|length| {
            let limit = length.to_string();
            (end.len(), &end) > (limit.len(), &limit)
        })
    {
        return Err(invalid(message));
    }
    Ok(())
}
fn number_equals(value: &Value, expected: u32) -> bool {
    value.as_f64() == Some(f64::from(expected))
        || value.as_bool().is_some_and(|b| u32::from(b) == expected)
}
fn native_profile(path: &Path) -> Result<()> {
    // Raw values preserve integer tokens without a floating-point conversion.
    let raw: Box<serde_json::value::RawValue> = serde_json::from_str(&read_text(path)?)?;
    let message = |kind: &str| format!("Invalid native Node {kind}profile: {}", path.display());
    let data: NativeProfile = serde_json::from_str(raw.get()).map_err(|_| invalid(message("")))?;
    if data.result.is_empty() {
        return Err(invalid(message("")));
    }
    for script in data.result {
        let script: NativeScript =
            serde_json::from_str(script.get()).map_err(|_| invalid(message("script ")))?;
        for function in script.functions {
            let function: NativeFunction =
                serde_json::from_str(function.get()).map_err(|_| invalid(message("function ")))?;
            if function.ranges.is_empty() {
                return Err(invalid(message("function ")));
            }
            for interval in function.ranges {
                validate_range(&interval, None, &message("range "))?;
            }
        }
    }
    Ok(())
}

fn file_bytes<'a>(files: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a [u8]> {
    files
        .get(name)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(format!("Missing browser artifact: {name}")))
}
fn decoded(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|error| invalid(error.to_string()))
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut result, byte| {
            write!(&mut result, "{byte:02x}").expect("Writing to a String cannot fail.");
            result
        })
}

fn normpath(path: &Path) -> PathBuf {
    // Source maps use lexical normalization. Never follow filesystem links here.
    let mut parts = PathBuf::new();
    for part in path.components() {
        if part == Component::ParentDir {
            if parts.file_name().is_some_and(|name| name != "..") {
                parts.pop();
            } else if !parts.is_absolute() {
                parts.push("..");
            }
        } else {
            parts.push(part);
        }
    }
    lexical_path(&parts)
}
fn candidates<'a>(mapped: &str, sources: &'a BTreeSet<String>) -> Vec<&'a String> {
    sources
        .iter()
        .filter(|name| mapped == name.as_str() || mapped.ends_with(&format!("/{name}")))
        .collect()
}

fn browser_reports(
    outputs: &[(String, Value)],
    saved: &Path,
    sources: &BTreeSet<String>,
    root: &Path,
    label: &str,
) -> Result<(Vec<PathBuf>, BTreeMap<String, String>)> {
    let files = browser_files(outputs, saved, label)?;
    let receipts = browser_receipts(&files, label)?;
    let mut reports = Vec::new();
    let mut observed = BTreeMap::new();
    for (index, name) in receipts.iter().enumerate() {
        let prefix = name.strip_suffix("receipt.json").unwrap_or("");
        let data: Value = serde_json::from_slice(file_bytes(&files, name)?)?;
        for (filename, key) in [
            ("profile.json", "profile_sha256"),
            ("report.lcov", "report_sha256"),
        ] {
            if data[key].as_str()
                != Some(&digest(file_bytes(&files, &format!("{prefix}{filename}"))?))
            {
                return Err(invalid(format!("Corrupt required browser output: {label}")));
            }
        }
        if !number_equals(&data["schema_version"], 1)
            || !data["scripts"].is_array()
            || !data["sources"].is_array()
        {
            return Err(invalid(format!(
                "Invalid browser coverage receipt: {label}"
            )));
        }
        let profiles = browser_profiles(&files, prefix, label)?;
        let scripts = optional_array(&data, "scripts")?;
        if profiles.len() != scripts.len() {
            return Err(invalid(format!(
                "Incomplete browser coverage inputs: {label}"
            )));
        }
        let verified = browser_sources(&files, prefix, &profiles, scripts, sources, root, label)?;
        let report_path = saved.join(format!("{index}.lcov"));
        write(
            &report_path,
            file_bytes(&files, &format!("{prefix}report.lcov"))?,
        )?;
        let data_sources: BTreeSet<_> = optional_array(&data, "sources")?
            .iter()
            .map(|s| string(s).map(str::to_owned))
            .collect::<Result<_>>()?;
        let measured = if data_sources.is_empty() {
            Vec::new()
        } else {
            records(&report_path, true)?
        };
        if measured
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>()
            != data_sources
            || (data_sources.is_empty() && !read_text(&report_path)?.trim().is_empty())
        {
            return Err(invalid(format!("Incomplete browser report: {label}")));
        }
        let mut canonical = Vec::new();
        for (original, lines) in measured {
            let matches = candidates(&original, &verified);
            if matches.len() > 1 {
                return Err(invalid(format!(
                    "Ambiguous browser report source: {original}"
                )));
            }
            let Some(name) = matches.first() else {
                continue;
            };
            if contains(&canonical, name) {
                return Err(invalid(format!(
                    "Duplicate canonical browser source: {name}"
                )));
            }
            canonical.push((
                (*name).clone(),
                lines
                    .into_iter()
                    .map(|line| {
                        if line.starts_with("SF:") {
                            format!("SF:{name}")
                        } else {
                            line
                        }
                    })
                    .collect(),
            ));
            observed.insert((*name).clone(), report_path.to_string_lossy().into_owned());
        }
        let supported = saved.join(format!("{index}-supported.lcov"));
        write(&supported, record_text(&canonical))?;
        reports.push(supported);
    }
    Ok((reports, observed))
}

struct AngularProject {
    prefix: String,
    target: String,
    runner_sources: BTreeSet<String>,
}
fn angular_projects(root: &Path) -> Result<Vec<AngularProject>> {
    let file = root.join("angular/angular.json");
    let mut projects = Vec::new();
    if !file.exists() {
        return Ok(projects);
    }
    let data = read_json(file)?;
    if let Some(entries) = data["projects"].as_object() {
        for project in entries.values() {
            let test = &project["architect"]["test"];
            if test["builder"] != "@angular/build:unit-test" {
                continue;
            }
            let project_root = string(&project["root"])?;
            let source = project["sourceRoot"]
                .as_str()
                .map_or_else(|| format!("{project_root}/src"), str::to_owned);
            let mut runners = BTreeSet::from(["angular/bzl/vitest-coverage.mjs".to_owned()]);
            if let Some(config) = test["options"]["runnerConfig"].as_str() {
                let config = lexical_path(&Path::new("angular").join(config));
                runners.insert(config.to_string_lossy().into_owned());
                runners.insert(
                    config
                        .with_file_name("vitest-global-setup.ts")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            projects.push(AngularProject {
                prefix: format!("angular/{}/", source.trim_end_matches('/')),
                target: format!("//angular/{project_root}:"),
                runner_sources: runners,
            });
        }
    }
    Ok(projects)
}
fn configured_inputs(
    events: &BuildEvents,
    root: &Path,
    run: &Path,
) -> Result<BTreeMap<(String, String), Value>> {
    let mut configured = BTreeMap::new();
    for input in events.javascript_inputs(root)? {
        for path in input.paths {
            if !path.file_name().is_some_and(|s| {
                s.to_string_lossy()
                    .ends_with(".javascript-test-inputs.json")
            }) {
                continue;
            }
            let data = read_json(&path)?;
            if !data["sources"].is_array() || !data["instrumented"].is_boolean() {
                return Err(invalid(format!(
                    "Invalid configured JavaScript inputs: {}",
                    path.display()
                )));
            }
            let configuration = input
                .identity
                .configuration
                .as_ref()
                .map_or(Value::Null, |v| v["id"].clone());
            configured.insert(
                (string(&data["rule"])?.to_owned(), configuration.to_string()),
                data.clone(),
            );
            let number = configured.len();
            copy(&path, &run.join(format!("{number}-configured.json")))?;
            let saved = run.join(format!("{number}-inputs"));
            directory(&saved)?;
            for artifact in optional_array(&data, "artifacts")? {
                let name = string(&artifact["path"])?;
                let source = root.join(name);
                if source.is_dir() {
                    continue;
                }
                let destination = saved.join(name);
                directory(destination.parent().unwrap_or(&saved))?;
                copy(&source, &destination)?;
                if extension(name) == "map" {
                    let mapping = read_json(&source)?;
                    if !number_equals(&mapping["version"], 3)
                        || !mapping["sources"].is_array()
                        || !mapping["mappings"].is_string()
                    {
                        return Err(invalid(format!(
                            "Invalid required JavaScript source map: {}",
                            source.display()
                        )));
                    }
                }
            }
        }
    }
    Ok(configured)
}
#[derive(Default)]
struct Analysis {
    observed: BTreeMap<String, String>,
    raw_sources: BTreeSet<String>,
    reports: Vec<PathBuf>,
    limits: Vec<Value>,
    executions: Vec<Value>,
}
struct Context<'a> {
    root: &'a Path,
    run: &'a Path,
    sources: &'a BTreeMap<String, Value>,
    native: &'a BTreeSet<String>,
    angular: &'a BTreeSet<String>,
    projects: &'a [AngularProject],
    combined: &'a Records,
}
fn analyze_test(
    analysis: &mut Analysis,
    context: &Context<'_>,
    execution: &JavascriptExecution,
    configured: &BTreeMap<(String, String), Value>,
) -> Result<()> {
    let label = &execution.label;
    let data = configured
        .get(&(label.clone(), execution.configuration.to_string()))
        .filter(|v| v["instrumented"] == true)
        .ok_or_else(|| {
            invalid(format!(
                "Missing required configured JavaScript instrumentation: {label}"
            ))
        })?;
    let index = analysis.executions.len();
    let browser_saved = context.run.join(format!("browser-{index}"));
    let browser = data.get("browser").is_some_and(truthy);
    if browser {
        collect_browser(analysis, context, execution, &browser_saved)?;
    }
    let output = execution
        .outputs
        .iter()
        .find(|(name, _)| name == "test.lcov")
        .ok_or_else(|| invalid(format!("Missing required JavaScript test report: {label}")))?;
    let report = BuildEvents::javascript_artifact(&output.1)?;
    let measured = records(&report, false)?;
    for (name, lines) in &measured {
        if context.sources.contains_key(name)
            && lines
                .iter()
                .any(|s| s.starts_with("DA:") || s.starts_with("BRDA:"))
            && !contains(context.combined, name)
        {
            return Err(invalid(format!(
                "Missing required combined JavaScript source: {label}: {name}"
            )));
        }
    }
    let angular_test = context
        .projects
        .iter()
        .any(|p| label.starts_with(&p.target));
    let configured_sources: BTreeSet<_> = optional_array(data, "sources")?
        .iter()
        .map(|s| string(s).map(str::to_owned))
        .collect::<Result<_>>()?;
    for name in configured_sources.intersection(context.native) {
        if !contains(&measured, name) {
            if angular_test
                && context
                    .projects
                    .iter()
                    .any(|p| label.starts_with(&p.target) && p.runner_sources.contains(name))
            {
                analysis.limits.push(json!({"source":name,"rule":label,"record_absence":true,"missing_guarantee":ANGULAR_RUNNER,"evidence":report}));
            } else {
                return Err(invalid(format!(
                    "Missing required native JavaScript source: {label}: {name}"
                )));
            }
        }
    }
    let saved = context.run.join(format!("test-{index}"));
    let (profiles, retained) = retain_native_profiles(&report, &saved, label)?;
    for name in configured_sources
        .iter()
        .filter(|name| context.sources.contains_key(*name))
    {
        analysis.limits.push(json!({"source":name,"rule":label,"missing_guarantee":CHILD_PROFILES,"evidence":retained}));
    }
    analysis.executions.push(json!({"rule":label,"report":saved.join("report.lcov"),"original_report":report,"retained_profiles":retained,"profiles":profiles,"browser_outputs":if browser{json!(browser_saved)}else{Value::Null},"cached_locally":execution.cached,"execution":execution.execution}));
    let mut supported = Vec::new();
    for (name, lines) in &measured {
        if !context.sources.contains_key(name) {
            continue;
        }
        analysis.raw_sources.insert(name.clone());
        if !angular_test
            && ORIGINALS.contains(&extension(name))
            && lines
                .iter()
                .any(|s| s.starts_with("FN:") && s.contains("(empty-report)"))
            && !has_hits(lines)
        {
            analysis.limits.push(json!({"source":name,"rule":label,"record_absence":true,"missing_guarantee":EMPTY_ORIGINAL,"evidence":saved.join("report.lcov")}));
        } else {
            supported.push((name.clone(), lines.clone()));
        }
    }
    let supported_path = saved.join("supported.lcov");
    write(&supported_path, record_text(&supported))?;
    analysis.reports.push(supported_path);
    analysis.observed.extend(supported.iter().map(|(name, _)| {
        (
            name.clone(),
            saved.join("report.lcov").to_string_lossy().into_owned(),
        )
    }));
    validate_angular_sources(analysis, context, label, &report, &measured)?;
    Ok(())
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
    }
}
fn command(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(invalid(format!("Collector command failed: {status}")));
    }
    Ok(())
}

/// Validate collector evidence and replace the report only after successful merging.
///
/// # Errors
/// Return an error for invalid required evidence or a collector failure.
pub fn run(
    inventory_path: &Path,
    events_path: &Path,
    report_path: &Path,
    evidence_path: &Path,
    collector: &Path,
    merger: &Path,
) -> Result<()> {
    let root = fs::canonicalize(
        std::env::var_os("BUILD_WORKSPACE_DIRECTORY")
            .map_or_else(std::env::current_dir, |p| Ok(PathBuf::from(p)))?,
    )?;
    std::env::set_current_dir(&root)?;
    let inventory_path = root.join(inventory_path);
    let events_path = root.join(events_path);
    let report_path = root.join(report_path);
    let inventory = read_json(&inventory_path)?;
    let mut sources = BTreeMap::new();
    for item in optional_array(&inventory, "eligible")? {
        if item["language"] == "javascript" || item["language"] == "browser" {
            sources.insert(string(&item["path"])?.to_owned(), item.clone());
        }
    }
    let excluded: BTreeSet<_> = optional_array(&inventory, "excluded")?
        .iter()
        .map(|s| string(&s["path"]).map(str::to_owned))
        .collect::<Result<_>>()?;
    let evidence = absolute(&root, evidence_path);
    directory(&evidence)?;
    let run = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&evidence)?
        .keep();
    copy(&inventory_path, &run.join("inventory.json"))?;
    let event_text = read_text(&events_path)?;
    retain_events(&run, &event_text)?;
    let projects = angular_projects(&root)?;
    let angular: BTreeSet<_> = sources
        .keys()
        .filter(|name| projects.iter().any(|p| name.starts_with(&p.prefix)))
        .cloned()
        .collect();
    let native: BTreeSet<_> = sources
        .iter()
        .filter(|(name, item)| !angular.contains(*name) && item["language"] == "javascript")
        .map(|(name, _)| name.clone())
        .collect();
    let NativeBaseline {
        raw_path: baseline_path,
        records: baseline,
        supported,
        supported_path,
    } = native_baseline(&root, &run, &evidence, &native, collector)?;
    let combined = records(&report_path, false)?;
    let events = BuildEvents::parse(&event_text)?;
    let configured = configured_inputs(&events, &root, &run)?;
    let tests: BTreeSet<_> = optional_array(&inventory, "javascript_tests")?
        .iter()
        .map(|v| string(v).map(str::to_owned))
        .collect::<Result<_>>()?;
    let mut analysis = Analysis::default();
    let context = Context {
        root: &root,
        run: &run,
        sources: &sources,
        native: &native,
        angular: &angular,
        projects: &projects,
        combined: &combined,
    };
    for execution in events.javascript_test_results(&tests)? {
        analyze_test(&mut analysis, &context, &execution, &configured)?;
    }
    source_limits(
        &mut analysis,
        &context,
        &baseline,
        &supported,
        &baseline_path,
        &events_path,
    );
    let expected = merge_reports(
        &context,
        &analysis,
        &supported,
        &supported_path,
        &evidence,
        &excluded,
        merger,
    )?;
    let expected_sources: BTreeMap<_, _> = expected
        .iter()
        .map(|(name, lines)| (name, lines.keys().collect::<Vec<_>>()))
        .collect();
    json_file(
        &evidence.join("receipt.json"),
        &json!({"schema_version":1,"native_sources":supported.iter().map(|(name,_)|name).collect::<BTreeSet<_>>(),"sources":expected_sources,"native_manifest_sources":native,"observed_sources":analysis.observed,"executions":analysis.executions,"limits":analysis.limits,"baseline":supported_path,"raw_native_baseline":baseline_path,"events":run.join("events.json"),"inventory":run.join("inventory.json"),"report":absolute(&root,&report_path)}),
    )?;
    write(&report_path, read_text(evidence.join("merged.lcov"))?)?;
    Ok(())
}

const BROWSER_NAVIGATION: &str = "Chromium coverage resets across page navigation. The retained final page profile does not establish complete measurement of earlier navigations.";

const ANGULAR_RUNNER: &str = "The authoritative Vitest report omits runner configuration and global setup code outside its test instrumentation context.";

const CHILD_PROFILES: &str = "Native Node profiles do not declare the expected process set. A valid parent profile cannot prove that every executed child profile remains available.";

const EMPTY_ORIGINAL: &str = "The native original-manifest path produced an empty-script fallback. It does not establish original executable records or measured zero coverage.";

const ANGULAR_EXCLUDED: &str =
    "The Angular builder excludes test sources and declarations from its coverage input.";

const ANGULAR_NOT_RUN: &str = "The selected targets or execution filters did not run this Angular builder. Its documented coverage inclusion requires a test run.";

const UNMAPPED_ORIGINAL: &str = "The native original-manifest baseline does not establish original TypeScript or JSX executable records. No supported mapped record is available in this run.";

const ASTRO_LIMIT: &str = "The selected browser reports supply no map with the exact original Astro source content. Static build-time template execution and disconnected originals have no measured browser baseline. No processed-script bridge or original mapping is reconstructed.";

const PROVIDER_LIMIT: &str = "The maintained provider supplies its own source attribution and lowered branch semantics. Original executable-line and branch identities are not reconstructed.";

fn validate_record(path: &Path, name: &str, lines: &[String]) -> Result<()> {
    static LINE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^DA:([1-9][0-9]*),(\d+)(?:,[^,]+)?$").expect("Static pattern")
    });
    static BRANCH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^BRDA:([1-9][0-9]*,\d+,\d+),(-|\d+)$").expect("Static pattern")
    });
    static DEFINITION_PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^FN:[0-9]+,(?:[0-9]+,)?.+$").expect("Static pattern"));
    static CALL_PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^FNDA:[0-9]+,.+$").expect("Static pattern"));
    let mut counts = BTreeMap::new();
    let mut branches = BTreeMap::new();
    let fail = |kind: &str| invalid(format!("{kind}: {}: {name}", path.display()));
    for line in lines {
        if line.starts_with("DA:") {
            let m = LINE_PATTERN
                .captures(line)
                .ok_or_else(|| fail("Invalid JavaScript line record"))?;
            let number: u64 = m[1]
                .parse()
                .map_err(|_| fail("Invalid JavaScript line record"))?;
            let count: u64 = m[2]
                .parse()
                .map_err(|_| fail("Invalid JavaScript line record"))?;
            if counts.insert(number, count).is_some() {
                return Err(fail("Invalid JavaScript line record"));
            }
        }
        if line.starts_with("BRDA:") {
            let m = BRANCH_PATTERN
                .captures(line)
                .ok_or_else(|| fail("Invalid JavaScript branch record"))?;
            let count = if &m[2] == "-" {
                0
            } else {
                m[2].parse::<u64>()
                    .map_err(|_| fail("Invalid JavaScript branch record"))?
            };
            if branches.insert(m[1].to_owned(), count).is_some() {
                return Err(fail("Invalid JavaScript branch record"));
            }
        }
        if !line.split_once(':').is_some_and(|(key, _)| {
            [
                "TN", "SF", "VER", "FN", "FNDA", "FNF", "FNH", "DA", "LF", "LH", "BRDA", "BRF",
                "BRH",
            ]
            .contains(&key)
        }) {
            return Err(invalid(format!(
                "Invalid JavaScript LCOV field: {}",
                path.display()
            )));
        }
        if (line.starts_with("FN:") && !DEFINITION_PATTERN.is_match(line))
            || (line.starts_with("FNDA:") && !CALL_PATTERN.is_match(line))
        {
            return Err(invalid(format!(
                "Invalid JavaScript function record: {}",
                path.display()
            )));
        }
    }
    validate_summaries(path, name, lines, &counts, &branches)
}

fn validate_summaries(
    path: &Path,
    name: &str,
    lines: &[String],
    counts: &BTreeMap<u64, u64>,
    branches: &BTreeMap<String, u64>,
) -> Result<()> {
    let fail = |kind: &str| invalid(format!("{kind}: {}: {name}", path.display()));
    let fields = |prefix: &str| -> Vec<&str> {
        lines
            .iter()
            .filter(|s| s.starts_with(prefix))
            .map(String::as_str)
            .collect()
    };
    if fields("LF:") != [format!("LF:{}", counts.len())]
        || fields("LH:")
            != [format!(
                "LH:{}",
                counts.values().filter(|n| **n > 0).count()
            )]
    {
        return Err(fail("Incomplete JavaScript executable records"));
    }
    let definitions = fields("FN:");
    let calls = fields("FNDA:");
    if (!definitions.is_empty()
        || !calls.is_empty()
        || !fields("FNF:").is_empty()
        || !fields("FNH:").is_empty())
        && (fields("FNF:") != [format!("FNF:{}", definitions.len())]
            || calls.len() != definitions.len()
            || fields("FNH:")
                != [format!(
                    "FNH:{}",
                    calls
                        .iter()
                        .filter(|s| has_hits(&[(**s).to_owned()]))
                        .count()
                )])
    {
        return Err(invalid(format!(
            "Incomplete JavaScript function coverage: {}",
            path.display()
        )));
    }
    if (!branches.is_empty() || !fields("BRF:").is_empty() || !fields("BRH:").is_empty())
        && (fields("BRF:") != [format!("BRF:{}", branches.len())]
            || fields("BRH:")
                != [format!(
                    "BRH:{}",
                    branches.values().filter(|n| **n > 0).count()
                )])
    {
        return Err(fail("Incomplete JavaScript branch coverage"));
    }
    Ok(())
}

fn browser_files(
    outputs: &[(String, Value)],
    saved: &Path,
    label: &str,
) -> Result<BTreeMap<String, Vec<u8>>> {
    fs::DirBuilder::new().mode(0o700).create(saved)?;
    let mut files = BTreeMap::new();
    if let Some((_, file)) = outputs
        .iter()
        .find(|(name, _)| name == "test.outputs__outputs.zip")
    {
        let path = BuildEvents::javascript_artifact(file)?;
        copy(&path, &saved.join("outputs.zip"))?;
        let mut archive =
            ZipArchive::new(fs::File::open(path)?).map_err(|e| invalid(e.to_string()))?;
        // ZipFile.read(name) selects the last member for duplicate names.
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| invalid(e.to_string()))?;
            let name = entry
                .name()
                .map_err(|error| invalid(error.to_string()))?
                .into_owned();
            if name.starts_with("browser-coverage/") && !name.ends_with('/') {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                files.insert(name, bytes);
            }
        }
    } else {
        for (name, file) in outputs {
            if let Some(index) = name.find("browser-coverage/") {
                files.insert(
                    name[index..].to_owned(),
                    fs::read(BuildEvents::javascript_artifact(file)?)?,
                );
            }
        }
        for (name, bytes) in &files {
            if Path::new(name)
                .components()
                .any(|part| part == Component::ParentDir)
            {
                return Err(invalid(format!("Invalid browser artifact path: {name}")));
            }
            let destination = saved.join(name);
            directory(destination.parent().unwrap_or(saved))?;
            write(&destination, bytes)?;
        }
    }
    if files.is_empty() {
        return Err(invalid(format!(
            "Missing required browser coverage: {label}"
        )));
    }
    Ok(files)
}

fn browser_receipts(files: &BTreeMap<String, Vec<u8>>, label: &str) -> Result<Vec<String>> {
    let receipts: Vec<_> = files
        .keys()
        .filter(|name| name.ends_with("/receipt.json"))
        .cloned()
        .collect();
    if receipts.is_empty() {
        return Err(invalid(format!(
            "Missing required browser coverage receipts: {label}"
        )));
    }
    let pages: Value = serde_json::from_slice(file_bytes(files, "browser-coverage/pages.json")?)?;
    let page_pattern = Regex::new(r"^[A-Za-z0-9-]+$").expect("Static pattern");
    let pages = pages
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| invalid(format!("Incomplete required browser page outputs: {label}")))?;
    let mut page_names = BTreeSet::new();
    for page in pages {
        let page = page
            .as_str()
            .filter(|s| page_pattern.is_match(s))
            .ok_or_else(|| invalid(format!("Incomplete required browser page outputs: {label}")))?;
        if !page_names.insert(format!("browser-coverage/{page}/receipt.json")) {
            return Err(invalid(format!(
                "Incomplete required browser page outputs: {label}"
            )));
        }
    }
    if page_names.into_iter().collect::<Vec<_>>() != receipts {
        return Err(invalid(format!(
            "Incomplete required browser page outputs: {label}"
        )));
    }
    Ok(receipts)
}

type RawBrowserProfile = BTreeMap<String, Box<serde_json::value::RawValue>>;
fn browser_profiles(
    files: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
    label: &str,
) -> Result<Vec<RawBrowserProfile>> {
    let raw: Box<serde_json::value::RawValue> =
        serde_json::from_slice(file_bytes(files, &format!("{prefix}profile.json"))?)?;
    let raw: Vec<BTreeMap<String, Box<serde_json::value::RawValue>>> =
        serde_json::from_str(raw.get())
            .map_err(|_| invalid(format!("Invalid browser coverage receipt: {label}")))?;
    let mut profiles = Vec::new();
    for profile in raw {
        let url = profile
            .get("url")
            .ok_or_else(|| invalid(format!("Incomplete browser coverage inputs: {label}")))?;
        let url: Option<String> = serde_json::from_str(url.get())
            .map_err(|_| invalid(format!("Incomplete browser coverage inputs: {label}")))?;
        if url.is_some_and(|url| {
            let url = url.split(['?', '#']).next().unwrap_or("");
            let Some((_, location)) = url.split_once("://") else {
                return false;
            };
            let (host, path) = location.split_once('/').unwrap_or((location, ""));
            let last_segment = path.rfind('/').map_or(0, |index| index + 1);
            let path = path[last_segment..]
                .find(';')
                .map_or(path, |index| &path[..last_segment + index]);
            host.split('@').next_back().unwrap_or("").split(':').next() == Some("127.0.0.1")
                && path
                    .rsplit_once('.')
                    .is_some_and(|(_, suffix)| suffix == "js")
        }) {
            profiles.push(profile);
        }
    }
    Ok(profiles)
}

fn browser_sources(
    files: &BTreeMap<String, Vec<u8>>,
    prefix: &str,
    profiles: &[RawBrowserProfile],
    scripts: &[Value],
    sources: &BTreeSet<String>,
    root: &Path,
    label: &str,
) -> Result<BTreeSet<String>> {
    let mut verified = BTreeSet::new();
    for (script_index, (profile, script)) in profiles.iter().zip(scripts).enumerate() {
        let source = decoded(file_bytes(files, &format!("{prefix}{script_index}.js"))?)?;
        let map_bytes = file_bytes(files, &format!("{prefix}{script_index}.map"))?;
        if script["map_sha256"].as_str() != Some(&digest(map_bytes)) {
            return Err(invalid(format!(
                "Corrupt required browser source map: {label}"
            )));
        }
        let mapping: Value = serde_json::from_slice(map_bytes)?;
        let original_source: Option<String> = profile
            .get("source")
            .map(|raw| serde_json::from_str(raw.get()))
            .transpose()?;
        if original_source.as_deref() != Some(source)
            || !number_equals(&mapping["version"], 3)
            || !mapping["mappings"].is_string()
            || mapping["sources"] != script["sources"]
            || mapping["sourcesContent"] != script["sourcesContent"]
        {
            return Err(invalid(format!(
                "Invalid required browser source map: {label}"
            )));
        }
        let function_error = || invalid(format!("Invalid browser function ranges: {label}"));
        let functions = profile.get("functions").ok_or_else(function_error)?;
        let functions: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_str(functions.get()).map_err(|_| function_error())?;
        for function in functions {
            let function: BrowserFunction =
                serde_json::from_str(function.get()).map_err(|_| function_error())?;
            if function.ranges.is_empty() {
                return Err(function_error());
            }
            for interval in function.ranges {
                validate_range(
                    &interval,
                    Some(source.encode_utf16().count()),
                    &format!("Invalid browser coverage range: {label}"),
                )?;
            }
        }
        let originals = script["sources"]
            .as_array()
            .ok_or_else(|| invalid(format!("Invalid required browser source map: {label}")))?;
        let contents = script["sourcesContent"]
            .as_array()
            .ok_or_else(|| invalid(format!("Invalid required browser source map: {label}")))?;
        if originals.len() != contents.len() {
            return Err(invalid("Unequal browser source lists."));
        }
        for (original, content) in originals.iter().zip(contents) {
            let script_path = Path::new(string(&script["script"])?);
            let path = normpath(
                &script_path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(
                        mapping
                            .get("sourceRoot")
                            .map(string)
                            .transpose()?
                            .unwrap_or(""),
                    )
                    .join(string(original)?),
            );
            let mapped = path.to_string_lossy();
            let matches = candidates(&mapped, sources);
            if matches.len() > 1 {
                return Err(invalid(format!("Ambiguous browser source: {mapped}")));
            }
            if let Some(name) = matches.first()
                && read_text(root.join(name))? == *content
            {
                verified.insert((*name).clone());
            }
        }
    }
    Ok(verified)
}

fn retain_native_profiles(
    report: &Path,
    saved: &Path,
    label: &str,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let profile_dir = report
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("_coverage");
    let mut profiles = Vec::new();
    if profile_dir.is_dir() {
        for entry in fs::read_dir(profile_dir)? {
            let path = entry?.path();
            if path.file_name().is_some_and(|s| {
                s.to_string_lossy().starts_with("coverage-")
                    && s.to_string_lossy().ends_with(".json")
            }) {
                profiles.push(path);
            }
        }
    }
    profiles.sort();
    fs::DirBuilder::new().mode(0o700).create(saved)?;
    copy(report, &saved.join("report.lcov"))?;
    let mut retained = Vec::new();
    for profile in &profiles {
        let destination = saved.join(
            profile
                .file_name()
                .ok_or_else(|| invalid("Missing profile filename."))?,
        );
        copy(profile, &destination)?;
        retained.push(destination);
    }
    if profiles.is_empty() {
        return Err(invalid(format!(
            "Missing required native Node profile: {label}"
        )));
    }
    for profile in &profiles {
        native_profile(profile)?;
    }
    Ok((profiles, retained))
}

fn validate_angular_sources(
    analysis: &mut Analysis,
    context: &Context<'_>,
    label: &str,
    report: &Path,
    measured: &Records,
) -> Result<()> {
    let test_source = Regex::new(r"\.(spec|test)\.[cm]?[jt]sx?$").expect("Static pattern");
    for project in context.projects {
        if !label.starts_with(&project.target) {
            continue;
        }
        for name in context
            .angular
            .iter()
            .filter(|name| name.starts_with(&project.prefix))
        {
            if test_source.is_match(name) || name.ends_with(".d.ts") {
                analysis.limits.push(json!({"source":name,"rule":label,"record_absence":true,"missing_guarantee":ANGULAR_EXCLUDED,"evidence":report}));
            } else if !contains(measured, name) {
                return Err(invalid(format!(
                    "Missing required Angular source record: {label}: {name}"
                )));
            }
        }
    }
    Ok(())
}

struct NativeBaseline {
    raw_path: PathBuf,
    records: Records,
    supported: Records,
    supported_path: PathBuf,
}
fn native_baseline(
    root: &Path,
    run: &Path,
    evidence: &Path,
    native: &BTreeSet<String>,
    collector: &Path,
) -> Result<NativeBaseline> {
    let manifest = evidence.join("native-manifest.txt");
    write(
        &manifest,
        native.iter().map(|s| s.clone() + "\n").collect::<String>(),
    )?;
    let native_dir = tempfile::Builder::new()
        .prefix("native-")
        .tempdir_in(run)?
        .keep();
    command(
        Command::new(collector)
            .current_dir(root)
            .env("COVERAGE_MANIFEST", &manifest)
            .env("JAVASCRIPT_BASELINE_DIRECTORY", &native_dir)
            .env("NODE_V8_COVERAGE", "")
            .env("JS_COVERAGE__RUNFILES", root)
            .env("TEST_WORKSPACE", ".")
            .env("BAZEL_BINDIR", ".")
            .env_remove("COVERAGE_OUTPUT_FILE")
            .env_remove("COVERAGE_DIR"),
    )?;
    let baseline_path = native_dir.join("_rules_js_report.lcov");
    let baseline = if native.is_empty() {
        Vec::new()
    } else {
        records(&baseline_path, false)?
    };
    let baseline_sources: BTreeSet<_> = baseline.iter().map(|(name, _)| name.clone()).collect();
    if &baseline_sources != native {
        let quoted = |set: Vec<&String>| {
            format!(
                "[{}]",
                set.iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return Err(invalid(format!(
            "Incomplete native JavaScript baseline: missing={}, unexpected={}",
            quoted(native.difference(&baseline_sources).collect()),
            quoted(baseline_sources.difference(native).collect())
        )));
    }
    let supported: Records = baseline
        .iter()
        .filter(|(name, _)| ["js", "mjs", "cjs"].contains(&extension(name)))
        .cloned()
        .collect();
    let supported_path = run.join("native-supported.lcov");
    write(&supported_path, record_text(&supported))?;
    Ok(NativeBaseline {
        raw_path: baseline_path,
        records: baseline,
        supported,
        supported_path,
    })
}

fn source_limits(
    analysis: &mut Analysis,
    context: &Context<'_>,
    baseline: &Records,
    supported: &Records,
    baseline_path: &Path,
    events_path: &Path,
) {
    let root = context.root;
    let run = context.run;
    let sources = context.sources;
    let native = context.native;
    let angular = context.angular;
    for name in angular
        .iter()
        .filter(|name| !analysis.observed.contains_key(*name))
    {
        if !analysis.limits.iter().any(|item| item["source"] == *name) {
            analysis.limits.push(json!({"source":name,"owners":sources[name]["owners"],"record_absence":true,"missing_guarantee":ANGULAR_NOT_RUN,"evidence":absolute(root,events_path)}));
        }
    }
    for name in native
        .iter()
        .filter(|name| !contains(supported, name) && !analysis.observed.contains_key(*name))
    {
        analysis.limits.push(json!({"source":name,"owners":sources[name]["owners"],"record_absence":true,"missing_guarantee":UNMAPPED_ORIGINAL,"evidence":baseline_path}));
    }
    for (name, item) in sources {
        if item["language"] == "browser" && !analysis.observed.contains_key(name) {
            analysis.limits.push(json!({"source":name,"owners":item["owners"],"record_absence":true,"missing_guarantee":ASTRO_LIMIT,"evidence":[run,"https://docs.astro.build/en/reference/configuration-reference/#vite","https://playwright.dev/docs/api/class-coverage"]}));
        }
    }
    for (name, item) in sources {
        analysis.limits.push(json!({"source":name,"owners":item["owners"],"missing_guarantee":PROVIDER_LIMIT,"evidence":analysis.observed.get(name).cloned().unwrap_or_else(||if contains(baseline,name){baseline_path.to_string_lossy().into_owned()}else{absolute(root,events_path).to_string_lossy().into_owned()})}));
    }
}

type LineCounts = BTreeMap<String, BTreeMap<u64, u64>>;
fn required_lines(inputs: &[PathBuf], sources: &BTreeMap<String, Value>) -> Result<LineCounts> {
    let mut expected = BTreeMap::<String, BTreeMap<u64, u64>>::new();
    for path in inputs {
        if read_text(path)?.trim().is_empty() {
            continue;
        }
        for (name, lines) in records(path, false)? {
            if !sources.contains_key(&name) {
                continue;
            }
            let counts = expected.entry(name).or_default();
            for line in lines {
                if let Some(data) = line.strip_prefix("DA:") {
                    let mut parts = data.split(',');
                    let number = parts
                        .next()
                        .unwrap_or("")
                        .parse::<u64>()
                        .map_err(|e| invalid(e.to_string()))?;
                    let count = parts
                        .next()
                        .unwrap_or("")
                        .parse::<u64>()
                        .map_err(|e| invalid(e.to_string()))?;
                    counts
                        .entry(number)
                        .and_modify(|old| *old = (*old).max(count))
                        .or_insert(count);
                }
            }
        }
    }
    Ok(expected)
}

fn validate_merged(
    final_records: &Records,
    expected: &LineCounts,
    supported: &Records,
    observed: &BTreeMap<String, String>,
) -> Result<()> {
    for name in supported
        .iter()
        .map(|(name, _)| name)
        .chain(observed.keys())
    {
        if !contains(final_records, name) {
            return Err(invalid(format!("Missing merged JavaScript source: {name}")));
        }
    }
    for (name, required) in expected {
        let mut actual = BTreeMap::new();
        if let Some((_, lines)) = final_records.iter().find(|(key, _)| key == name) {
            for line in lines {
                if let Some(data) = line.strip_prefix("DA:") {
                    let parts: Vec<_> = data.split(',').collect();
                    actual.insert(
                        parts[0]
                            .parse::<u64>()
                            .map_err(|e| invalid(e.to_string()))?,
                        parts[1]
                            .parse::<u64>()
                            .map_err(|e| invalid(e.to_string()))?,
                    );
                }
            }
        }
        if !contains(final_records, name)
            || required
                .iter()
                .any(|(number, count)| actual.get(number).is_none_or(|n| n < count))
        {
            return Err(invalid(format!(
                "Missing merged JavaScript lines or hits: {name}"
            )));
        }
    }
    Ok(())
}

fn merge_reports(
    context: &Context<'_>,
    analysis: &Analysis,
    supported: &Records,
    supported_path: &Path,
    evidence: &Path,
    excluded: &BTreeSet<String>,
    merger: &Path,
) -> Result<LineCounts> {
    let root = context.root;
    let sources = context.sources;
    let native = context.native;
    let combined = context.combined;
    let retained: Records = combined
        .iter()
        .filter(|(name, lines)| {
            !(analysis.raw_sources.contains(name)
                || (native.contains(name)
                    && ORIGINALS.contains(&extension(name))
                    && !has_hits(lines))
                || (EXTENSIONS.contains(&extension(name))
                    && (excluded.contains(name) || !sources.contains_key(name)))
                || (name.starts_with("angular/")
                    && name
                        .rsplit_once('.')
                        .is_some_and(|(_, suffix)| suffix == "html")))
        })
        .cloned()
        .collect();
    let filtered = evidence.join("filtered.lcov");
    write(&filtered, record_text(&retained))?;
    let mut inputs = vec![filtered, supported_path.to_path_buf()];
    inputs.extend(analysis.reports.iter().cloned());
    let expected = required_lines(&inputs, sources)?;
    let reports_file = evidence.join("reports.txt");
    write(
        &reports_file,
        inputs
            .iter()
            .map(|p| p.to_string_lossy().into_owned() + "\n")
            .collect::<String>(),
    )?;
    let output_path = evidence.join("merged.lcov");
    command(
        Command::new(merger)
            .current_dir(root)
            .arg(format!("--reports_file={}", reports_file.display()))
            .arg(format!("--output_file={}", output_path.display())),
    )?;
    let final_records = records(&output_path, false)?;
    validate_merged(&final_records, &expected, supported, &analysis.observed)?;
    Ok(expected)
}

fn retain_events(run: &Path, event_text: &str) -> Result<()> {
    let redaction =
        Regex::new(r#"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+"#).expect("Static pattern");
    write(
        &run.join("events.json"),
        redaction
            .replace_all(event_text, "${1}<REDACTED>")
            .as_bytes(),
    )?;
    Ok(())
}

fn collect_browser(
    analysis: &mut Analysis,
    context: &Context<'_>,
    execution: &JavascriptExecution,
    browser_saved: &Path,
) -> Result<()> {
    let label = &execution.label;
    let (reports, observed) = browser_reports(
        &execution.outputs,
        browser_saved,
        &context.sources.keys().cloned().collect(),
        context.root,
        label,
    )?;
    analysis.reports.extend(reports);
    for source in observed.keys() {
        analysis.limits.push(json!({"source":source,"rule":label,"missing_guarantee":BROWSER_NAVIGATION,"evidence":[browser_saved,"https://playwright.dev/docs/api/class-coverage#coverage-start-js-coverage"]}));
    }
    analysis.observed.extend(observed);
    Ok(())
}
