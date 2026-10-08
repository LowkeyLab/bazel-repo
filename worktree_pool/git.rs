//! Byte-preserving observations of ordinary Git repositories. No shell commands.
use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::error::PoolError;

pub struct Checkout {
    pub common_directory: PathBuf,
    pub path: PathBuf,
    pub git_directory: PathBuf,
}
fn command(path: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("--no-optional-locks").arg("-C").arg(path);
    for name in [
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES",
        "GIT_DISCOVERY_ACROSS_FILESYSTEM",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG",
        "GIT_SHALLOW_FILE",
        "GIT_GRAFT_FILE",
        "GIT_REPLACE_REF_BASE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_LITERAL_PATHSPECS",
        "GIT_GLOB_PATHSPECS",
        "GIT_NOGLOB_PATHSPECS",
        "GIT_ICASE_PATHSPECS",
    ] {
        command.env_remove(name);
    }
    command
}
fn observe_path(path: &Path, argument: &str) -> Result<PathBuf, PoolError> {
    let output = command(path)
        .args(["rev-parse", "--path-format=absolute", argument])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() || output.stdout.last() != Some(&b'\n') {
        return Err(PoolError::Git);
    }
    let mut bytes = output.stdout;
    bytes.pop();
    if bytes.is_empty() || bytes.contains(&0) {
        return Err(PoolError::Git);
    }
    fs::canonicalize(PathBuf::from(OsString::from_vec(bytes))).map_err(|_| PoolError::Git)
}
/// # Errors
/// Rejects inaccessible paths, bare repositories and malformed Git observations.
pub fn checkout(path: &Path) -> Result<Checkout, PoolError> {
    let root = observe_path(path, "--show-toplevel")?;
    ensure_registered_path(&root)?;
    Ok(Checkout {
        common_directory: observe_path(&root, "--git-common-dir")?,
        git_directory: observe_path(&root, "--absolute-git-dir")?,
        path: root,
    })
}
/// Refreshes only the recorded canonical common-directory identity.
/// # Errors
/// Refuses substituted/missing contexts and failed Git effects without stale fallback.
pub fn refresh_origin_main(common_directory: &Path) -> Result<String, PoolError> {
    if observe_path(common_directory, "--git-common-dir")? != common_directory {
        return Err(PoolError::Git);
    }
    if !fetch(common_directory)?.status.success() {
        return Err(PoolError::Git);
    }
    origin_main(common_directory)
}
/// # Errors
/// Rejects failures without treating a stale remote-tracking ref as fresh.
fn fetch(path: &Path) -> Result<Output, PoolError> {
    command(path)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsync=objects,reference",
            "-c",
            "core.fsyncMethod=fsync",
            "fetch",
            "--no-prune",
            "--no-prune-tags",
            "--no-tags",
            "--no-recurse-submodules",
            "--no-auto-maintenance",
            "--no-write-fetch-head",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main",
        ])
        .output()
        .map_err(|_| PoolError::Git)
}

/// # Errors
/// Requires an existing full commit; malformed or failed resolution is uncertainty.
fn origin_main(path: &Path) -> Result<String, PoolError> {
    let output = command(path)
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            "refs/remotes/origin/main^{commit}",
        ])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    let oid = output.stdout.strip_suffix(b"\n").ok_or(PoolError::Git)?;
    if !matches!(oid.len(), 40 | 64)
        || !oid
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(PoolError::Git);
    }
    String::from_utf8(oid.to_vec()).map_err(|_| PoolError::Git)
}

fn ensure_registered_path(root: &Path) -> Result<(), PoolError> {
    use std::{collections::HashSet, os::unix::ffi::OsStrExt};
    let output = command(root)
        .args(["worktree", "list", "--porcelain", "-z"])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() || !output.stdout.ends_with(b"\0\0") {
        return Err(PoolError::Git);
    }
    let mut seen = HashSet::new();
    let mut path = None;
    let mut matched = false;
    let mut attributes: HashSet<Vec<u8>> = HashSet::new();
    for attribute in output.stdout[..output.stdout.len() - 1].split(|b| *b == 0) {
        if attribute.is_empty() {
            let bare = attributes.contains(b"bare".as_slice());
            if (!bare
                && (!attributes.contains(b"HEAD".as_slice())
                    || attributes.contains(b"branch".as_slice())
                        == attributes.contains(b"detached".as_slice())))
                || (bare
                    && attributes
                        .iter()
                        .any(|label| matches!(label.as_slice(), b"HEAD" | b"branch" | b"detached")))
            {
                return Err(PoolError::Git);
            }
            attributes.clear();
            let recorded: Vec<u8> = path.take().ok_or(PoolError::Git)?;
            if !seen.insert(recorded.clone()) {
                return Err(PoolError::Git);
            }
            let candidate = PathBuf::from(OsString::from_vec(recorded));
            matched |= candidate.as_os_str().as_bytes() == root.as_os_str().as_bytes()
                || fs::canonicalize(candidate).is_ok_and(|p| p == root);
        } else if path.is_none() {
            let recorded = attribute.strip_prefix(b"worktree ").ok_or(PoolError::Git)?;
            if !PathBuf::from(OsString::from_vec(recorded.to_vec())).is_absolute() {
                return Err(PoolError::Git);
            }
            path = Some(recorded.to_vec());
        } else {
            let mut fields = attribute.splitn(2, |b| *b == b' ');
            let label = fields.next().ok_or(PoolError::Git)?;
            let value = fields.next();
            let valid = match (label, value) {
                (b"HEAD", Some(oid)) => {
                    matches!(oid.len(), 40 | 64)
                        && oid
                            .iter()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                }
                (b"branch", Some(reference)) => {
                    reference.starts_with(b"refs/") && reference.len() > 5
                }
                (b"bare" | b"detached", None) | (b"locked" | b"prunable", _) => true,
                _ => false,
            };
            if !valid || !attributes.insert(label.to_vec()) {
                return Err(PoolError::Git);
            }
        }
    }
    if path.is_some() || !matched {
        return Err(PoolError::Git);
    }
    Ok(())
}
