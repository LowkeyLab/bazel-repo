//! Offline Cargo generation from declared Bazel analysis and provenance inputs.
#![forbid(unsafe_code)]

use cargo_publish_core::{
    AnalysisSpec, BinaryTarget, ContractError, CrateMetadata, Dependency, DependencyKind,
    GenerationContract, InputFile, LibraryTarget, PackageMetadata, PackagePath, Provenance,
    SCHEMA_VERSION, Selection, TARGET, TestTarget, digest_bytes, digest_file, read_json,
    write_json,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Debug, thiserror::Error)]
pub enum GenerationError {
    #[error("{0}")]
    Policy(String),
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: invalid Cargo TOML: {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
type Result<T> = std::result::Result<T, GenerationError>;
fn reject<T>(message: impl Into<String>) -> Result<T> {
    Err(GenerationError::Policy(message.into()))
}
fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| GenerationError::Io {
        path: path.to_owned(),
        source,
    })
}
fn io<T>(path: &Path, result: std::io::Result<T>) -> Result<T> {
    result.map_err(|source| GenerationError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Explicit command inputs; generation does not scan the checkout or use a registry.
pub struct GenerateOptions {
    pub spec: PathBuf,
    pub root_manifest: PathBuf,
    pub root_lockfile: PathBuf,
    pub module: PathBuf,
    pub output: PathBuf,
    pub contract: PathBuf,
}

#[derive(Default, Deserialize)]
struct CargoPackage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
}
#[derive(Default, Deserialize)]
struct CargoLib {
    name: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum CargoDependency {
    Version(String),
    Detailed(CargoDependencyDetails),
}
#[derive(Default, Deserialize)]
struct CargoDependencyDetails {
    package: Option<String>,
    version: Option<String>,
    optional: Option<bool>,
    #[serde(flatten)]
    provenance: BTreeMap<String, toml::Value>,
}
#[derive(Default, Deserialize)]
struct CargoTarget {
    #[serde(default)]
    dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, CargoDependency>,
}
#[derive(Default, Deserialize)]
struct CargoManifest {
    #[serde(default)]
    package: CargoPackage,
    #[serde(default, rename = "lib")]
    library: CargoLib,
    #[serde(default)]
    dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    target: BTreeMap<String, CargoTarget>,
}
#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}
#[derive(Default, Deserialize)]
struct CargoLock {
    #[serde(default)]
    package: Vec<LockPackage>,
}
fn toml_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    toml::from_str(&read(path)?).map_err(|source| GenerationError::Toml {
        path: path.to_owned(),
        source,
    })
}

fn validate_root_declaration(root: &CargoManifest, name: &str, label: &str) -> Result<()> {
    let mut declared = false;
    for table in [
        &root.dependencies,
        &root.dev_dependencies,
        &root.build_dependencies,
    ] {
        for (key, item) in table {
            let package = match item {
                CargoDependency::Version(_) => key.as_str(),
                CargoDependency::Detailed(d) => d.package.as_deref().unwrap_or(key),
            };
            if package != name {
                continue;
            }
            declared = true;
            let valid = match item {
                CargoDependency::Version(v) => !v.is_empty(),
                CargoDependency::Detailed(d) => {
                    d.version.is_some()
                        && !["git", "path", "registry", "workspace"]
                            .iter()
                            .any(|key| d.provenance.contains_key(*key))
                }
            };
            if !valid {
                return reject(format!(
                    "{label}: unsupported/ambiguous root dependency provenance for {name}"
                ));
            }
        }
    }
    if !declared {
        return reject(format!(
            "{label}: {name} has no direct package identity in root Cargo manifest"
        ));
    }
    Ok(())
}

fn registry_checksum(lock: &CargoLock, name: &str, version: &str, label: &str) -> Result<String> {
    let locked: Vec<_> = lock
        .package
        .iter()
        .filter(|item| item.name == name && item.version == version)
        .collect();
    if locked.len() != 1
        || locked[0].source.as_deref() != Some(REGISTRY)
        || locked[0].checksum.as_ref().is_none_or(String::is_empty)
    {
        return reject(format!(
            "{label}: ambiguous or unsupported crates.io lock provenance for {name}@{version}"
        ));
    }
    Ok(locked[0].checksum.clone().unwrap_or_default())
}

fn configured_features(
    upstream: &CargoManifest,
    metadata: &CrateMetadata,
    name: &str,
) -> Result<BTreeSet<String>> {
    let label = &metadata.label;
    let mut known: BTreeSet<String> = upstream.features.keys().cloned().collect();
    known.insert("default".into());
    let mut tables = vec![&upstream.dependencies, &upstream.build_dependencies];
    for target in upstream.target.values() {
        tables.extend([&target.dependencies, &target.build_dependencies]);
    }
    for table in tables {
        for (key, value) in table {
            if matches!(value,CargoDependency::Detailed(d) if d.optional==Some(true)) {
                known.insert(key.clone());
            }
        }
    }
    let features: BTreeSet<_> = metadata.features.iter().cloned().collect();
    let unknown: Vec<_> = features.difference(&known).collect();
    if !unknown.is_empty() {
        return reject(format!(
            "{label}: ambiguous configured features {unknown:?} for {name}"
        ));
    }
    Ok(features)
}

fn dependency(
    metadata: &CrateMetadata,
    root: &CargoManifest,
    lock: &CargoLock,
    kind: DependencyKind,
    alias: &str,
) -> Result<Dependency> {
    let label = &metadata.label;
    if !metadata.external {
        return reject(format!(
            "{label}: separate local dependency packages are unsupported"
        ));
    }
    if !metadata.compiler_settings.is_empty() {
        return reject(format!(
            "{label}: unsupported compiler settings {:?}",
            metadata.compiler_settings
        ));
    }
    let (Some(manifest), Some(version)) = (&metadata.manifest, &metadata.version) else {
        return reject(format!(
            "{label}: ambiguous/missing Cargo manifest or version metadata"
        ));
    };
    if version.is_empty() || manifest.as_os_str().is_empty() {
        return reject(format!(
            "{label}: ambiguous/missing Cargo manifest or version metadata"
        ));
    }
    let upstream: CargoManifest = toml_file(manifest)?;
    let name = &upstream.package.name;
    if name.is_empty() || upstream.package.version != *version {
        return reject(format!(
            "{label}: ambiguous Cargo package identity/version {name}@{version}"
        ));
    }
    if metadata.repository != format!("rules_rs++crate+crates__{name}-{version}") {
        return reject(format!(
            "{label}: unsupported or ambiguous configured repository provenance for {name}@{version}"
        ));
    }
    if upstream
        .library
        .name
        .as_deref()
        .map_or_else(|| name.replace('-', "_"), str::to_owned)
        != metadata.crate_name
    {
        return reject(format!(
            "{label}: ambiguous library name {} for package {name}",
            metadata.crate_name
        ));
    }
    validate_root_declaration(root, name, label)?;
    let checksum = registry_checksum(lock, name, version, label)?;
    let features = configured_features(&upstream, metadata, name)?;
    Ok(Dependency {
        target: label.clone(),
        package: name.clone(),
        version: version.clone(),
        source: REGISTRY.into(),
        checksum,
        default_features: features.contains("default"),
        features: features.into_iter().collect(),
        alias: alias.into(),
        kind,
    })
}

// MODULE.bazel remains data: tokenize real syntax and inspect only literal
// attribute calls. Strings and comments cannot create spurious annotations.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    String(String),
    Punct(char),
}
fn tokens(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut result = Vec::new();
    while i < chars.len() {
        let raw = matches!(chars[i], 'r' | 'R')
            && chars.get(i + 1).is_some_and(|c| matches!(c, '\'' | '"'));
        if raw {
            i += 1;
        }
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            let quote = c;
            let triple = chars.get(i + 1) == Some(&quote) && chars.get(i + 2) == Some(&quote);
            i += if triple { 3 } else { 1 };
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == quote
                    && (!triple
                        || (chars.get(i + 1) == Some(&quote) && chars.get(i + 2) == Some(&quote)))
                {
                    i += if triple { 3 } else { 1 };
                    closed = true;
                    break;
                }
                if chars[i] == '\\' {
                    i += 1;
                    let Some(&escaped) = chars.get(i) else {
                        break;
                    };
                    if raw {
                        value.push('\\');
                    }
                    value.push(if raw {
                        escaped
                    } else {
                        match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        }
                    });
                } else {
                    value.push(chars[i]);
                }
                i += 1;
            }
            if !closed {
                return reject("MODULE.bazel: unterminated quoted string");
            }
            result.push(Token::String(value));
            continue;
        }
        if c.is_ascii_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            result.push(Token::Word(chars[start..i].iter().collect()));
        } else {
            result.push(Token::Punct(c));
            i += 1;
        }
    }
    Ok(result)
}
#[derive(Clone, Debug)]
enum Literal {
    String(String),
    Bool(bool),
    Number(i64),
    None,
    List(Vec<Self>),
    Tuple(Vec<Self>),
    Dict(Vec<(Self, Self)>),
}
impl Literal {
    const fn string(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v.as_str())
        } else {
            None
        }
    }
    const fn truthy(&self) -> bool {
        match self {
            Self::String(v) => !v.is_empty(),
            Self::Bool(v) => *v,
            Self::Number(v) => *v != 0,
            Self::None => false,
            Self::List(v) | Self::Tuple(v) => !v.is_empty(),
            Self::Dict(v) => !v.is_empty(),
        }
    }
    fn hashable(&self) -> bool {
        match self {
            Self::String(_) | Self::Bool(_) | Self::Number(_) | Self::None => true,
            Self::Tuple(values) => values.iter().all(Self::hashable),
            Self::List(_) | Self::Dict(_) => false,
        }
    }
    fn contains(&self, needle: &str) -> bool {
        match self {
            Self::List(v) | Self::Tuple(v) => v.iter().any(|v| v.string() == Some(needle)),
            Self::String(v) => v.contains(needle),
            Self::Dict(v) => v.iter().any(|(key, _)| key.string() == Some(needle)),
            _ => false,
        }
    }
}
fn literal(tokens: &[Token], i: &mut usize) -> Result<Literal> {
    let Some(token) = tokens.get(*i) else {
        return reject("MODULE.bazel: ambiguous crate annotation/override configuration");
    };
    *i += 1;
    match token {
        Token::String(v) => Ok(Literal::String(v.clone())),
        Token::Word(v) if v == "True" => Ok(Literal::Bool(true)),
        Token::Word(v) if v == "False" => Ok(Literal::Bool(false)),
        Token::Word(v) if v == "None" => Ok(Literal::None),
        Token::Word(v) if v.parse::<i64>().is_ok() => {
            Ok(Literal::Number(v.parse().unwrap_or_default()))
        }
        Token::Punct('+' | '-') => {
            let Some(Token::Word(digits)) = tokens.get(*i) else {
                return reject("MODULE.bazel: ambiguous crate annotation/override configuration");
            };
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                return reject("MODULE.bazel: ambiguous crate annotation/override configuration");
            }
            let sign = if *token == Token::Punct('-') {
                "-"
            } else {
                "+"
            };
            let value = format!("{sign}{digits}").parse::<i64>().map_err(|_| {
                GenerationError::Policy(
                    "MODULE.bazel: ambiguous crate annotation/override configuration".into(),
                )
            })?;
            *i += 1;
            Ok(Literal::Number(value))
        }
        Token::Punct('[' | '(') => {
            let close = if *token == Token::Punct('[') {
                ']'
            } else {
                ')'
            };
            let mut values = Vec::new();
            while tokens.get(*i) != Some(&Token::Punct(close)) {
                values.push(literal(tokens, i)?);
                if tokens.get(*i) == Some(&Token::Punct(',')) {
                    *i += 1;
                } else if tokens.get(*i) != Some(&Token::Punct(close)) {
                    return reject(
                        "MODULE.bazel: ambiguous crate annotation/override configuration",
                    );
                }
            }
            *i += 1;
            Ok(if close == ']' {
                Literal::List(values)
            } else {
                Literal::Tuple(values)
            })
        }
        Token::Punct('{') => {
            let mut values = Vec::new();
            while tokens.get(*i) != Some(&Token::Punct('}')) {
                let key = literal(tokens, i)?;
                if !key.hashable() {
                    return reject(
                        "MODULE.bazel: ambiguous crate annotation/override configuration",
                    );
                }
                if tokens.get(*i) != Some(&Token::Punct(':')) {
                    return reject(
                        "MODULE.bazel: ambiguous crate annotation/override configuration",
                    );
                }
                *i += 1;
                values.push((key, literal(tokens, i)?));
                if tokens.get(*i) == Some(&Token::Punct(',')) {
                    *i += 1;
                } else if tokens.get(*i) != Some(&Token::Punct('}')) {
                    return reject(
                        "MODULE.bazel: ambiguous crate annotation/override configuration",
                    );
                }
            }
            *i += 1;
            Ok(Literal::Dict(values))
        }
        _ => reject("MODULE.bazel: ambiguous crate annotation/override configuration"),
    }
}
#[derive(Default)]
struct Annotation {
    normal: Option<BTreeSet<String>>,
    selected: BTreeSet<String>,
}
type Annotations = BTreeMap<String, BTreeMap<String, Annotation>>;
fn annotations(path: &Path) -> Result<Annotations> {
    let tokens = tokens(&read(path)?)?;
    let mut output: Annotations = BTreeMap::new();
    let harmless = [
        "crate",
        "version",
        "repositories",
        "triples",
        "gen_binaries",
        "crate_features",
        "tags",
        "visibility",
        "build_script_tags",
    ];
    for i in 0..tokens.len().saturating_sub(3) {
        if !matches!(&tokens[i], Token::Word(_))
            || tokens[i + 1] != Token::Punct('.')
            || tokens[i + 3] != Token::Punct('(')
        {
            continue;
        }
        let Token::Word(kind) = &tokens[i + 2] else {
            continue;
        };
        if !["annotation", "annotation_select", "override"].contains(&kind.as_str()) {
            continue;
        }
        let mut p = i + 4;
        let mut settings = BTreeMap::new();
        while tokens.get(p) != Some(&Token::Punct(')')) {
            let Some(Token::Word(key)) = tokens.get(p) else {
                return reject("MODULE.bazel: ambiguous crate annotation/override identity");
            };
            let key = key.clone();
            p += 1;
            if tokens.get(p) != Some(&Token::Punct('=')) {
                return reject("MODULE.bazel: ambiguous crate annotation/override configuration");
            }
            p += 1;
            let value = literal(&tokens, &mut p)?;
            settings.insert(key, value);
            if tokens.get(p) == Some(&Token::Punct(',')) {
                p += 1;
            } else if tokens.get(p) != Some(&Token::Punct(')')) {
                return reject("MODULE.bazel: ambiguous crate annotation/override configuration");
            }
        }
        let Some(name) = settings.get("crate").and_then(Literal::string) else {
            return reject("MODULE.bazel: ambiguous crate annotation/override identity");
        };
        if settings
            .get("repositories")
            .is_some_and(|v| v.truthy() && !v.contains("crates"))
        {
            continue;
        }
        let version = settings
            .get("version")
            .and_then(Literal::string)
            .filter(|v| !v.is_empty())
            .unwrap_or("*");
        let mut mutations: BTreeSet<String> = settings
            .iter()
            .filter(|(key, value)| {
                !harmless.contains(&key.as_str())
                    && value.truthy()
                    && !(key.as_str() == "gen_build_script"
                        && matches!(value.string(), Some("auto" | "on")))
            })
            .map(|(key, _)| key.clone())
            .collect();
        let entry = output
            .entry(name.into())
            .or_default()
            .entry(version.into())
            .or_default();
        if kind == "annotation_select" {
            if settings.get("triples").is_some_and(|v| v.contains(TARGET)) {
                entry.selected.extend(mutations);
            }
        } else {
            if kind == "override" {
                mutations.insert("override".into());
            }
            entry.normal = Some(mutations);
        }
    }
    Ok(output)
}
fn mutations(annotations: &Annotations, name: &str, version: &str) -> BTreeSet<String> {
    let Some(versions) = annotations.get(name) else {
        return BTreeSet::new();
    };
    let wildcard = versions.get("*");
    let exact = versions.get(version);
    let mut result = exact
        .and_then(|v| v.normal.as_ref())
        .or_else(|| wildcard.and_then(|v| v.normal.as_ref()))
        .cloned()
        .unwrap_or_default();
    for entry in [wildcard, exact].into_iter().flatten() {
        result.extend(entry.selected.iter().cloned());
    }
    result
}

fn edition(value: &str) -> bool {
    matches!(value, "2015" | "2018" | "2021" | "2024")
}
fn identifier(value: &str, first_letter: bool) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || (!first_letter && c == '_'))
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
fn quoted<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
// Cargo include entries use gitignore syntax, rather than literal file paths.
// Anchor every declared file at the package root and quote pattern metacharacters.
fn cargo_include_pattern(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '{' | '}' | ' ' | '\t') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn target_path(target: &CrateMetadata) -> Result<PackagePath> {
    target.path.clone().ok_or_else(|| {
        GenerationError::Policy(format!("{}: missing required package path", target.label))
    })
}

fn validate_production(spec: &AnalysisSpec) -> Result<&str> {
    if spec.schema_version != SCHEMA_VERSION {
        return reject("unsupported generation input schema_version");
    }
    let production: Vec<_> = spec.library.iter().chain(spec.binaries.iter()).collect();
    let Some(first) = production.first() else {
        return reject("cargo_publish requires a library or binary");
    };
    let label = first.label.as_str();
    if spec.target != TARGET {
        return reject(format!(
            "{label}: unsupported target platform {}",
            spec.target
        ));
    }
    if let Some(lib) = &spec.library {
        if lib.crate_type != "rlib" {
            return reject(format!(
                "{}: unsupported library crate type {}",
                lib.label, lib.crate_type
            ));
        }
        if lib.edition != spec.package.edition {
            return reject(format!("{}: ambiguous library/package edition", lib.label));
        }
    }
    let mut names = BTreeSet::new();
    for binary in &spec.binaries {
        if binary.crate_type != "bin" {
            return reject(format!(
                "{}: unsupported binary crate type {}",
                binary.label, binary.crate_type
            ));
        }
        let name = binary.name.as_deref().unwrap_or_default();
        if !identifier(name, false) || !names.insert(name) {
            return reject(format!(
                "{}: invalid or conflicting Cargo binary name {name}",
                binary.label
            ));
        }
        if !edition(&binary.edition) {
            return reject(format!(
                "{}: unsupported binary edition {}",
                binary.label, binary.edition
            ));
        }
    }
    Ok(label)
}

fn package_fields(package: &PackageMetadata) -> BTreeMap<&'static str, &String> {
    BTreeMap::from([
        ("name", &package.name),
        ("version", &package.version),
        ("description", &package.description),
        ("license", &package.license),
        ("repository", &package.repository),
        ("rust-version", &package.rust_version),
        ("readme", &package.readme),
        ("edition", &package.edition),
    ])
}

fn validate_package(package: &PackageMetadata, label: &str) -> Result<()> {
    let metadata = package_fields(package);
    for (key, value) in &metadata {
        if value.is_empty() {
            return reject(format!("{label}: missing package metadata {key}"));
        }
    }
    if !identifier(&package.name, true) {
        return reject(format!("{label}: malformed package name {}", package.name));
    }
    if semver::Version::parse(&package.version).is_err() {
        return reject(format!(
            "{label}: malformed package version {}",
            package.version
        ));
    }
    if !edition(&package.edition) {
        return reject(format!(
            "{label}: unsupported package edition {}",
            package.edition
        ));
    }
    let rust_parts: Vec<_> = package.rust_version.split('.').collect();
    if !(2..=3).contains(&rust_parts.len())
        || rust_parts[0] != "1"
        || rust_parts[1..]
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return reject(format!(
            "{label}: malformed rust-version {}",
            package.rust_version
        ));
    }
    Ok(())
}

fn validate_dependency_closure(spec: &AnalysisSpec, annotations: &Annotations) -> Result<()> {
    let mut seen = BTreeSet::new();
    for dep in &spec.dependency_closure {
        if !dep.external || !seen.insert((&dep.label, &dep.features, &dep.version)) {
            continue;
        }
        let name = if let Some(path) = &dep.manifest {
            toml_file::<CargoManifest>(path)?.package.name
        } else {
            String::new()
        };
        let modified = mutations(
            annotations,
            &name,
            dep.version.as_deref().unwrap_or_default(),
        );
        if !modified.is_empty() {
            return reject(format!(
                "{}: unsupported Bazel annotation/override for {name}: {modified:?}",
                dep.label
            ));
        }
    }
    Ok(())
}

fn collect_dependencies(
    spec: &AnalysisSpec,
    root: &CargoManifest,
    lock: &CargoLock,
) -> Result<Vec<Dependency>> {
    let production = spec
        .library
        .iter()
        .chain(spec.binaries.iter())
        .collect::<Vec<_>>();
    let mut aliases: BTreeMap<(u8, String), Dependency> = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for (group, kind, targets) in [
        (1, DependencyKind::Normal, production),
        (0, DependencyKind::Dev, spec.tests.iter().collect()),
    ] {
        for target in targets {
            for dep in &target.dependencies {
                let alias = dep.alias.as_deref().ok_or_else(|| {
                    GenerationError::Policy(format!(
                        "{}: missing direct dependency alias",
                        target.label
                    ))
                })?;
                if let Some(lib) = &spec.library
                    && dep.label == lib.label
                {
                    if alias != lib.crate_name {
                        return reject(format!(
                            "{}: unsupported selected library alias {alias}",
                            target.label
                        ));
                    }
                    continue;
                }
                let item = dependency(dep, root, lock, kind, alias)?;
                let identity = (
                    item.package.clone(),
                    item.version.clone(),
                    item.source.clone(),
                );
                if identities
                    .get(alias)
                    .is_some_and(|previous| previous != &identity)
                {
                    return reject(format!(
                        "{}: conflicting direct dependency alias {alias}",
                        target.label
                    ));
                }
                identities.insert(alias.to_owned(), identity);
                let key = (group, alias.to_owned());
                if let Some(previous) = aliases.get_mut(&key) {
                    let features: BTreeSet<_> = previous
                        .features
                        .iter()
                        .chain(item.features.iter())
                        .cloned()
                        .collect();
                    previous.features = features.into_iter().collect();
                    previous.default_features |= item.default_features;
                    previous.target = previous.target.clone().min(item.target);
                } else {
                    aliases.insert(key, item);
                }
            }
        }
    }
    Ok(aliases.into_values().collect())
}

fn project_selection(spec: &AnalysisSpec) -> Result<Selection> {
    let library = spec
        .library
        .as_ref()
        .map(|lib| {
            Ok::<_, GenerationError>(LibraryTarget {
                label: lib.label.clone(),
                crate_name: lib.crate_name.clone(),
                crate_type: lib.crate_type.clone(),
                edition: lib.edition.clone(),
                path: target_path(lib)?,
            })
        })
        .transpose()?;
    let mut binaries = spec
        .binaries
        .iter()
        .map(|bin| {
            Ok(BinaryTarget {
                label: bin.label.clone(),
                crate_name: bin.crate_name.clone(),
                name: bin.name.clone().unwrap_or_default(),
                crate_type: bin.crate_type.clone(),
                edition: bin.edition.clone(),
                path: target_path(bin)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    binaries.sort_by(|a, b| a.name.cmp(&b.name));
    let mut tests: Vec<_> = spec
        .tests
        .iter()
        .map(|target| TestTarget {
            label: target.label.clone(),
            crate_name: (!target.crate_name.is_empty()).then(|| target.crate_name.clone()),
            crate_type: (!target.crate_type.is_empty()).then(|| target.crate_type.clone()),
            edition: (!target.edition.is_empty()).then(|| target.edition.clone()),
            crate_: target.crate_.clone(),
        })
        .collect();
    tests.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(Selection {
        library,
        binaries,
        tests,
    })
}

fn render_manifest(
    spec: &AnalysisSpec,
    selection: &Selection,
    dependencies: &[Dependency],
) -> Result<String> {
    let mut lines = vec!["[package]".to_owned()];
    for (key, value) in package_fields(&spec.package) {
        lines.push(format!("{key} = {}", quoted(value)?));
    }
    lines.extend(
        [
            "build = false",
            "autolib = false",
            "autobins = false",
            "autotests = false",
            "autoexamples = false",
            "autobenches = false",
        ]
        .map(String::from),
    );
    let mut paths: Vec<_> = spec.inputs.iter().map(|item| item.path.as_str()).collect();
    paths.sort_unstable();
    paths.insert(0, "Cargo.toml");
    let include: Vec<_> = paths.into_iter().map(cargo_include_pattern).collect();
    lines.push(format!("include = {}", quoted(&include)?));
    if let Some(lib) = &selection.library {
        lines.extend([
            String::new(),
            "[lib]".into(),
            format!("name = {}", quoted(&lib.crate_name)?),
            format!("path = {}", quoted(&lib.path)?),
        ]);
    }
    for bin in &selection.binaries {
        lines.extend([
            String::new(),
            "[[bin]]".into(),
            format!("name = {}", quoted(&bin.name)?),
            format!("path = {}", quoted(&bin.path)?),
            format!("edition = {}", quoted(&bin.edition)?),
        ]);
    }
    for (kind, section) in [
        (DependencyKind::Normal, "dependencies"),
        (DependencyKind::Dev, "dev-dependencies"),
    ] {
        let selected: Vec<_> = dependencies
            .iter()
            .filter(|item| item.kind == kind)
            .collect();
        if kind == DependencyKind::Dev && selected.is_empty() {
            continue;
        }
        lines.extend([String::new(), format!("[{section}]")]);
        for item in selected {
            let features: Vec<_> = item
                .features
                .iter()
                .filter(|f| f.as_str() != "default")
                .collect();
            lines.push(format!(
                "{} = {{ package = {}, version = {}, default-features = {}, features = {} }}",
                quoted(&item.alias)?,
                quoted(&item.package)?,
                quoted(&format!("={}", item.version))?,
                item.default_features,
                quoted(&features)?
            ));
        }
    }
    Ok(lines.join("\n") + "\n")
}

fn validate_inventory<'a>(
    spec: &'a AnalysisSpec,
    selection: &Selection,
) -> Result<BTreeMap<PackagePath, &'a PathBuf>> {
    let mut declared = BTreeMap::new();
    for item in &spec.inputs {
        PackagePath::declared(item.path.as_str())?;
        if declared.insert(item.path.clone(), &item.source).is_some() {
            return reject(format!("conflicting package path: {}", item.path));
        }
        if !item.source.is_file() {
            return reject(format!(
                "missing declared package input: {} from {}",
                item.path,
                item.source.display()
            ));
        }
    }
    for path in declared.keys() {
        let mut candidate = path.as_str();
        while let Some((parent, _)) = candidate.rsplit_once('/') {
            if declared.keys().any(|path| path.as_str() == parent) {
                return reject(format!("conflicting file/directory package path: {path}"));
            }
            candidate = parent;
        }
    }
    for required in std::iter::once(spec.package.readme.as_str())
        .chain(selection.library.iter().map(|lib| lib.path.as_str()))
        .chain(selection.binaries.iter().map(|bin| bin.path.as_str()))
    {
        if !declared.keys().any(|path| path.as_str() == required) {
            return reject(format!("missing required package path: {required}"));
        }
    }
    Ok(declared)
}

fn stage_inputs(
    output: &Path,
    declared: BTreeMap<PackagePath, &PathBuf>,
) -> Result<Vec<InputFile>> {
    io(output, fs::create_dir_all(output))?;
    let mut inputs = Vec::new();
    for (path, source) in declared {
        let dest = output.join(path.as_str());
        if let Some(parent) = dest.parent() {
            io(parent, fs::create_dir_all(parent))?;
        }
        io(&dest, fs::copy(source, &dest))?;
        inputs.push(InputFile {
            path,
            sha256: digest_file(&dest)?,
            size: io(&dest, fs::metadata(&dest))?.len(),
        });
    }
    Ok(inputs)
}

/// Generate the declared manifest, package files and schema-1 contract.
///
/// # Errors
/// Returns an error for unsupported metadata or provenance, conflicting inputs,
/// malformed declarations, or filesystem failures.
pub fn generate(options: &GenerateOptions) -> Result<()> {
    let spec: AnalysisSpec = read_json(&options.spec).map_err(|error| {
        GenerationError::Policy(format!(
            "{error}; package paths must be relative and cannot escape the package"
        ))
    })?;
    let label = validate_production(&spec)?;
    let root: CargoManifest = toml_file(&options.root_manifest)?;
    let lock: CargoLock = toml_file(&options.root_lockfile)?;
    let annotations = annotations(&options.module)?;
    validate_package(&spec.package, label)?;
    validate_dependency_closure(&spec, &annotations)?;
    let dependencies = collect_dependencies(&spec, &root, &lock)?;
    let selection = project_selection(&spec)?;
    let manifest = render_manifest(&spec, &selection, &dependencies)?;
    let declared = validate_inventory(&spec, &selection)?;
    let provenance = Provenance {
        root_manifest_sha256: digest_file(&options.root_manifest)?,
        root_lockfile_sha256: digest_file(&options.root_lockfile)?,
        module_sha256: digest_file(&options.module)?,
    };
    selection.selected_targets()?;
    let inputs = stage_inputs(&options.output, declared)?;
    let manifest_path = options.output.join("Cargo.toml");
    io(&manifest_path, fs::write(&manifest_path, &manifest))?;
    let contract = GenerationContract {
        schema_version: SCHEMA_VERSION,
        package: spec.package,
        target: spec.target,
        selection,
        dependencies,
        inputs,
        manifest_sha256: digest_bytes(manifest.as_bytes()),
        provenance: Some(provenance),
    };
    write_json(&options.contract, &contract)?;
    Ok(())
}
