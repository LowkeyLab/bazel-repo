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
    fn process(&mut self, pid: u32, target: &Metadata) {
        self.processes += 1;
        let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
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
            let Some(access) = access(&std::path::PathBuf::from(format!(
                "/proc/{pid}/fdinfo/{fd}"
            ))) else {
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
    let mut scan = Scan::default();
    let current = std::process::id();
    // Observe this fixture first. Other visible processes share the remaining budget.
    scan.process(current, target);
    if let Ok(entries) = fs::read_dir("/proc") {
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
                scan.process(pid, target);
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
