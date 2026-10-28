//! Serialize callers and retain durable request intents outside checkouts.
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Commit(String);
impl TryFrom<String> for Commit {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        ensure!(
            value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
            "commit must be a full SHA-1"
        );
        Ok(Self(value.to_ascii_lowercase()))
    }
}
impl From<Commit> for String {
    fn from(value: Commit) -> Self {
        value.0
    }
}
impl Commit {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);
impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RequestId {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        let parsed = Uuid::parse_str(&value)?;
        ensure!(
            !parsed.is_nil() && parsed.to_string() == value,
            "invalid request UUID"
        );
        Ok(Self(value))
    }
}
impl From<RequestId> for String {
    fn from(value: RequestId) -> Self {
        value.0
    }
}
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub id: RequestId,
    pub head: Commit,
    pub at: u64,
    pub actor: u64,
    pub comment: Option<u64>,
}
impl Request {
    pub fn body(&self) -> String {
        format!(
            "@codex review\n\nReview commit `{}`.\n\n<!-- hosted-review-request:v1 {} {} -->",
            self.head.as_str(),
            self.id.as_str(),
            self.head.as_str()
        )
    }
}
#[derive(Serialize, Deserialize)]
pub struct State {
    pub version: u8,
    pub repository: String,
    pub pr: u64,
    pub requests: Vec<Request>,
}
pub struct Store {
    directory: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(root: &Path, repository: &str, pr: u64) -> Result<Self> {
        let absolute = std::env::current_dir()?.join(root);
        let existing = absolute
            .ancestors()
            .find(|p| p.exists())
            .context("state path has no ancestor")?
            .canonicalize()?;
        ensure!(
            !existing.ancestors().any(|p| p.join(".git").exists()),
            "state must be outside Git checkouts"
        );
        ensure!(!root.is_symlink(), "state directory cannot be a symlink");
        private_directory(root)?;
        let mut directory = root.to_path_buf();
        let number = pr.to_string();
        for segment in repository
            .split('/')
            .chain(std::iter::once(number.as_str()))
        {
            directory.push(segment);
            private_directory(&directory)?;
        }
        let lock_path = directory.join("lock");
        ensure!(!lock_path.is_symlink(), "state lock cannot be a symlink");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(lock_path)?;
        lock.try_lock()
            .context("another caller holds the review lock")?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }
    pub fn read(&self, repository: &str, pr: u64) -> Result<State> {
        let path = self.directory.join("state.json");
        ensure!(!path.is_symlink(), "state file cannot be a symlink");
        if !path.try_exists()? {
            return Ok(State {
                version: 1,
                repository: repository.into(),
                pr,
                requests: Vec::new(),
            });
        }
        let state: State = serde_json::from_reader(File::open(path)?)?;
        ensure!(
            state.version == 1 && state.repository == repository && state.pr == pr,
            "saved review identity does not match"
        );
        let mut ids = std::collections::BTreeSet::new();
        for request in &state.requests {
            ensure!(
                ids.insert(request.id.as_str()) && request.actor > 0 && request.comment != Some(0),
                "invalid saved request identity"
            );
        }
        Ok(state)
    }
    pub fn save(&self, state: &State) -> Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer_pretty(&mut temporary, state)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join("state.json"))
            .context("cannot persist review state")?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
fn private_directory(path: &Path) -> Result<()> {
    if !path.try_exists()? {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            private_directory(parent)?;
        }
        DirBuilder::new().mode(0o700).create(path)?;
    }
    check_directory(path)?;
    // Synchronize the path to the existing root before any remote effect.
    for directory in path.canonicalize()?.ancestors() {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}
fn check_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && metadata.permissions().mode().trailing_zeros() >= 6,
        "review state directory must be private"
    );
    Ok(())
}
