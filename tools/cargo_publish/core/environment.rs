//! Process isolation for pinned Cargo validation.

use std::{
    collections::BTreeMap,
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use crate::ContractError;

/// Private Cargo/compiler paths and the complete allowed child environment.
pub struct CargoEnvironment {
    pub environment: BTreeMap<String, String>,
    pub cargo_home: PathBuf,
    pub rustc_wrapper: PathBuf,
}

fn io<T>(path: &Path, result: std::io::Result<T>) -> Result<T, ContractError> {
    result.map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })
}
fn text(path: &Path) -> Result<String, ContractError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        ContractError::Policy(format!("non-UTF-8 tool/evidence path {}", path.display()))
    })
}
fn shell_quote(path: &Path) -> Result<String, ContractError> {
    Ok(format!("'{}'", text(path)?.replace('\'', "'\"'\"'")))
}

/// Create private homes/tmp and a wrapper that pins rustc and its sysroot.
/// Callers own registry caches, Cargo configuration, evidence and target paths.
///
/// # Errors
/// Returns an error for non-UTF-8 paths, preexisting private directories, or I/O failures.
pub fn prepare_cargo_environment(
    output: &Path,
    rustc: &Path,
    sysroot: &Path,
    target_directory: &Path,
) -> Result<CargoEnvironment, ContractError> {
    let mut environment = BTreeMap::new();
    for key in [
        "PATH",
        "NIX_LD",
        "NIX_LD_LIBRARY_PATH",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NIX_SSL_CERT_FILE",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    for name in ["home", "cargo-home", "tmp"] {
        let path = output.join(name);
        io(&path, DirBuilder::new().mode(0o700).create(&path))?;
    }
    let cargo_home = output.join("cargo-home");
    let rustc_wrapper = output.join("rustc-pinned.sh");
    let bytes = format!(
        "#!/usr/bin/env bash\nexec {} --sysroot {} \"$@\"\n",
        shell_quote(rustc)?,
        shell_quote(sysroot)?
    );
    let mut file = io(
        &rustc_wrapper,
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&rustc_wrapper),
    )?;
    io(&rustc_wrapper, file.write_all(bytes.as_bytes()))?;
    io(
        &rustc_wrapper,
        fs::set_permissions(&rustc_wrapper, fs::Permissions::from_mode(0o700)),
    )?;
    for (key, path) in [
        ("HOME", output.join("home")),
        ("TMPDIR", output.join("tmp")),
        ("CARGO_HOME", cargo_home.clone()),
        ("RUSTC", rustc_wrapper.clone()),
        ("CARGO_TARGET_DIR", target_directory.to_owned()),
    ] {
        environment.insert(key.into(), text(&path)?);
    }
    for (key, value) in [
        ("CARGO_BUILD_JOBS", "2"),
        ("LANG", "C.UTF-8"),
        ("LC_ALL", "C.UTF-8"),
    ] {
        environment.insert(key.into(), value.into());
    }
    Ok(CargoEnvironment {
        environment,
        cargo_home,
        rustc_wrapper,
    })
}
