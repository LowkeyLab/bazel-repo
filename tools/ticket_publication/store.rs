//! Private durable files and one process lock for each batch.
use crate::plan::{Batch, BatchId};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub struct Store {
    directory: PathBuf,
    _lock: File,
}
impl Store {
    pub fn open(root: &Path, batch: &BatchId, create: bool) -> Result<Self> {
        check_location(root)?;
        if create {
            private_directory(root)?;
        }
        check_private_directory(root)?;
        let directory = root.join(batch.as_str());
        if create {
            private_directory(&directory)?;
        }
        check_private_directory(&directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("lock"))?;
        lock.try_lock()
            .context("another command holds the batch lock")?;
        lock.sync_all()?;
        File::open(&directory)?.sync_all()?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }
    pub fn plan_path(&self) -> PathBuf {
        self.directory.join("batch.json")
    }
    pub fn read(&self, batch: &BatchId) -> Result<Batch> {
        let saved: Batch = serde_json::from_reader(
            File::open(self.plan_path())
                .context("batch is missing or preparation is incomplete")?,
        )?;
        saved.check(batch)?;
        Ok(saved)
    }
    pub fn save_new(&self, batch: &Batch) -> Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer_pretty(&mut temporary, batch)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(self.plan_path())
            .context("cannot commit the prepared batch")?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
}
fn check_private_directory(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path)?.is_dir(),
        "state path is not a directory"
    );
    // The low six permission bits must be zero for private Unix access.
    ensure!(
        fs::metadata(path)?.permissions().mode().trailing_zeros() >= 6,
        "state directory is not private"
    );
    created_directory(path)?;
    Ok(())
}

const CREATION_MARKER: &str = ".ticket-publication-directory";
const CREATION_VERSION: &[u8] = b"1\n";

fn created_directory(path: &Path) -> Result<bool> {
    let marker = path.join(CREATION_MARKER);
    let metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && fs::read(marker)? == CREATION_VERSION,
        "invalid directory creation marker"
    );
    Ok(true)
}

fn private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            private_directory(parent)?;
        }
        let staging = tempfile::Builder::new()
            .prefix(".ticket-publication-")
            .tempdir_in(parent)?;
        fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o700))?;
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(staging.path().join(CREATION_MARKER))?;
        marker.write_all(CREATION_VERSION)?;
        marker.sync_all()?;
        File::open(staging.path())?.sync_all()?;
        ensure!(
            !path.try_exists()?,
            "state directory appeared during preparation; retry"
        );
        fs::rename(staging.path(), path)?;
        let _ = staging.keep();
    }
    check_private_directory(path)?;
    // A marker is durable before the directory receives its final name.
    // Retry parent syncs for marked directories, then stop at the existing boundary.
    let mut directory = path.canonicalize()?;
    while created_directory(&directory)? {
        let parent = directory
            .parent()
            .context("created directory has no parent")?;
        File::open(parent)?.sync_all()?;
        directory = parent.to_path_buf();
    }
    Ok(())
}

fn check_location(path: &Path) -> Result<()> {
    let absolute = std::env::current_dir()?.join(path);
    let existing = absolute
        .ancestors()
        .find(|p| p.exists())
        .context("state path has no existing ancestor")?;
    let canonical = existing.canonicalize()?;
    ensure!(
        !canonical.ancestors().any(|p| p.join(".git").exists()),
        "state must be outside Git checkouts"
    );
    Ok(())
}
