use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::PoolError;

/// Unix filenames are bytes; display is informational and never used to reopen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodedPath {
    pub encoding: String,
    pub bytes: Vec<u8>,
    pub display: String,
}
impl EncodedPath {
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        Self {
            encoding: "unix-bytes".into(),
            bytes: path.as_os_str().as_bytes().to_vec(),
            display: path.display().to_string(),
        }
    }
    /// # Errors
    /// Rejects unsupported encodings, NUL bytes, and non-absolute persisted paths.
    pub fn to_path(&self) -> Result<PathBuf, PoolError> {
        if self.encoding != "unix-bytes" || self.bytes.contains(&0) {
            return Err(PoolError::Corrupt);
        }
        let path = PathBuf::from(OsString::from_vec(self.bytes.clone()));
        if !path.is_absolute() {
            return Err(PoolError::Corrupt);
        }
        Ok(path)
    }
}
#[derive(Clone, Debug)]
pub struct Paths {
    pub catalog: PathBuf,
    pub state: PathBuf,
    pub json: bool,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    catalog_dir: Option<String>,
    json: Option<bool>,
}
fn xdg(name: &str, fallback: &str) -> Result<PathBuf, PoolError> {
    let value = match env::var_os(name) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(
            env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .ok_or(PoolError::Configuration)?,
        )
        .join(fallback),
    };
    if !value.is_absolute() {
        return Err(PoolError::Configuration);
    }
    Ok(value)
}
fn absolute(path: PathBuf) -> Result<PathBuf, PoolError> {
    if path.as_os_str().is_empty() {
        return Err(PoolError::Configuration);
    }
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    if path
        .components()
        .any(|v| matches!(v, std::path::Component::ParentDir))
    {
        return Err(PoolError::Configuration);
    }
    Ok(path)
}
fn load_settings(config_path: Option<PathBuf>) -> Result<Settings, PoolError> {
    let explicit_config = config_path.is_some();
    let file =
        config_path.unwrap_or(xdg("XDG_CONFIG_HOME", ".config")?.join("worktree-pool/config.toml"));
    let settings: Settings = match std::fs::read_to_string(file) {
        Ok(text) => config::Config::builder()
            .add_source(config::File::from_str(&text, config::FileFormat::Toml))
            .build()
            .and_then(config::Config::try_deserialize)
            .map_err(|_| PoolError::Configuration)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !explicit_config => {
            Settings::default()
        }
        Err(_) => return Err(PoolError::Configuration),
    };
    Ok(settings)
}
fn json_preference(settings: &Settings, json: bool) -> Result<bool, PoolError> {
    let selected = if json {
        true
    } else if let Some(value) = env::var_os("WORKTREE_POOL_JSON") {
        match value.to_str() {
            Some("true" | "1") => true,
            Some("false" | "0") => false,
            _ => return Err(PoolError::Configuration),
        }
    } else {
        settings.json.unwrap_or(false)
    };
    Ok(selected)
}
impl Paths {
    /// Resolves output preference without requiring command or catalog arguments.
    /// # Errors
    /// Rejects unavailable/invalid configuration and invalid environment output settings.
    pub fn output_preference(config_path: Option<PathBuf>, json: bool) -> Result<bool, PoolError> {
        json_preference(&load_settings(config_path)?, json)
    }

    /// # Errors
    /// Rejects invalid configuration, unavailable home/XDG roots, and unreadable explicit configuration.
    pub fn load(
        catalog_dir: Option<PathBuf>,
        config_path: Option<PathBuf>,
        json: bool,
    ) -> Result<Self, PoolError> {
        let settings = load_settings(config_path)?;
        let selected = catalog_dir
            .or_else(|| env::var_os("WORKTREE_POOL_CATALOG_DIR").map(PathBuf::from))
            .or_else(|| settings.catalog_dir.as_ref().map(PathBuf::from));
        let dir = match selected {
            Some(path) => absolute(path)?,
            None => xdg("XDG_DATA_HOME", ".local/share")?.join("worktree-pool"),
        };
        let json = json_preference(&settings, json)?;
        Ok(Self {
            catalog: dir.join("catalog.redb"),
            state: xdg("XDG_STATE_HOME", ".local/state")?.join("worktree-pool"),
            json,
        })
    }
}
