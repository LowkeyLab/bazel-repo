//! Private durable files and one process lock for each batch.
use crate::plan::{Batch, BatchId};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
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
        ensure!(
            fs::symlink_metadata(root)?.is_dir(),
            "state path is not a directory"
        );
        ensure!(
            fs::metadata(root)?.permissions().mode().trailing_zeros() >= 6,
            "state directory is not private"
        );
        let directory = root.join(batch.as_str());
        if create {
            private_directory(&directory)?;
        }
        ensure!(
            fs::symlink_metadata(&directory)?.is_dir(),
            "batch path is not a directory"
        );
        ensure!(
            fs::metadata(&directory)?
                .permissions()
                .mode()
                .trailing_zeros()
                >= 6,
            "batch directory is not private"
        );
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
fn private_directory(path: &Path) -> Result<()> {
    if path.exists() {
        ensure!(
            fs::symlink_metadata(path)?.is_dir(),
            "state path is not a directory"
        );
        ensure!(
            fs::metadata(path)?.permissions().mode().trailing_zeros() >= 6,
            "state directory is not private"
        );
        return Ok(());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        private_directory(parent)?;
    }
    fs::DirBuilder::new().mode(0o700).create(path)?;
    File::open(parent)?.sync_all()?;
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
