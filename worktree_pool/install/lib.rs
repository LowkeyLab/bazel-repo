//! Checksum-pinned upstream native source archive extraction.
#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Cursor, Read, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use cargo_publish_core::{Sha256, digest_bytes};
use flate2::read::GzDecoder;

pub const GIT_SHA256: &str = "af5ebfc1658464f5d0d45a2bfd884c935fb607a10cc021d95bc80778861cc1d3";
pub const ZLIB_SHA256: &str = "9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23";

#[derive(Debug, thiserror::Error)]
pub enum NativeArchiveError {
    #[error("native archive policy: {0}")]
    Policy(String),
    #[error("native archive I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
fn io_error(path: impl AsRef<Path>, source: std::io::Error) -> NativeArchiveError {
    NativeArchiveError::Io {
        path: path.as_ref().to_path_buf(),
        source,
    }
}
fn policy(message: impl Into<String>) -> NativeArchiveError {
    NativeArchiveError::Policy(message.into())
}

fn decoded(bytes: Vec<u8>, source: &Path) -> Result<Vec<u8>, NativeArchiveError> {
    if bytes.starts_with(b"\x1f\x8b") {
        let mut data = Vec::new();
        GzDecoder::new(Cursor::new(bytes))
            .read_to_end(&mut data)
            .map_err(|error| io_error(source, error))?;
        return Ok(data);
    }
    if !bytes.starts_with(b"\xfd7zXZ\0") {
        return Err(policy(
            "unsupported native source compression; expected gzip or XZ",
        ));
    }
    // Decode the already hashed bytes, not a second read of the source path.
    // Clear XZ_OPT/XZ_DEFAULTS and all unrelated ambient configuration.
    let mut command = Command::new("xz");
    command
        .args(["--decompress", "--stdout", "--threads=1", "--", "-"])
        .env_clear()
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in ["PATH", "NIX_LD", "NIX_LD_LIBRARY_PATH"] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().map_err(|error| io_error("xz", error))?;
    let mut input = child.stdin.take().expect("piped xz stdin");
    // Drain stdout concurrently so a compressed input larger than a pipe cannot deadlock.
    let writer = thread::spawn(move || input.write_all(&bytes));
    let result = child
        .wait_with_output()
        .map_err(|error| io_error("xz", error))?;
    let written = writer
        .join()
        .map_err(|_| policy("xz input writer panicked"))?;
    if !result.status.success() {
        return Err(policy(format!(
            "xz decoder failed for {} ({}): {}",
            source.display(),
            result.status,
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    written.map_err(|error| io_error("xz stdin", error))?;
    Ok(result.stdout)
}
#[derive(Debug)]
enum Kind {
    File(Vec<u8>),
    Directory,
    Symbolic(PathBuf),
    Hard(PathBuf),
}
#[derive(Debug)]
struct Member {
    kind: Kind,
    mode: u32,
}
fn member_path(path: &Path) -> Result<PathBuf, NativeArchiveError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => (),
            _ => {
                return Err(policy(format!(
                    "native archive traversal or absolute path: {}",
                    path.display()
                )));
            }
        }
    }
    if result.as_os_str().is_empty() {
        return Err(policy("empty native archive path"));
    }
    Ok(result)
}
fn link_path(path: &Path, target: &Path, symbolic: bool) -> Result<PathBuf, NativeArchiveError> {
    let mut result = if symbolic {
        path.parent().unwrap_or(Path::new("")).to_path_buf()
    } else {
        PathBuf::new()
    };
    for component in target.components() {
        match component {
            Component::Normal(name) => result.push(name),
            Component::CurDir => (),
            Component::ParentDir if result.components().count() > 1 => {
                result.pop();
            }
            _ => {
                return Err(policy(format!(
                    "external native archive link: {} -> {}",
                    path.display(),
                    target.display()
                )));
            }
        }
    }
    if result.components().next() != path.components().next() {
        return Err(policy(format!(
            "external native archive link: {} -> {}",
            path.display(),
            target.display()
        )));
    }
    Ok(result)
}
fn resolved(
    path: &Path,
    members: &BTreeMap<PathBuf, Member>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<PathBuf, NativeArchiveError> {
    if !visited.insert(path.to_path_buf()) {
        return Err(policy(format!(
            "native archive link cycle: {}",
            path.display()
        )));
    }
    for ancestor in path
        .ancestors()
        .skip(1)
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if let Some(member) = members.get(ancestor) {
            if !matches!(member.kind, Kind::Directory) {
                return Err(policy(format!(
                    "native archive link/file parent: {}",
                    ancestor.display()
                )));
            }
        }
    }
    match members.get(path).map(|member| &member.kind) {
        Some(Kind::Symbolic(target)) => resolved(&link_path(path, target, true)?, members, visited),
        Some(Kind::Hard(target)) => resolved(&link_path(path, target, false)?, members, visited),
        Some(_) => Ok(path.to_path_buf()),
        None if members.keys().any(|member| member.starts_with(path)) => Ok(path.to_path_buf()),
        None => Err(policy(format!(
            "missing internal native archive link target: {}",
            path.display()
        ))),
    }
}

/// Check exact compressed bytes and preflight every member before creating the
/// new private destination. Upstream sources may contain safe internal links;
/// Cargo application archives have a separate, link-free policy in the core.
pub fn extract_pinned_source(
    archive: &Path,
    expected: &Sha256,
    destination: &Path,
) -> Result<PathBuf, NativeArchiveError> {
    let bytes = fs::read(archive).map_err(|error| io_error(archive, error))?;
    let actual = digest_bytes(&bytes);
    if &actual != expected {
        return Err(policy(format!(
            "checksum mismatch for {}: expected {expected}, actual {actual}",
            archive.display()
        )));
    }
    let mut source = tar::Archive::new(Cursor::new(decoded(bytes, archive)?));
    let mut members = BTreeMap::new();
    let mut roots = BTreeSet::new();
    for item in source.entries().map_err(|error| io_error(archive, error))? {
        let mut entry = item.map_err(|error| io_error(archive, error))?;
        let path = member_path(&entry.path().map_err(|error| io_error(archive, error))?)?;
        roots.insert(
            path.components()
                .next()
                .expect("nonempty path")
                .as_os_str()
                .to_os_string(),
        );
        let kind = entry.header().entry_type();
        let mode = entry
            .header()
            .mode()
            .map_err(|error| io_error(archive, error))?
            & 0o777;
        let kind = if kind.is_file() {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|error| io_error(archive, error))?;
            Kind::File(data)
        } else if kind.is_dir() {
            Kind::Directory
        } else if kind.is_symlink() || kind.is_hard_link() {
            let target = entry
                .link_name()
                .map_err(|error| io_error(archive, error))?
                .ok_or_else(|| policy("native archive link without target"))?
                .into_owned();
            if kind.is_symlink() {
                Kind::Symbolic(target)
            } else {
                Kind::Hard(target)
            }
        } else {
            return Err(policy(format!(
                "special native archive member: {}",
                path.display()
            )));
        };
        if members
            .insert(path.clone(), Member { kind, mode })
            .is_some()
        {
            return Err(policy(format!(
                "duplicate native archive member: {}",
                path.display()
            )));
        }
    }
    if roots.len() != 1 {
        return Err(policy("native archive must have exactly one source root"));
    }
    let root = PathBuf::from(roots.pop_first().expect("one root"));
    if members
        .get(&root)
        .is_some_and(|member| !matches!(member.kind, Kind::Directory))
    {
        return Err(policy("native archive source root must be a directory"));
    }
    for (path, member) in &members {
        let resolved_path = resolved(path, &members, &mut BTreeSet::new())?;
        if matches!(member.kind, Kind::Hard(_))
            && !matches!(
                members.get(&resolved_path).map(|item| &item.kind),
                Some(Kind::File(_))
            )
        {
            return Err(policy(format!(
                "native archive hard link requires regular file: {}",
                path.display()
            )));
        }
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(destination)
        .map_err(|error| io_error(destination, error))?;
    for (path, member) in &members {
        let full = destination.join(path);
        fs::create_dir_all(full.parent().expect("rooted source member"))
            .map_err(|error| io_error(&full, error))?;
        match &member.kind {
            Kind::Directory => {
                fs::create_dir_all(&full).map_err(|error| io_error(&full, error))?;
            }
            Kind::File(data) => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&full)
                    .map_err(|error| io_error(&full, error))?;
                file.write_all(data)
                    .map_err(|error| io_error(&full, error))?;
                fs::set_permissions(&full, fs::Permissions::from_mode(member.mode))
                    .map_err(|error| io_error(&full, error))?;
            }
            _ => (),
        }
    }
    for (path, member) in &members {
        if matches!(member.kind, Kind::Hard(_)) {
            let target = resolved(path, &members, &mut BTreeSet::new())?;
            fs::hard_link(destination.join(target), destination.join(path))
                .map_err(|error| io_error(path, error))?;
        }
    }
    for (path, member) in &members {
        if let Kind::Symbolic(target) = &member.kind {
            symlink(target, destination.join(path)).map_err(|error| io_error(path, error))?;
        }
    }
    Ok(destination.join(root))
}
