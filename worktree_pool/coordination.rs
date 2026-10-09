use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    path::Path,
};

use crate::error::PoolError;

/// # Errors
/// Fails when creation/access fails or the directory is not private.
pub fn private_directory(path: &Path) -> Result<(), PoolError> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(PoolError::Permissions);
    }
    Ok(())
}
/// # Errors
/// Fails when the file is missing, inaccessible, non-regular, or not private.
pub fn private_file(path: &Path) -> Result<(), PoolError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(PoolError::Permissions);
    }
    Ok(())
}
/// File descriptors opened by std are close-on-exec. Drop unlocks the stable inode.
pub struct LockGuard(File);
impl LockGuard {
    /// # Errors
    /// Fails when the stable private lock file cannot be opened or locked.
    pub fn acquire(path: &Path, exclusive: bool) -> Result<Self, PoolError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        private_file(path)?;
        if exclusive {
            file.lock()?;
        } else {
            file.lock_shared()?;
        }
        Ok(Self(file))
    }
}
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
