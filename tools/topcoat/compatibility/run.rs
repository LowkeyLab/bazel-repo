//! Launch exactly the archive layout from a temporary working directory.
use std::{fs, process::Command};

use anyhow::{Context, ensure};
use runfiles::{Runfiles, rlocation};
fn main() -> anyhow::Result<()> {
    std::env::var("DATABASE_URL").context("set DATABASE_URL for an isolated fixture")?;
    std::env::var("FIXTURE_READER_ID")
        .context("set FIXTURE_READER_ID for the controlled identity")?;
    let runfiles = Runfiles::create()?;
    let archive =
        rlocation!(runfiles, std::env::var("PACKAGE_ARCHIVE")?).context("package runfile")?;
    let package = tempfile::tempdir()?;
    tar::Archive::new(fs::File::open(archive)?).unpack(package.path())?;
    let cwd = tempfile::tempdir()?;
    let status = Command::new(package.path().join("app"))
        .current_dir(cwd.path())
        .status()?;
    ensure!(status.success(), "packaged application exited {status}");
    Ok(())
}
