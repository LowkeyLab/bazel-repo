//! Byte-preserving observations of ordinary Git repositories. No shell commands.
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
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
    command
        .arg("--no-replace-objects")
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .arg("-C")
        .arg(path);
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

pub struct Safety {
    pub head: String,
    pub branch: Option<String>,
}

/// Resolves once, preserving non-UTF-8 reference arguments until Git parses them.
/// # Errors
/// Rejects missing references and malformed commit observations.
pub fn resolve_commit(path: &Path, reference: &OsStr) -> Result<String, PoolError> {
    let mut peeled = reference.as_bytes().to_vec();
    peeled.extend_from_slice(b"^{commit}");
    let output = command(path)
        .args(["rev-parse", "--verify", "--end-of-options"])
        .arg(OsString::from_vec(peeled))
        .output()
        .map_err(|_| PoolError::Git)?;
    output_oid(&output)
}

fn output_oid(output: &Output) -> Result<String, PoolError> {
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    let oid = output.stdout.strip_suffix(b"\n").ok_or(PoolError::Git)?;
    if !valid_oid(oid) {
        return Err(PoolError::Git);
    }
    String::from_utf8(oid.to_vec()).map_err(|_| PoolError::Git)
}

fn valid_oid(oid: &[u8]) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

/// # Errors
/// Rejects protected dirty state or failed observations.
pub fn observe_safe(path: &Path, target: &str) -> Result<Safety, PoolError> {
    no_operation(path)?;
    let output = command(path)
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    if !output.stdout.is_empty() {
        return Err(PoolError::UnfinishedWork);
    }
    let tracked = indexed_paths(path)?;
    no_retained_collisions(path, &tracked, &target_paths(path, target)?)?;
    let head = resolve_commit(path, std::ffi::OsStr::new("HEAD"))?;
    let output = command(path)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(|_| PoolError::Git)?;
    let branch = match output.status.code() {
        Some(0) => Some(
            String::from_utf8(
                output
                    .stdout
                    .strip_suffix(b"\n")
                    .ok_or(PoolError::Git)?
                    .to_vec(),
            )
            .map_err(|_| PoolError::Git)?,
        ),
        Some(1) => None,
        _ => return Err(PoolError::Git),
    };
    Ok(Safety { head, branch })
}
/// Performs an ordinary detached checkout of the fixed resolved commit.
/// # Errors
/// Reports Git failures without forcing, resetting, stashing or deleting work.
pub fn detach(path: &Path, target: &str) -> Result<(), PoolError> {
    if !valid_oid(target.as_bytes()) {
        return Err(PoolError::Git);
    }
    let output = command(path)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "checkout",
            "--detach",
            "--no-overwrite-ignore",
            "--no-recurse-submodules",
            target,
            "--",
        ])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    Ok(())
}

/// Hardens a new reachability root in the tool-owned preservation namespace.
/// # Errors
/// Refuses existing refs, invalid identities, failed writes or mismatched results.
pub fn preserve_tip(common: &Path, operation: &str, head: &str) -> Result<String, PoolError> {
    if uuid::Uuid::parse_str(operation).is_err() || !valid_oid(head.as_bytes()) {
        return Err(PoolError::Git);
    }
    let reference = format!("refs/worktree-pool/{operation}");
    let output = command(common)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsync=reference",
            "-c",
            "core.fsyncMethod=fsync",
            "update-ref",
            "--no-deref",
            &reference,
            head,
            "",
        ])
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    let observed = output_oid(
        &command(common)
            .args(["show-ref", "--verify", "--hash", &reference])
            .output()
            .map_err(|_| PoolError::Git)?,
    )?;
    if observed != head {
        return Err(PoolError::Git);
    }
    Ok(reference)
}

/// Observes the exact recorded preservation root without rewriting it.
/// # Errors
/// Rejects missing, changed or unreadable preservation evidence.
pub fn verify_preserved_tip(common: &Path, operation: &str, head: &str) -> Result<(), PoolError> {
    if uuid::Uuid::parse_str(operation).is_err() || !valid_oid(head.as_bytes()) {
        return Err(PoolError::Git);
    }
    let reference = format!("refs/worktree-pool/{operation}");
    let symbolic = command(common)
        .args(["symbolic-ref", "--quiet", &reference])
        .output()
        .map_err(|_| PoolError::Git)?;
    if symbolic.status.code() != Some(1) {
        return Err(PoolError::Git);
    }
    let observed = output_oid(
        &command(common)
            .args(["show-ref", "--verify", "--hash", &reference])
            .output()
            .map_err(|_| PoolError::Git)?,
    )?;
    if observed != head {
        return Err(PoolError::Git);
    }
    Ok(())
}

fn observe_output(path: &Path, arguments: &[&str]) -> Result<Vec<u8>, PoolError> {
    let output = command(path)
        .args(arguments)
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    Ok(output.stdout)
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, PoolError> {
    match fs::symlink_metadata(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(PoolError::Git),
    }
}

fn nul_records(bytes: &[u8]) -> Result<impl Iterator<Item = &[u8]>, PoolError> {
    if !bytes.is_empty() && !bytes.ends_with(b"\0") {
        return Err(PoolError::Git);
    }
    Ok(bytes.split(|b| *b == 0).filter(|record| !record.is_empty()))
}

fn relative_path(bytes: &[u8]) -> Result<PathBuf, PoolError> {
    let path = PathBuf::from(OsString::from_vec(bytes.to_vec()));
    if bytes.is_empty()
        || bytes
            .split(|b| *b == b'/')
            .any(|c| c.is_empty() || c == b"." || c == b"..")
        || !path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err(PoolError::Git);
    }
    Ok(path)
}

fn indexed_paths(path: &Path) -> Result<HashSet<PathBuf>, PoolError> {
    let flags = observe_output(path, &["ls-files", "-v", "-z"])?;
    for entry in nul_records(&flags)? {
        if !entry.starts_with(b"H ") {
            return Err(PoolError::UnsupportedIndex);
        }
        relative_path(&entry[2..])?;
    }
    let entries = observe_output(path, &["ls-files", "--sparse", "--stage", "-z"])?;
    let mut paths = HashSet::new();
    for entry in nul_records(&entries)? {
        let mut parts = entry.splitn(2, |b| *b == b'\t');
        let header = parts.next().ok_or(PoolError::Git)?;
        let name = parts.next().ok_or(PoolError::Git)?;
        let fields: Vec<_> = header.split(|b| *b == b' ').collect();
        // Gitlinks and sparse directories require nested/special-state support.
        if fields.len() != 3
            || !matches!(fields[0], b"100644" | b"100755" | b"120000")
            || !valid_oid(fields[1])
            || fields[2] != b"0"
            || !paths.insert(relative_path(name)?)
        {
            return Err(PoolError::Git);
        }
        let relative = relative_path(name)?;
        let actual = path.join(&relative);
        let metadata = fs::symlink_metadata(&actual).map_err(|_| PoolError::Git)?;
        if fields[0] == b"120000" {
            if !metadata.file_type().is_symlink() {
                return Err(PoolError::Git);
            }
            let expected = observe_output(
                path,
                &[
                    "cat-file",
                    "blob",
                    std::str::from_utf8(fields[1]).map_err(|_| PoolError::Git)?,
                ],
            )?;
            if fs::read_link(&actual)
                .map_err(|_| PoolError::Git)?
                .as_os_str()
                .as_bytes()
                != expected
            {
                return Err(PoolError::Git);
            }
        } else {
            use std::os::unix::fs::PermissionsExt;
            if !metadata.is_file()
                || (metadata.permissions().mode() & 0o100 != 0) != (fields[0] == b"100755")
            {
                return Err(PoolError::Git);
            }
            let oid = output_oid(
                &command(path)
                    .args(["hash-object", "--no-filters", "--"])
                    .arg(&actual)
                    .output()
                    .map_err(|_| PoolError::Git)?,
            )?;
            if oid.as_bytes() != fields[1] {
                return Err(PoolError::Git);
            }
        }
    }
    Ok(paths)
}

fn target_paths(path: &Path, target: &str) -> Result<Vec<PathBuf>, PoolError> {
    if !valid_oid(target.as_bytes()) {
        return Err(PoolError::Git);
    }
    let bytes = observe_output(path, &["ls-tree", "-r", "-z", target])?;
    let mut paths = HashSet::new();
    for entry in nul_records(&bytes)? {
        let mut parts = entry.splitn(2, |b| *b == b'\t');
        let header = parts.next().ok_or(PoolError::Git)?;
        let name = parts.next().ok_or(PoolError::Git)?;
        let fields: Vec<_> = header.split(|b| *b == b' ').collect();
        if fields.len() != 3
            || !matches!(fields[0], b"100644" | b"100755" | b"120000")
            || fields[1] != b"blob"
            || !valid_oid(fields[2])
            || !paths.insert(relative_path(name)?)
        {
            return Err(PoolError::Git);
        }
    }
    Ok(paths.into_iter().collect())
}

// A replaced tracked directory is safe only when every descendant is tracked or
// a structural directory. In particular, retained empty directories are work.
fn only_tracked_descendants(
    root: &Path,
    relative: &Path,
    tracked: &HashSet<PathBuf>,
) -> Result<(), PoolError> {
    if !tracked
        .iter()
        .any(|p| p.starts_with(relative) && p != relative)
    {
        return Err(PoolError::RetainedCollision);
    }
    for entry in fs::read_dir(root.join(relative)).map_err(|_| PoolError::Git)? {
        let entry = entry.map_err(|_| PoolError::Git)?;
        let child = relative.join(entry.file_name());
        let state = metadata(&root.join(&child))?.ok_or(PoolError::Git)?;
        if state.is_dir() {
            only_tracked_descendants(root, &child, tracked)?;
        } else if !tracked.contains(&child) {
            return Err(PoolError::RetainedCollision);
        }
    }
    Ok(())
}

fn no_retained_collisions(
    root: &Path,
    tracked: &HashSet<PathBuf>,
    targets: &[PathBuf],
) -> Result<(), PoolError> {
    for target in targets {
        let mut ancestors: Vec<_> = target
            .ancestors()
            .skip(1)
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        ancestors.reverse();
        let mut replaces_tracked_ancestor = false;
        // Walk from the root: never follow a retained or tracked symlink into
        // an external directory while examining descendants.
        for ancestor in ancestors {
            if let Some(state) = metadata(&root.join(ancestor))?
                && !state.is_dir()
            {
                if !tracked.contains(ancestor) {
                    return Err(PoolError::RetainedCollision);
                }
                replaces_tracked_ancestor = true;
                break;
            }
        }
        if replaces_tracked_ancestor {
            continue;
        }
        if let Some(state) = metadata(&root.join(target))?
            && !tracked.contains(target)
        {
            if state.is_dir() {
                only_tracked_descendants(root, target, tracked)?;
            } else {
                return Err(PoolError::RetainedCollision);
            }
        }
    }
    Ok(())
}

fn no_operation(path: &Path) -> Result<(), PoolError> {
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "BISECT_LOG",
        "BISECT_START",
        "index.lock",
    ] {
        let bytes = observe_output(
            path,
            &["rev-parse", "--path-format=absolute", "--git-path", marker],
        )?;
        let bytes = bytes.strip_suffix(b"\n").ok_or(PoolError::Git)?;
        let marker_path = PathBuf::from(OsString::from_vec(bytes.to_vec()));
        if !marker_path.is_absolute() || metadata(&marker_path)?.is_some() {
            return Err(PoolError::GitOperation);
        }
    }
    for key in ["core.sparseCheckout", "index.sparse"] {
        let output = command(path)
            .args(["config", "--bool", "--get", key])
            .output()
            .map_err(|_| PoolError::Git)?;
        match output.status.code() {
            Some(0) if output.stdout == b"false\n" => {}
            Some(1) if output.stdout.is_empty() => {}
            _ => return Err(PoolError::UnsupportedIndex),
        }
    }
    Ok(())
}

/// Creates a detached linked checkout without force, hooks, reset or branch creation.
/// # Errors
/// Refuses invalid commits, existing metadata and failed Git preparation.
pub fn add_worktree(common: &Path, path: &Path, target: &str) -> Result<(), PoolError> {
    if !valid_oid(target.as_bytes()) {
        return Err(PoolError::Git);
    }
    let output = command(common)
        .args(["worktree", "add", "--detach", "--"])
        .arg(path)
        .arg(target)
        .output()
        .map_err(|_| PoolError::Git)?;
    if !output.status.success() {
        return Err(PoolError::Git);
    }
    Ok(())
}
