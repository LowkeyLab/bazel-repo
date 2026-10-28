//! Preserve invocation evidence before interpreting a pool command result.
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub struct Receipt {
    directory: PathBuf,
    pub started: bool,
    pub pid: Option<u32>,
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub complete: bool,
}
pub fn argument(value: &OsStr) -> Value {
    json!({"bytes":value.as_bytes(),"display_informational":value.to_string_lossy()})
}
impl Receipt {
    pub fn new(root: &Path) -> Result<Self> {
        let label = root.to_str().context("receipt root must be UTF-8")?;
        ensure!(
            root.components().all(|part| matches!(
                part,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )),
            "receipt root must not contain parent traversal"
        );
        ensure!(
            root.is_absolute() && label.len() <= 2048 && !label.chars().any(char::is_control),
            "receipt root must be absolute and at most 2048 bytes without control characters"
        );
        let mut ancestor = root;
        while !ancestor.exists() {
            ancestor = ancestor.parent().context("receipt ancestor missing")?;
        }
        let probe = Command::new("git")
            .arg("-C")
            .arg(ancestor)
            .args(["rev-parse", "--absolute-git-dir"])
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_CEILING_DIRECTORIES")
            .env("GIT_DISCOVERY_ACROSS_FILESYSTEM", "1")
            .env("LC_ALL", "C")
            .output()
            .context("cannot inspect receipt location")?;
        ensure!(
            !probe.status.success(),
            "receipts must be outside Git repositories"
        );
        ensure!(
            probe.status.code() == Some(128)
                && String::from_utf8_lossy(&probe.stderr).contains("not a git repository"),
            "receipt location inspection failed"
        );
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        // The low six mode bits grant group or other access.
        ensure!(
            fs::metadata(root)?.permissions().mode().trailing_zeros() >= 6,
            "receipt root must be private"
        );
        let directory = root.join(uuid::Uuid::new_v4().to_string());
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        for parent in root.ancestors() {
            File::open(parent)?.sync_all()?;
            if parent == ancestor {
                break;
            }
        }
        Ok(Self {
            directory,
            started: false,
            pid: None,
            code: None,
            signal: None,
            complete: false,
        })
    }
    pub fn path(&self) -> &Path {
        &self.directory
    }
    fn file(&self, name: &str) -> Result<File> {
        Ok(fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.directory.join(name))?)
    }
    pub fn save(&self, name: &str, value: &Value) -> Result<()> {
        let mut file = self.file(name)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        File::open(&self.directory)?.sync_all()?;
        Ok(())
    }
    pub fn run(&mut self, args: &[OsString], task: &str, selected: Option<&str>) -> Result<()> {
        let recorded: Vec<_> = std::iter::once(OsStr::new("--json"))
            .chain(args.iter().map(OsString::as_os_str))
            .map(argument)
            .collect();
        self.save("invocation.json",&json!({"version":1,"program":"worktree-pool","arguments":recorded,"task":task,"selected_assignment":selected}))?;
        ensure!(!task.is_empty(), "task identity is empty");
        if let Some(handle) = selected {
            uuid::Uuid::parse_str(handle).context("invalid explicit assignment handle")?;
        }
        let out = self.file("stdout")?;
        let err = self.file("stderr")?;
        let child = Command::new("worktree-pool")
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .stdout(out.try_clone()?)
            .stderr(err.try_clone()?)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                self.save(
                    "exit-status.json",
                    &json!({"code":null,"signal":null,"started":false}),
                )?;
                return Err(error).context("cannot start worktree-pool");
            }
        };
        self.started = true;
        self.pid = Some(child.id());
        let status = child.wait().context("pool wait outcome is uncertain")?;
        self.code = status.code();
        self.signal = status.signal();
        out.sync_all()?;
        err.sync_all()?;
        self.save(
            "exit-status.json",
            &json!({"code":self.code,"signal":self.signal,"pid":self.pid,"started":true}),
        )?;
        self.complete = true;
        Ok(())
    }
}
