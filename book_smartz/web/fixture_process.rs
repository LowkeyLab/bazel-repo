//! Bounded error observations for fixture-owned executable starts.
use std::{
    fs::{self, File, Metadata},
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
    process::{Child, Command, ExitStatus},
};

use serde_json::{Value, json};

const MAX_PROCESS_ENTRIES: usize = 256;
const MAX_FD_ENTRIES: usize = 4_096;
const MAX_WRITERS: usize = 16;
const MAX_FDINFO_BYTES: u64 = 4_096;

pub fn spawn(command: &mut Command, stage: &'static str) -> anyhow::Result<Child> {
    observe(command, stage, "spawn", Command::spawn)
}
pub fn status(command: &mut Command, stage: &'static str) -> anyhow::Result<ExitStatus> {
    observe(command, stage, "status", Command::status)
}
fn observe<T>(
    command: &mut Command,
    stage: &'static str,
    operation: &'static str,
    run: fn(&mut Command) -> std::io::Result<T>,
) -> anyhow::Result<T> {
    run(command).map_err(|error| {
        let program = Path::new(command.get_program());
        let metadata = fs::metadata(program).ok();
        let name = program.file_name().map(|v| v.to_string_lossy().chars().take(80).collect::<String>());
        let executable = json!({"name":name,"device":metadata.as_ref().map(MetadataExt::dev),"inode":metadata.as_ref().map(MetadataExt::ino)});
        let observation = match (error.raw_os_error(), metadata.as_ref()) {
            (Some(26), Some(target)) => writers(target),
            (Some(26), None) => json!({"status":"identity_unavailable","writers":[]}),
            _ => json!({"status":"not_requested","writers":[]}),
        };
        let record = json!({"version":1,"stage":stage,"operation":operation,"os_error":error.raw_os_error(),"kind":format!("{:?}",error.kind()),"current_pid":std::process::id(),"identity_observed_after_error":true,"executable":executable,"observation":observation});
        anyhow::Error::new(error).context(format!("fixture_process_failure {record}"))
    })
}
#[derive(Default)]
struct Scan {
    processes: usize,
    descriptors: usize,
    unreadable: usize,
    limited: bool,
    proc_unavailable: bool,
    writers: Vec<Value>,
}
impl Scan {
    const fn full(&self) -> bool {
        self.descriptors >= MAX_FD_ENTRIES || self.writers.len() >= MAX_WRITERS
    }
    fn process(&mut self, pid: u32, target: &Metadata, proc_root: &Path) {
        self.processes += 1;
        let Ok(entries) = fs::read_dir(proc_root.join(pid.to_string()).join("fd")) else {
            self.unreadable += 1;
            return;
        };
        for entry in entries {
            if self.full() {
                self.limited = true;
                break;
            }
            self.descriptors += 1;
            let Ok(entry) = entry else {
                self.unreadable += 1;
                continue;
            };
            let Ok(fd) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(metadata) = fs::metadata(entry.path()) else {
                self.unreadable += 1;
                continue;
            };
            if metadata.dev() != target.dev() || metadata.ino() != target.ino() {
                continue;
            }
            let Some(access) = access(
                &proc_root
                    .join(pid.to_string())
                    .join("fdinfo")
                    .join(fd.to_string()),
            ) else {
                self.unreadable += 1;
                continue;
            };
            // A reused descriptor must not combine two different file identities.
            let Ok(after) = fs::metadata(entry.path()) else {
                self.unreadable += 1;
                continue;
            };
            if after.dev() != target.dev() || after.ino() != target.ino() {
                self.unreadable += 1;
                continue;
            }
            if access != "read_only" {
                self.writers.push(json!({"pid":pid,"fd":fd,"access":access,"device":metadata.dev(),"inode":metadata.ino()}));
            }
        }
    }
    fn report(self) -> Value {
        json!({"status":if self.writers.is_empty() { if self.proc_unavailable {"proc_unavailable"} else {"not_observed"} } else {"observed"},"complete":!self.limited && !self.proc_unavailable && self.unreadable==0,"processes_scanned":self.processes,"fd_entries_scanned":self.descriptors,"unreadable_entries":self.unreadable,"limited":self.limited,"limits":{"process_entries":MAX_PROCESS_ENTRIES,"fd_entries":MAX_FD_ENTRIES,"writers":MAX_WRITERS,"fdinfo_bytes":MAX_FDINFO_BYTES},"writers":self.writers})
    }
}
fn writers(target: &Metadata) -> Value {
    writers_in(target, Path::new("/proc"))
}
fn writers_in(target: &Metadata, proc_root: &Path) -> Value {
    let mut scan = Scan::default();
    let current = std::process::id();
    // Observe this fixture first. Other visible processes share the remaining budget.
    scan.process(current, target, proc_root);
    if let Ok(entries) = fs::read_dir(proc_root) {
        for (index, entry) in entries.take(MAX_PROCESS_ENTRIES).enumerate() {
            if scan.full() {
                scan.limited = true;
                break;
            }
            if index + 1 == MAX_PROCESS_ENTRIES {
                scan.limited = true;
            }
            let Ok(entry) = entry else {
                scan.unreadable += 1;
                continue;
            };
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>()
                && pid != current
            {
                scan.process(pid, target, proc_root);
            }
        }
    } else {
        scan.proc_unavailable = true;
    }
    scan.report()
}
fn access(path: &Path) -> Option<&'static str> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(MAX_FDINFO_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let flags = text.lines().find_map(|line| line.strip_prefix("flags:"))?;
    match u64::from_str_radix(flags.trim(), 8).ok()? & 0b11 {
        0 => Some("read_only"),
        1 => Some("write_only"),
        2 => Some("read_write"),
        _ => None,
    }
}

#[cfg(test)]
mod observation_tests {
    use std::{fs, os::unix::fs::symlink, process::Command};

    use anyhow::{Context, Result};
    use googletest::{assert_that, matchers::eq};
    use serde_json::Value;

    use super::{MAX_FDINFO_BYTES, MAX_PROCESS_ENTRIES, access, observe, writers_in};

    #[googletest::test]
    fn unavailable_process_files_cannot_claim_a_complete_observation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("executable");
        fs::write(&target, b"target")?;
        let metadata = fs::metadata(&target)?;
        let absent = writers_in(&metadata, &root.path().join("absent"));
        assert_that!(absent["status"].as_str(), eq(Some("proc_unavailable")));
        assert_that!(absent["complete"].as_bool(), eq(Some(false)));
        let empty = writers_in(&metadata, root.path());
        assert_that!(empty["status"].as_str(), eq(Some("not_observed")));
        assert_that!(empty["unreadable_entries"].as_u64(), eq(Some(1)));
        assert_that!(empty["complete"].as_bool(), eq(Some(false)));
        Ok(())
    }

    #[googletest::test]
    fn malformed_and_disappeared_descriptor_inputs_never_become_writers() -> Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("executable");
        fs::write(&target, b"target")?;
        let process = root.path().join(std::process::id().to_string());
        let descriptors = process.join("fd");
        let info = process.join("fdinfo");
        fs::create_dir_all(&descriptors)?;
        fs::create_dir(&info)?;
        // A non-descriptor name, a vanished file, and unavailable flags are distinct inputs.
        symlink(&target, descriptors.join("not-a-number"))?;
        symlink(root.path().join("gone"), descriptors.join("3"))?;
        symlink(&target, descriptors.join("4"))?;
        symlink(&target, descriptors.join("5"))?;
        fs::write(info.join("5"), b"flags: invalid")?;
        let report = writers_in(&fs::metadata(&target)?, root.path());
        assert_that!(
            report["writers"].as_array().context("writers")?.len(),
            eq(0)
        );
        assert_that!(report["unreadable_entries"].as_u64(), eq(Some(3)));
        assert_that!(report["fd_entries_scanned"].as_u64(), eq(Some(4)));
        assert_that!(report["complete"].as_bool(), eq(Some(false)));
        Ok(())
    }

    #[googletest::test]
    fn directory_budget_prevents_a_complete_claim_without_observed_writers() -> Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("executable");
        fs::write(&target, b"target")?;
        // Non-process entries consume the same enumeration budget as live /proc entries.
        for index in 0..MAX_PROCESS_ENTRIES {
            fs::write(root.path().join(format!("entry-{index}")), b"")?;
        }
        let report = writers_in(&fs::metadata(&target)?, root.path());
        assert_that!(report["status"].as_str(), eq(Some("not_observed")));
        assert_that!(report["limited"].as_bool(), eq(Some(true)));
        assert_that!(report["complete"].as_bool(), eq(Some(false)));
        Ok(())
    }

    #[googletest::test]
    fn descriptor_flags_require_valid_octal_access_within_the_read_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        let info = root.path().join("fdinfo");
        for text in ["position: 0", "flags: xyz", "flags: 3"] {
            fs::write(&info, text)?;
            assert_that!(access(&info), eq(None));
        }
        let beyond_budget = format!(
            "{}\nflags: 1",
            "x".repeat(usize::try_from(MAX_FDINFO_BYTES)?)
        );
        fs::write(&info, beyond_budget)?;
        assert_that!(access(&info), eq(None));
        Ok(())
    }

    #[googletest::test]
    fn vanished_executable_identity_preserves_the_original_kernel_error() -> Result<()> {
        let root = tempfile::tempdir()?;
        let target = root.path().join("executable");
        fs::write(&target, b"target")?;
        // This stub supplies a completed start failure after the path disappears.
        fn disappeared(command: &mut Command) -> std::io::Result<()> {
            fs::remove_file(command.get_program())?;
            Err(std::io::Error::from_raw_os_error(26))
        }
        let error = observe(
            &mut Command::new(&target),
            "vanished_identity",
            "spawn",
            disappeared,
        )
        .err()
        .context("start failure")?;
        assert_that!(
            error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            eq(Some(26))
        );
        let message = error.to_string();
        let report: Value = serde_json::from_str(
            message
                .strip_prefix("fixture_process_failure ")
                .context("diagnostic")?,
        )?;
        assert_that!(
            report["observation"]["status"].as_str(),
            eq(Some("identity_unavailable"))
        );
        assert_that!(report["executable"]["inode"].is_null(), eq(true));
        assert_that!(
            message.contains(target.to_str().context("path")?),
            eq(false)
        );
        Ok(())
    }
}
