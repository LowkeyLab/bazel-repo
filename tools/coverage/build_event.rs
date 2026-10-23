//! Interpret build events without imposing one report family's artifact policy on others.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Result, invalid};

/// These fields identify a configured target without collapsing independent aspect events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentity {
    pub label: String,
    pub configuration: Option<Value>,
    pub aspect: Option<Value>,
}

#[derive(Debug)]
pub struct BaselineArtifacts {
    pub identity: TargetIdentity,
    pub paths: Vec<PathBuf>,
}

/// Callers select integrity requirements explicitly. Workspace paths are lexical.
#[derive(Debug, Clone, Copy)]
pub enum ArtifactPolicy {
    WorkspaceIntegrity,
    WorkspaceUnchecked,
}

pub struct BuildEvents {
    events: Vec<Value>,
    named_sets: BTreeMap<String, Value>,
}

impl BuildEvents {
    /// Decode JSON lines and index the named sets.
    ///
    /// # Errors
    /// Return an error for invalid JSON or a named set without a string identifier.
    pub fn parse(text: &str) -> Result<Self> {
        let events: Vec<Value> = text
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
        let mut named_sets = BTreeMap::new();
        for event in &events {
            if let Some(entry) = event.get("namedSetOfFiles") {
                named_sets.insert(
                    string(&event["id"]["namedSet"]["id"])?.to_owned(),
                    entry.clone(),
                );
            }
        }
        Ok(Self { events, named_sets })
    }

    /// Select all baseline configurations, in event order, excluding aspect completions.
    ///
    /// # Errors
    /// Return an error when required roots, groups, sets, or artifacts are invalid.
    pub fn baselines(
        &self,
        roots: &BTreeSet<String>,
        group_name: &str,
        root: &Path,
        family: &str,
        policy: ArtifactPolicy,
    ) -> Result<Vec<BaselineArtifacts>> {
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for event in &self.events {
            let target = &event["id"]["targetCompleted"];
            let Some(label) = target["label"]
                .as_str()
                .filter(|label| roots.contains(*label))
            else {
                continue;
            };
            if truthy(&target["aspect"]) {
                continue;
            }
            let completed = &event["completed"];
            if !truthy(&completed["success"]) {
                return Err(invalid(format!("{family} baseline root failed: {label}")));
            }
            let mut paths = Vec::new();
            for group in optional_array(completed, "outputGroup")? {
                if string(&group["name"])? != group_name {
                    continue;
                }
                if truthy(&group["incomplete"]) {
                    return Err(invalid(format!(
                        "Incomplete {family} baseline output group: {label}"
                    )));
                }
                for reference in optional_array(group, "fileSets")? {
                    self.files(
                        string(&reference["id"])?,
                        root,
                        family,
                        policy,
                        &mut BTreeSet::new(),
                        &mut paths,
                    )?;
                }
            }
            if paths.is_empty() {
                return Err(invalid(format!(
                    "Missing {family} baseline artifact: {label}"
                )));
            }
            seen.insert(label.to_owned());
            selected.push(BaselineArtifacts {
                identity: TargetIdentity {
                    label: label.to_owned(),
                    configuration: target.get("configuration").cloned(),
                    aspect: target.get("aspect").cloned(),
                },
                paths,
            });
        }
        let missing: Vec<_> = roots.difference(&seen).collect();
        if !missing.is_empty() {
            let labels = missing
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(invalid(format!(
                "Missing {family} baseline roots: [{labels}]"
            )));
        }
        Ok(selected)
    }

    fn files(
        &self,
        id: &str,
        root: &Path,
        family: &str,
        policy: ArtifactPolicy,
        active: &mut BTreeSet<String>,
        paths: &mut Vec<PathBuf>,
    ) -> Result<()> {
        if !active.insert(id.to_owned()) {
            return Err(invalid(format!(
                "A {family} baseline artifact set contains a cycle."
            )));
        }
        let entry = self
            .named_sets
            .get(id)
            .ok_or_else(|| invalid(format!("'{id}'")))?;
        for file in optional_array(entry, "files")? {
            let mut path = root.to_path_buf();
            for prefix in optional_array(file, "pathPrefix")? {
                path.push(string(prefix)?);
            }
            path.push(string(&file["name"])?);
            if matches!(policy, ArtifactPolicy::WorkspaceIntegrity) {
                let bytes = std::fs::read(&path)?;
                let mut digest = String::with_capacity(64);
                for byte in Sha256::digest(&bytes) {
                    write!(&mut digest, "{byte:02x}").expect("Writing to a String cannot fail.");
                }
                if file["digest"].as_str() != Some(&digest)
                    || !matches_length(file.get("length"), bytes.len())
                {
                    return Err(invalid(format!(
                        "Invalid {family} baseline artifact integrity: {}",
                        path.display()
                    )));
                }
            }
            paths.push(path);
        }
        for child in optional_array(entry, "fileSets")? {
            self.files(string(&child["id"])?, root, family, policy, active, paths)?;
        }
        active.remove(id);
        Ok(())
    }
}

pub(crate) fn string(value: &Value) -> Result<&str> {
    value.as_str().ok_or_else(|| invalid("Expected a string."))
}
pub(crate) fn optional_array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    match value.get(name) {
        None => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        _ => Err(invalid(format!("Invalid array: {name}"))),
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(value) => value.as_f64() != Some(0.0),
    }
}

// Match the original command's int() conversion before comparing byte lengths.
fn matches_length(value: Option<&Value>, actual: usize) -> bool {
    match value {
        None => actual == 0,
        Some(Value::Bool(value)) => actual == usize::from(*value),
        Some(Value::Number(value)) => value.as_u64().map_or_else(
            || {
                value.as_f64().is_some_and(|number| {
                    format!("{:.0}", number.trunc())
                        .parse::<i128>()
                        .is_ok_and(|number| number == actual as i128)
                })
            },
            |number| u128::from(number) == actual as u128,
        ),
        Some(Value::String(text)) => {
            let text = text.trim();
            let bytes = text.as_bytes();
            if bytes.iter().enumerate().any(|(index, byte)| {
                *byte == b'_'
                    && (index == 0
                        || index + 1 == bytes.len()
                        || !bytes[index - 1].is_ascii_digit()
                        || !bytes[index + 1].is_ascii_digit())
            }) {
                return false;
            }
            text.replace('_', "")
                .parse::<i128>()
                .is_ok_and(|number| number == actual as i128)
        }
        _ => false,
    }
}
