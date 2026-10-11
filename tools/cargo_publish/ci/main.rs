use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    os::unix::{fs::DirBuilderExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Parser)]
#[command(about = "Run real publishing and installed acceptance, retaining selected CI evidence")]
struct Args {
    #[arg(long, default_value = "bazel")]
    bazel: String,
    #[arg(long)]
    workspace: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Serialize)]
struct Stage {
    name: String,
    argv: Vec<String>,
    returncode: Option<i32>,
}

#[derive(Serialize)]
struct Summary {
    status: &'static str,
    stages: Vec<Stage>,
    collection_errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    returncode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    consumer_cargo_files_absent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_stage: Option<String>,
}

#[derive(Deserialize)]
struct Handoff {
    status: String,
    run_id: String,
    archive: PathBuf,
    report: PathBuf,
}

struct Runner {
    args: Args,
    output: PathBuf,
    summary: Summary,
}

fn redact(text: &str) -> String {
    let header = Regex::new(r"--remote_header(?:=|\s+)\S+").expect("constant header pattern");
    let url = Regex::new(r#"(https?://)[^/\s\"]+@"#).expect("constant URL pattern");
    url.replace_all(
        &header.replace_all(text, "--remote_header=<REDACTED>"),
        "$1<REDACTED>@",
    )
    .into_owned()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut contents = serde_json::to_vec_pretty(value)?;
    contents.push(b'\n');
    fs::write(path, contents).with_context(|| format!("write {}", path.display()))
}

fn entries(root: &Path) -> Result<BTreeSet<PathBuf>> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    Ok(fs::read_dir(root)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?)
}

fn package_directory(run: &Path) -> Result<PathBuf> {
    let target = run.join("target");
    let package = target.join("package");
    anyhow::ensure!(
        fs::symlink_metadata(&target)?.is_dir()
            && fs::symlink_metadata(&package)?.is_dir()
            && package.canonicalize()?.starts_with(run.canonicalize()?),
        "publishing package directory must belong to this invocation's run"
    );
    Ok(package)
}

fn stream(source: impl std::io::Read, destination: PathBuf) -> Result<()> {
    let mut output = File::create(destination)?;
    for line in BufReader::new(source).split(b'\n') {
        let line = line?;
        writeln!(output, "{}", redact(&String::from_utf8_lossy(&line)))?;
    }
    Ok(())
}

fn copy_evidence(source: &Path, destination: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(source) else {
        return Ok(());
    };
    if !metadata.is_file() {
        return Ok(());
    }
    fs::create_dir_all(
        destination
            .parent()
            .context("evidence destination parent")?,
    )?;
    let bytes = fs::read(source)?;
    if source.extension().is_some_and(|suffix| suffix == "crate") {
        fs::write(destination, bytes)?;
    } else {
        fs::write(destination, redact(&String::from_utf8_lossy(&bytes)))?;
    }
    Ok(())
}

impl Runner {
    fn new(args: Args) -> Result<Self> {
        let output = if args.output.is_absolute() {
            args.output.clone()
        } else {
            std::env::current_dir()?.join(&args.output)
        };
        fs::DirBuilder::new().mode(0o700).create(&output)?;
        let runner = Self {
            args,
            output,
            summary: Summary {
                status: "incomplete",
                stages: vec![],
                collection_errors: vec![],
                returncode: None,
                consumer_cargo_files_absent: None,
                failed_stage: None,
            },
        };
        runner.save()?;
        Ok(runner)
    }
    fn save(&self) -> Result<()> {
        write_json(&self.output.join("summary.json"), &self.summary)
    }
    fn command(&mut self, name: &str, argv: Vec<String>) -> Result<i32> {
        let stage = self.output.join(name);
        fs::create_dir(&stage)?;
        self.summary.stages.push(Stage {
            name: name.into(),
            argv: argv.clone(),
            returncode: None,
        });
        self.save()?;
        let code = match Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&self.args.workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(mut process) => {
                let stdout = process.stdout.take().context("command stdout")?;
                let stderr = process.stderr.take().context("command stderr")?;
                let stdout_path = stage.join("stdout.log");
                let stderr_path = stage.join("stderr.log");
                let stdout_thread = thread::spawn(move || stream(stdout, stdout_path));
                let stderr_thread = thread::spawn(move || stream(stderr, stderr_path));
                let status = process.wait()?;
                stdout_thread
                    .join()
                    .map_err(|_| anyhow::anyhow!("stdout collector panicked"))??;
                stderr_thread
                    .join()
                    .map_err(|_| anyhow::anyhow!("stderr collector panicked"))??;
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
            }
            Err(error) => {
                fs::write(stage.join("stderr.log"), redact(&error.to_string()))?;
                127
            }
        };
        self.summary
            .stages
            .last_mut()
            .context("current stage")?
            .returncode = Some(code);
        write_json(
            &stage.join("status.json"),
            self.summary.stages.last().context("current stage")?,
        )?;
        self.save()?;
        Ok(code)
    }
    fn collect_generic(&self, root: &Path, before: &BTreeSet<PathBuf>, name: &str) -> Result<()> {
        for run in entries(root)?.difference(before) {
            let metadata = fs::symlink_metadata(run)?;
            if !metadata.is_dir() || run.file_name().is_some_and(|n| n == "registry-cache") {
                continue;
            }
            let destination = self
                .output
                .join(name)
                .join("runs")
                .join(run.file_name().context("run name")?);
            for source in [run.join("report.json"), run.join("metadata.json")] {
                copy_evidence(&source, &destination.join(source.strip_prefix(run)?))?;
            }
            for source in entries(run)? {
                let filename = source.file_name().context("log name")?.to_string_lossy();
                if filename.starts_with(|c: char| c.is_ascii_digit())
                    && filename.contains('-')
                    && filename.ends_with(".log")
                {
                    copy_evidence(&source, &destination.join(source.strip_prefix(run)?))?;
                }
            }
            if fs::symlink_metadata(run.join("target/package"))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                continue;
            }
            for source in entries(&package_directory(run)?)? {
                if source.extension().is_some_and(|suffix| suffix == "crate") {
                    copy_evidence(&source, &destination.join(source.strip_prefix(run)?))?;
                }
            }
        }
        Ok(())
    }
    fn finish(&mut self, name: &str, code: i32) -> Result<i32> {
        self.summary.status = if code == 0 { "success" } else { "failure" };
        self.summary.returncode = Some(code);
        self.summary.failed_stage = (code != 0).then(|| name.into());
        self.save()?;
        println!(
            "{}",
            serde_json::json!({"status": self.summary.status, "evidence": self.output})
        );
        Ok(code)
    }
    fn handoff(&self, name: &str, root: &Path, before: &BTreeSet<PathBuf>) -> Result<Handoff> {
        let handoff: Handoff =
            serde_json::from_slice(&fs::read(self.output.join(name).join("stdout.log"))?)?;
        anyhow::ensure!(
            handoff.status == "success",
            "publishing command did not report success"
        );
        anyhow::ensure!(
            !handoff.run_id.is_empty()
                && Path::new(&handoff.run_id).components().count() == 1
                && !handoff.run_id.contains(['/', '\\'])
                && handoff.run_id != "."
                && handoff.run_id != "..",
            "publishing handoff must identify this invocation's new run"
        );
        let run = root.join(&handoff.run_id);
        anyhow::ensure!(
            entries(root)?.difference(before).any(|path| path == &run)
                && fs::symlink_metadata(&run)?.is_dir(),
            "publishing handoff must identify this invocation's new run"
        );
        anyhow::ensure!(
            handoff.archive.is_absolute()
                && handoff.report.is_absolute()
                && fs::symlink_metadata(&handoff.archive)?.is_file()
                && fs::symlink_metadata(&handoff.report)?.is_file()
                && handoff.report == run.join("report.json")
                && handoff
                    .archive
                    .canonicalize()?
                    .starts_with(package_directory(&run)?.canonicalize()?)
                && handoff
                    .archive
                    .extension()
                    .is_some_and(|suffix| suffix == "crate"),
            "publishing handoff paths must belong to this invocation's run"
        );
        Ok(handoff)
    }
    fn collect_installed(&self, root: &Path) -> Result<()> {
        for name in [
            "artifact.json",
            "validation-report.json",
            "runtime.json",
            "toolchain.txt",
            "commands.log",
            "native-closure.log",
            "native-build-tools.log",
            "installed-version.log",
            "installed-cli.log",
        ] {
            copy_evidence(&root.join(name), &self.output.join("installed").join(name))?;
        }
        copy_evidence(
            &root.join("verified-handoff/verified.crate"),
            &self.output.join("installed/verified.crate"),
        )
    }
    fn run(&mut self) -> Result<i32> {
        let present: Vec<_> = ["Cargo.toml", "Cargo.lock"]
            .into_iter()
            .map(|name| self.args.workspace.join("worktree_pool").join(name))
            .filter(|path| fs::symlink_metadata(path).is_ok())
            .collect();
        if !present.is_empty() {
            fs::write(
                self.output.join("consumer-inputs.log"),
                format!(
                    "Consumer Cargo inputs must remain absent: {}\n",
                    present
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )?;
            return self.finish("consumer-inputs", 1);
        }
        self.summary.consumer_cargo_files_absent = Some(true);
        let code = self.command(
            "bazel-info",
            vec![self.args.bazel.clone(), "info".into(), "bazel-bin".into()],
        )?;
        if code != 0 {
            return self.finish("bazel-info", code);
        }
        let binary_root =
            PathBuf::from(fs::read_to_string(self.output.join("bazel-info/stdout.log"))?.trim())
                .canonicalize()?;
        let mut consumer = None;
        for (name, label, path) in [
            (
                "library",
                "//tools/cargo_publish/fixtures:publish",
                "tools/cargo_publish/fixtures/publish.runs",
            ),
            (
                "binary",
                "//tools/cargo_publish/fixtures:binary_publish",
                "tools/cargo_publish/fixtures/binary_publish.runs",
            ),
            (
                "combined",
                "//tools/cargo_publish/fixtures:combined_publish",
                "tools/cargo_publish/fixtures/combined_publish.runs",
            ),
            (
                "consumer",
                "//worktree_pool:publish",
                "worktree_pool/publish.runs",
            ),
        ] {
            let root = binary_root.join(path);
            let before = entries(&root)?;
            let mut code = self.command(
                name,
                vec![
                    self.args.bazel.clone(),
                    "run".into(),
                    label.into(),
                    "--remote_download_outputs=all".into(),
                ],
            )?;
            if let Err(error) = self.collect_generic(&root, &before, name) {
                self.summary
                    .collection_errors
                    .push(redact(&error.to_string()));
                if code == 0 {
                    code = 1;
                }
            }
            if code != 0 {
                return self.finish(name, code);
            }
            match self.handoff(name, &root, &before) {
                Ok(handoff) => {
                    if name == "consumer" {
                        consumer = Some(handoff);
                    }
                }
                Err(error) => {
                    fs::write(
                        self.output.join(name).join("handoff-error.log"),
                        redact(&error.to_string()),
                    )?;
                    return self.finish(name, 1);
                }
            }
        }
        let handoff = consumer.context("consumer publishing handoff")?;
        let installation = binary_root
            .join("worktree_pool/install/ci-acceptance-runs")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(installation.parent().context("installation parent")?)?;
        let mut code = self.command(
            "installed",
            vec![
                self.args.bazel.clone(),
                "run".into(),
                "//worktree_pool/install:acceptance".into(),
                "--remote_download_outputs=all".into(),
                "--".into(),
                "--archive".into(),
                handoff.archive.to_string_lossy().into_owned(),
                "--report".into(),
                handoff.report.to_string_lossy().into_owned(),
                "--output".into(),
                installation.to_string_lossy().into_owned(),
            ],
        )?;
        if let Err(error) = self.collect_installed(&installation) {
            self.summary
                .collection_errors
                .push(redact(&error.to_string()));
            if code == 0 {
                code = 1;
            }
        }
        self.finish("installed", code)
    }
}

fn main() {
    match Runner::new(Args::parse()).and_then(|mut runner| runner.run()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("publishing CI failed: {error:#}");
            std::process::exit(1);
        }
    }
}
