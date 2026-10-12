use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use flate2::read::MultiGzDecoder;

use crate::{
    ArchiveEntry, ContractError, ExpectedPackage, InputFile, PackageMetadata, PackagePath,
    ValidationReport, digest_bytes, digest_file, read_json, validate_report,
};

use crate::report::required;

const GENERATED: [&str; 3] = ["Cargo.toml", "Cargo.toml.orig", "Cargo.lock"];
const OPTIONAL: &str = ".cargo_vcs_info.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InspectedArchive {
    pub extracted_root: PathBuf,
    pub inventory: Vec<ArchiveEntry>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedArchive {
    pub report: ValidationReport,
    pub copied_archive: PathBuf,
    pub extracted_root: PathBuf,
}

fn io_error(path: &Path, source: io::Error) -> ContractError {
    ContractError::Io {
        path: path.to_owned(),
        source,
    }
}
fn policy(message: impl Into<String>) -> ContractError {
    ContractError::Policy(message.into())
}
fn private_directory(path: &Path) -> Result<(), ContractError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|source| io_error(path, source))
}
fn new_file(path: &Path) -> Result<File, ContractError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|source| io_error(path, source))
}
fn parents(name: &str) -> impl Iterator<Item = &str> {
    name.match_indices('/').map(|(index, _)| &name[..index])
}

/// Validate declared archive contents before creating the extraction directory.
///
/// Links, special objects, ambiguous paths, duplicates, undeclared entries, and
/// file/directory conflicts never reach the filesystem.
///
/// # Errors
/// Returns an I/O error for unreadable/corrupt archives or failed extraction, and
/// a policy error for unsafe members, missing inputs, or mismatched bytes.
pub fn inspect_archive(
    archive: impl AsRef<Path>,
    package: &PackageMetadata,
    inputs: &[InputFile],
    destination: impl AsRef<Path>,
) -> Result<InspectedArchive, ContractError> {
    let root = PackagePath::new(format!("{}-{}", package.name, package.version))?;
    if root.as_str().contains('/') {
        return Err(policy(format!("unsafe package archive root: {root}")));
    }
    let declared = declared_inputs(inputs)?;
    let contents = inspect_members(archive.as_ref(), &root, &declared)?;
    validate_contents(&contents.files, &declared)?;
    extract_contents(contents, &root, destination.as_ref())
}

fn declared_inputs(inputs: &[InputFile]) -> Result<BTreeMap<&str, &InputFile>, ContractError> {
    let mut declared = BTreeMap::new();
    for input in inputs {
        PackagePath::declared(input.path.as_str())?;
        if declared.insert(input.path.as_str(), input).is_some() {
            return Err(policy(format!(
                "conflicting declared package path: {}",
                input.path
            )));
        }
    }
    for name in declared.keys() {
        if parents(name).any(|parent| declared.contains_key(parent)) {
            return Err(policy(format!(
                "conflicting archive file/directory path: {name}"
            )));
        }
    }
    Ok(declared)
}

struct ArchiveContents {
    files: BTreeMap<PackagePath, Vec<u8>>,
    inventory: Vec<ArchiveEntry>,
}

fn inspect_members(
    archive: &Path,
    root: &PackagePath,
    declared: &BTreeMap<&str, &InputFile>,
) -> Result<ArchiveContents, ContractError> {
    let allowed: BTreeSet<&str> = declared
        .keys()
        .copied()
        .chain(GENERATED)
        .chain([OPTIONAL])
        .collect();
    let directories: BTreeSet<&str> = allowed.iter().flat_map(|name| parents(name)).collect();
    // Read through the gzip trailer as well as the tar entries, so checksum or
    // truncation errors are detected before any extraction writes.
    let source = File::open(archive).map_err(|source| io_error(archive, source))?;
    let mut decoded = Vec::new();
    MultiGzDecoder::new(source)
        .read_to_end(&mut decoded)
        .map_err(|source| io_error(archive, source))?;
    let mut tar = tar::Archive::new(decoded.as_slice());
    let mut seen = BTreeSet::new();
    let mut data = BTreeMap::new();
    let mut inventory = Vec::new();
    for entry in tar.entries().map_err(|source| io_error(archive, source))? {
        let mut entry = entry.map_err(|source| io_error(archive, source))?;
        let raw = entry.path_bytes();
        let raw = std::str::from_utf8(&raw).map_err(|_| policy("non-UTF8 package archive path"))?;
        let kind = entry.header().entry_type();
        let name = if kind.is_dir() {
            raw.strip_suffix('/').unwrap_or(raw)
        } else {
            raw
        };
        let name = PackagePath::new(name)?;
        if !seen.insert(name.clone()) {
            return Err(policy(format!("duplicate archive path: {name}")));
        }
        let relative = if &name == root {
            ""
        } else {
            name.as_str()
                .strip_prefix(root.as_str())
                .and_then(|name| name.strip_prefix('/'))
                .ok_or_else(|| policy(format!("escaping or unexpected archive root: {name}")))?
        };
        if !kind.is_file() && !kind.is_dir() {
            return Err(policy(format!("archive link or special entry: {name}")));
        }
        if kind.is_dir() {
            if !relative.is_empty() {
                if !directories.contains(relative) {
                    return Err(policy(format!("unexpected archive directory: {relative}")));
                }
                inventory.push(ArchiveEntry::Directory {
                    path: PackagePath::new(relative)?,
                });
            }
            continue;
        }
        if !allowed.contains(relative) {
            return Err(policy(format!("unexpected archive file: {name}")));
        }
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|source| io_error(archive, source))?;
        let sha256 = digest_bytes(&contents);
        let size = contents.len() as u64;
        if let Some(expected) = declared.get(relative)
            && (expected.size != size || expected.sha256 != sha256)
        {
            return Err(policy(format!(
                "archive input fidelity mismatch: {relative}"
            )));
        }
        let path = PackagePath::new(relative)?;
        inventory.push(ArchiveEntry::File {
            path: path.clone(),
            sha256,
            size,
        });
        data.insert(path, contents);
    }
    Ok(ArchiveContents {
        files: data,
        inventory,
    })
}

fn validate_contents(
    files: &BTreeMap<PackagePath, Vec<u8>>,
    declared: &BTreeMap<&str, &InputFile>,
) -> Result<(), ContractError> {
    let missing: Vec<&str> = declared
        .keys()
        .copied()
        .chain(GENERATED)
        .filter(|name| !files.keys().any(|path| path.as_str() == *name))
        .collect();
    if !missing.is_empty() {
        return Err(policy(format!(
            "missing required archive files: {missing:?}"
        )));
    }
    for path in files.keys() {
        if parents(path.as_str()).any(|parent| files.keys().any(|path| path.as_str() == parent)) {
            return Err(policy(format!(
                "conflicting archive file/directory path: {path}"
            )));
        }
    }
    Ok(())
}

fn extract_contents(
    mut contents: ArchiveContents,
    root: &PackagePath,
    destination: &Path,
) -> Result<InspectedArchive, ContractError> {
    private_directory(destination)?;
    let extracted_root = destination.join(root.as_str());
    private_directory(&extracted_root)?;
    for (name, contents) in contents.files {
        let path = extracted_root.join(name.as_str());
        for parent in parents(name.as_str()) {
            let parent = extracted_root.join(parent);
            if !parent.exists() {
                private_directory(&parent)?;
            }
        }
        new_file(&path)?
            .write_all(&contents)
            .map_err(|source| io_error(&path, source))?;
    }
    contents
        .inventory
        .sort_by(|left, right| left.path().cmp(right.path()));
    Ok(InspectedArchive {
        extracted_root,
        inventory: contents.inventory,
    })
}

/// Bind an explicit report to a verified private copy of relocated archive bytes.
///
/// Copy first, then compare inventory, manifest, and lockfile fingerprints before
/// returning the verified extraction.
///
/// # Errors
/// Returns an I/O/JSON error for failed reads or writes, or a policy error for
/// invalid reports, mismatched hashes/inventories, or unsafe archive contents.
pub fn validate_verified_archive(
    report_path: impl AsRef<Path>,
    archive_path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    expected: Option<&ExpectedPackage>,
) -> Result<VerifiedArchive, ContractError> {
    let report: ValidationReport = read_json(report_path)?;
    validate_report(&report, expected)?;
    let destination = destination.as_ref();
    private_directory(destination)?;
    let copied_archive = destination.join("verified.crate");
    let archive_path = archive_path.as_ref();
    let mut source = File::open(archive_path).map_err(|source| io_error(archive_path, source))?;
    let mut copied = new_file(&copied_archive)?;
    io::copy(&mut source, &mut copied).map_err(|source| io_error(&copied_archive, source))?;
    drop(copied);
    let archive = required(report.archive.as_ref(), "archive")?;
    if digest_file(&copied_archive)? != archive.sha256
        || fs::metadata(&copied_archive)
            .map_err(|source| io_error(&copied_archive, source))?
            .len()
            != archive.size
    {
        return Err(policy("archive/report SHA256 or size mismatch"));
    }
    let inspected = inspect_archive(
        &copied_archive,
        required(report.package.as_ref(), "package")?,
        required(report.inputs.as_ref(), "inputs")?,
        destination.join("extracted"),
    )?;
    if inspected.inventory != archive.inventory {
        return Err(policy("archive/report inventory or file hash mismatch"));
    }
    if digest_file(inspected.extracted_root.join("Cargo.lock"))?
        != required(report.lockfile.as_ref(), "lockfile")?.sha256
    {
        return Err(policy("packaged lockfile/report hash mismatch"));
    }
    let manifests = required(report.manifests.as_ref(), "manifests")?;
    let original = required(manifests.original.as_ref(), "manifests.original")?;
    let normalized = required(manifests.normalized.as_ref(), "manifests.normalized")?;
    for (filename, evidence) in [("Cargo.toml.orig", original), ("Cargo.toml", normalized)] {
        if digest_file(inspected.extracted_root.join(filename))? != evidence.sha256 {
            return Err(policy(format!("{filename}/report manifest hash mismatch")));
        }
    }
    if manifests.generated.sha256 != original.sha256 {
        return Err(policy("original/generated manifest fidelity mismatch"));
    }
    Ok(VerifiedArchive {
        report,
        copied_archive,
        extracted_root: inspected.extracted_root,
    })
}
