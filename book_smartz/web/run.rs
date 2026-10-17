//! Run the same relocatable package used by release acceptance.
use std::{fs, process::Command};

use anyhow::Context;
use runfiles::{Runfiles, rlocation};
fn main() -> anyhow::Result<std::process::ExitCode> {
    let files = Runfiles::create()?;
    let archive = rlocation!(files, std::env::var("PACKAGE_ARCHIVE")?)
        .context("package archive unavailable")?;
    let package = tempfile::tempdir()?;
    tar::Archive::new(fs::File::open(archive)?).unpack(package.path())?;
    let status = Command::new(package.path().join("book-smartz"))
        .args(std::env::args_os().skip(1))
        .status()?;
    Ok(std::process::ExitCode::from(
        u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
    ))
}
