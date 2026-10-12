use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, DirBuilder, OpenOptions},
    io::Write,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, symlink},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use cargo_publish_core::{
    ArchiveEvidence, Check, CheckRecord, CheckStatus, Command as CommandRecord, Configuration,
    DependencyEvidence, Diagnostic, FileEvidence, GenerationContract, ManifestEvidence,
    NetworkMode, PackagePath, REPORT_KIND, ReportStatus, SCHEMA_VERSION, SelectedTarget, TARGET,
    Toolchain, ValidationReport, digest_bytes, digest_file, inspect_archive,
    prepare_cargo_environment, read_json, validate_report,
};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    Args,
    manifest::compare_manifest,
    resolution::{compare_packaged_lockfile, compare_resolution},
};

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

pub fn private_directory(path: &Path) -> Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .with_context(|| format!("create private directory {}", path.display()))?;
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("write {}", path.display()))?;
    file.write_all(bytes)?;
    Ok(())
}

fn file_evidence(path: &Path) -> Result<FileEvidence> {
    Ok(FileEvidence {
        path: path.to_path_buf(),
        sha256: digest_file(path)?,
    })
}

pub struct ValidationRun {
    args: Args,
    output: PathBuf,
    pub path: PathBuf,
    phase: Check,
    env: BTreeMap<String, String>,
    report: ValidationReport,
}

impl ValidationRun {
    pub fn new(args: Args) -> Result<Self> {
        let run_id = Uuid::new_v4().to_string();
        private_directory(&args.output_root)?;
        let output = args.output_root.canonicalize()?.join(&run_id);
        DirBuilder::new().mode(0o700).create(&output)?;
        let run = Self {
            path: output.join("report.json"),
            output,
            args,
            phase: Check::InputFidelity,
            env: BTreeMap::new(),
            report: ValidationReport {
                schema_version: SCHEMA_VERSION,
                report_kind: REPORT_KIND.into(),
                run_id,
                status: ReportStatus::Incomplete,
                completed: false,
                started_at: timestamp(),
                completed_at: None,
                checks: Check::ALL
                    .into_iter()
                    .map(|phase| {
                        (
                            phase,
                            CheckRecord {
                                status: CheckStatus::Pending,
                                completed: false,
                                error: None,
                                evidence: BTreeMap::new(),
                            },
                        )
                    })
                    .collect(),
                commands: vec![],
                diagnostics: None,
                package: None,
                selection: None,
                inputs: None,
                configuration: None,
                toolchain: None,
                manifests: None,
                archive: None,
                lockfile: None,
                dependencies: None,
                generation_contract: None,
                provenance: None,
            },
        };
        run.save()?;
        Ok(run)
    }

    pub fn phase_name(&self) -> String {
        serde_json::to_value(self.phase)
            .expect("check serialization")
            .as_str()
            .expect("check name")
            .to_owned()
    }

    fn save(&self) -> Result<()> {
        let pending = self.path.with_extension("pending");
        let mut bytes = serde_json::to_vec_pretty(&self.report)?;
        bytes.push(b'\n');
        write_private(&pending, &bytes)?;
        fs::rename(pending, &self.path)?;
        Ok(())
    }

    pub fn fail(&mut self, message: &str) -> Result<()> {
        self.report.status = ReportStatus::Failure;
        self.report.completed = false;
        self.report.completed_at = Some(timestamp());
        self.report.diagnostics = Some(Diagnostic {
            phase: self.phase,
            message: message.into(),
        });
        self.report.checks.insert(
            self.phase,
            CheckRecord {
                status: CheckStatus::Failed,
                completed: false,
                error: Some(message.into()),
                evidence: BTreeMap::new(),
            },
        );
        self.save()
    }

    fn check(&mut self, evidence: Value) -> Result<()> {
        self.report.checks.insert(
            self.phase,
            CheckRecord {
                status: CheckStatus::Passed,
                completed: true,
                error: None,
                evidence: serde_json::from_value(evidence)?,
            },
        );
        self.save()
    }

    fn command(&mut self, argv: &[String]) -> Result<(Vec<u8>, String)> {
        let id = (self.report.commands.len() + 1).to_string();
        let log = self.output.join(format!("{id}-{}.log", self.phase_name()));
        self.report.commands.push(CommandRecord {
            id: id.clone(),
            phase: self.phase,
            argv: argv.to_vec(),
            cwd: self.output.clone(),
            log: log.clone(),
            started_at: timestamp(),
            completed_at: None,
            completed: false,
            returncode: None,
            error: None,
        });
        self.save()?;
        let result = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(&self.output)
            .env_clear()
            .envs(&self.env)
            .output();
        let index = self.report.commands.len() - 1;
        match result {
            Ok(output) => {
                let code = output
                    .status
                    .code()
                    .or_else(|| output.status.signal().map(|signal| -signal))
                    .context("command status has neither exit code nor signal")?;
                let mut bytes = output.stdout.clone();
                bytes.extend_from_slice(b"\n--- stderr ---\n");
                bytes.extend_from_slice(&output.stderr);
                write_private(&log, &bytes)?;
                self.report.commands[index].completed = true;
                self.report.commands[index].returncode = Some(code);
                self.report.commands[index].completed_at = Some(timestamp());
                self.save()?;
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let message = if stderr.trim().is_empty() {
                    stdout.trim()
                } else {
                    stderr.trim()
                };
                ensure!(
                    code == 0,
                    "command exited {code}: {}: {message}; log: {}",
                    argv.iter()
                        .map(|arg| shell_quote(arg))
                        .collect::<Vec<_>>()
                        .join(" "),
                    log.display()
                );
                Ok((output.stdout, id))
            }
            Err(error) => {
                write_private(&log, format!("{error}\n").as_bytes())?;
                let command = &mut self.report.commands[index];
                command.completed = true;
                command.completed_at = Some(timestamp());
                command.error = Some(error.to_string());
                self.save()?;
                Err(error).with_context(|| format!("launch {}", argv[0]))
            }
        }
    }

    fn cargo(
        &mut self,
        command: &str,
        manifest: &Path,
        extra: &[&str],
    ) -> Result<(Vec<u8>, String)> {
        let mut argv = vec![
            self.args.cargo.to_string_lossy().into_owned(),
            command.into(),
            "--manifest-path".into(),
            manifest.to_string_lossy().into_owned(),
        ];
        argv.extend(extra.iter().map(|argument| (*argument).into()));
        if self.args.offline {
            argv.push("--offline".into());
        }
        self.command(&argv)
    }

    fn configure(&mut self) -> Result<()> {
        for parent in self.output.ancestors() {
            for name in ["config", "config.toml"] {
                let path = parent.join(".cargo").join(name);
                ensure!(
                    !path.exists(),
                    "ambient Cargo configuration affects validation: {}; choose an isolated Bazel output root",
                    path.display()
                );
            }
        }
        let isolated = prepare_cargo_environment(
            &self.output,
            &self.args.rustc,
            &self.args.sysroot,
            &self.output.join("target"),
        )?;
        self.env = isolated.environment;
        let home = isolated.cargo_home;
        let wrapper = isolated.rustc_wrapper;
        let cache = self
            .args
            .registry_cache
            .clone()
            .unwrap_or_else(|| self.args.output_root.join("registry-cache"));
        private_directory(&cache)?;
        let cache = cache.canonicalize()?;
        private_directory(&cache.join("registry"))?;
        symlink(cache.join("registry"), home.join("registry"))?;
        for name in [".package-cache", ".package-cache-mutate", ".global-cache"] {
            // Cargo shares these lock/SQLite files: create missing files without truncating existing data.
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(cache.join(name))?;
            symlink(cache.join(name), home.join(name))?;
        }
        let config = home.join("config.toml");
        write_private(
            &config,
            format!(
                "[registries.crates-io]\nprotocol = \"sparse\"\n[net]\noffline = {}\n",
                self.args.offline
            )
            .as_bytes(),
        )?;
        self.report.configuration = Some(Configuration {
            target: TARGET.into(),
            network_mode: if self.args.offline {
                NetworkMode::Offline
            } else {
                NetworkMode::Online
            },
            cargo_home: home,
            target_directory: self.output.join("target"),
            registry_cache: cache,
            environment: report_environment(&self.env),
            config_path: config.clone(),
            config_sha256: digest_file(&config)?,
            rustc_wrapper: wrapper.clone(),
            rustc_wrapper_sha256: digest_file(&wrapper)?,
            cwd: self.output.clone(),
        });
        Ok(())
    }

    fn stage_declared_inputs(&mut self, contract: &GenerationContract) -> Result<PathBuf> {
        self.report.package = Some(contract.package.clone());
        self.report.selection = Some(contract.selection.clone());
        self.report.inputs = Some(contract.inputs.clone());
        self.report.provenance.clone_from(&contract.provenance);
        self.report.generation_contract = Some(file_evidence(&self.args.contract)?);
        let staged = self.output.join("staged");
        private_directory(&staged)?;
        let mut paths = BTreeSet::new();
        for input in &contract.inputs {
            let name = input.path.as_str();
            PackagePath::declared(name)?;
            ensure!(
                paths.insert(name),
                "conflicting declared input path: {name}"
            );
            let source = self.args.package.join(name);
            let metadata = fs::symlink_metadata(&source)
                .with_context(|| format!("declared input fidelity mismatch: {name}"))?;
            let bytes =
                fs::read(&source).with_context(|| format!("read declared input: {name}"))?;
            ensure!(
                metadata.is_file()
                    && !metadata.is_symlink()
                    && digest_bytes(&bytes) == input.sha256
                    && bytes.len() as u64 == input.size,
                "declared input fidelity mismatch: {name}"
            );
            let destination = staged.join(name);
            private_directory(destination.parent().context("declared path parent")?)?;
            write_private(&destination, &bytes)?;
        }
        let generated = self.args.package.join("Cargo.toml");
        let manifest_bytes = fs::read(&generated)?;
        ensure!(
            digest_bytes(&manifest_bytes) == contract.manifest_sha256,
            "generated Cargo.toml fingerprint mismatch"
        );
        write_private(&staged.join("Cargo.toml"), &manifest_bytes)?;
        self.check(
            json!({"inputs": contract.inputs, "manifest_sha256": contract.manifest_sha256}),
        )?;
        Ok(staged)
    }

    fn validate_toolchain(&mut self) -> Result<()> {
        self.phase = Check::Toolchain;
        self.env = allowed_environment(&["PATH", "NIX_LD", "NIX_LD_LIBRARY_PATH"]);
        let (rust_bytes, rust_id) =
            self.command(&[self.args.rustc.to_string_lossy().into_owned(), "-Vv".into()])?;
        let (cargo_bytes, cargo_id) =
            self.command(&[self.args.cargo.to_string_lossy().into_owned(), "-V".into()])?;
        let rust = String::from_utf8(rust_bytes)?;
        let cargo = String::from_utf8(cargo_bytes)?;
        ensure!(
            rust.contains("release: 1.96.0\n")
                && rust.contains(&format!("host: {TARGET}\n"))
                && cargo.starts_with("cargo 1.96.0 "),
            "unsupported pinned toolchain: {rust} {cargo}"
        );
        ensure!(
            std::env::consts::OS == "linux" && std::env::consts::ARCH == "x86_64",
            "unsupported runtime host: {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        self.report.toolchain = Some(Toolchain {
            rustc: rust,
            cargo,
            host: TARGET.into(),
            target: TARGET.into(),
            rustc_path: self.args.rustc.clone(),
            cargo_path: self.args.cargo.clone(),
            sysroot: self.args.sysroot.clone(),
        });
        self.check(json!({"command_ids": [rust_id, cargo_id], "versions": self.report.toolchain}))?;
        Ok(())
    }

    fn validate_generated_manifest(
        &mut self,
        contract: &GenerationContract,
        staged: &Path,
    ) -> Result<Vec<String>> {
        self.phase = Check::GeneratedManifest;
        contract.validate()?;
        let targets: Vec<String> = contract
            .selection
            .selected_targets()?
            .iter()
            .map(ToString::to_string)
            .collect();
        compare_manifest(&staged.join("Cargo.toml"), contract)?;
        self.report.manifests = Some(ManifestEvidence {
            generated: file_evidence(&staged.join("Cargo.toml"))?,
            original: None,
            normalized: None,
        });
        self.check(json!({"package": contract.package, "targets": targets, "dependencies": contract.dependencies}))?;
        Ok(targets)
    }

    fn package_archive(&mut self, contract: &GenerationContract, staged: &Path) -> Result<PathBuf> {
        self.phase = Check::CargoPackage;
        let (_, package_id) = self.cargo(
            "package",
            &staged.join("Cargo.toml"),
            &["--target", TARGET, "--allow-dirty"],
        )?;
        let archives = fs::read_dir(self.output.join("target/package"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "crate"))
            .collect::<Vec<_>>();
        let expected_name = format!(
            "{}-{}.crate",
            contract.package.name, contract.package.version
        );
        ensure!(
            archives.len() == 1
                && archives[0]
                    .file_name()
                    .is_some_and(|name| name == expected_name.as_str()),
            "missing or unexpected packaged archive: expected {expected_name}, found {archives:?}"
        );
        let archive = archives[0].clone();
        self.check(
            json!({"command_ids": [package_id], "archive": archive, "verification_enabled": true}),
        )?;
        Ok(archive)
    }

    fn inspect_packaged_archive(
        &mut self,
        contract: &GenerationContract,
        archive: &Path,
    ) -> Result<PathBuf> {
        self.phase = Check::ArchivePolicy;
        let inspected = inspect_archive(
            archive,
            &contract.package,
            &contract.inputs,
            self.output.join("extracted"),
        )?;
        let extracted = inspected.extracted_root;
        self.report.archive = Some(ArchiveEvidence {
            path: archive.to_path_buf(),
            sha256: digest_file(archive)?,
            size: fs::metadata(archive)?.len(),
            inventory: inspected.inventory.clone(),
            extracted_root: extracted.clone(),
            fidelity: CheckStatus::Passed,
        });
        self.check(json!({"inventory": inspected.inventory, "root": extracted.file_name().context("archive root name")?.to_string_lossy()}))?;
        self.phase = Check::ArchiveFidelity;
        self.check(json!({"inputs": contract.inputs, "verified": true}))?;
        Ok(extracted)
    }

    fn validate_archive_manifests(
        &mut self,
        contract: &GenerationContract,
        extracted: &Path,
    ) -> Result<FileEvidence> {
        self.phase = Check::OriginalManifest;
        let original = file_evidence(&extracted.join("Cargo.toml.orig"))?;
        ensure!(
            original.sha256 == contract.manifest_sha256,
            "Cargo.toml.orig differs from generated manifest"
        );
        self.report
            .manifests
            .as_mut()
            .context("generated manifest evidence")?
            .original = Some(original.clone());
        self.check(json!(original))?;
        self.phase = Check::NormalizedManifest;
        compare_manifest(&extracted.join("Cargo.toml"), contract)?;
        let normalized = file_evidence(&extracted.join("Cargo.toml"))?;
        self.report
            .manifests
            .as_mut()
            .context("generated manifest evidence")?
            .normalized = Some(normalized.clone());
        let lockfile = file_evidence(&extracted.join("Cargo.lock"))?;
        self.report.lockfile = Some(lockfile.clone());
        self.check(json!({"manifest": normalized, "package": contract.package, "dependencies": contract.dependencies}))?;
        Ok(lockfile)
    }

    fn validate_locked_resolution(
        &mut self,
        contract: &GenerationContract,
        extracted: &Path,
        lockfile: &FileEvidence,
    ) -> Result<()> {
        self.phase = Check::LockedMetadata;
        let (metadata_bytes, metadata_id) = self.cargo(
            "metadata",
            &extracted.join("Cargo.toml"),
            &[
                "--locked",
                "--filter-platform",
                TARGET,
                "--format-version",
                "1",
            ],
        )?;
        let metadata_path = self.output.join("metadata.json");
        write_private(&metadata_path, &metadata_bytes)?;
        let metadata_sha256 = digest_file(&metadata_path)?;
        let metadata: Value =
            serde_json::from_slice(&metadata_bytes).context("parse Cargo metadata")?;
        self.check(json!({"command_ids": [metadata_id], "path": metadata_path, "sha256": metadata_sha256, "lockfile_sha256": lockfile.sha256}))?;
        self.phase = Check::DirectDependencies;
        compare_packaged_lockfile(&extracted.join("Cargo.lock"), contract)?;
        let resolved = compare_resolution(
            serde_json::from_value(metadata).context("read Cargo metadata shape")?,
            contract,
            extracted,
        )?;
        self.report.dependencies = Some(DependencyEvidence {
            requested: contract.dependencies.clone(),
            observed: resolved.clone(),
            metadata_path,
            metadata_sha256,
        });
        self.check(json!({"requested": contract.dependencies, "resolved_graph": resolved, "compared": true}))?;
        Ok(())
    }

    fn validate_locked_builds(
        &mut self,
        contract: &GenerationContract,
        extracted: &Path,
        targets: &[String],
        lockfile: &FileEvidence,
    ) -> Result<()> {
        self.phase = Check::LockedBuilds;
        let mut build_ids = vec![];
        for target in contract.selection.selected_targets()? {
            let selector = match &target {
                SelectedTarget::Library(_) => vec!["--lib"],
                SelectedTarget::Binary(name) => vec!["--bin", name.as_str()],
            };
            let mut extra = vec!["--locked", "--target", TARGET];
            extra.extend(selector);
            let (_, id) = self.cargo("build", &extracted.join("Cargo.toml"), &extra)?;
            build_ids.push(id);
        }
        ensure!(
            digest_file(extracted.join("Cargo.lock"))? == lockfile.sha256,
            "packaged Cargo.lock changed during locked validation"
        );
        self.check(json!({"command_ids": build_ids, "targets": targets}))?;
        Ok(())
    }

    pub fn execute(&mut self) -> Result<()> {
        let contract: GenerationContract = read_json(&self.args.contract)?;
        let staged = self.stage_declared_inputs(&contract)?;
        self.validate_toolchain()?;
        self.phase = Check::Configuration;
        self.configure()?;
        self.check(json!(self.report.configuration))?;
        let targets = self.validate_generated_manifest(&contract, &staged)?;
        let archive = self.package_archive(&contract, &staged)?;
        let extracted = self.inspect_packaged_archive(&contract, &archive)?;
        let lockfile = self.validate_archive_manifests(&contract, &extracted)?;
        self.validate_locked_resolution(&contract, &extracted, &lockfile)?;
        self.validate_locked_builds(&contract, &extracted, &targets, &lockfile)?;
        self.report.status = ReportStatus::Success;
        self.report.completed = true;
        self.report.completed_at = Some(timestamp());
        validate_report(&self.report, None)?;
        self.save()?;
        println!(
            "{}",
            json!({"status":"success", "run_id":self.report.run_id, "archive":archive, "report":self.path})
        );
        Ok(())
    }
}

fn allowed_environment(allowed: &[&str]) -> BTreeMap<String, String> {
    std::env::vars()
        .filter(|(key, _)| allowed.contains(&key.as_str()))
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn report_environment(environment: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    environment
        .iter()
        .map(|(key, value)| {
            let is_proxy = matches!(
                key.as_str(),
                "HTTP_PROXY"
                    | "HTTPS_PROXY"
                    | "ALL_PROXY"
                    | "http_proxy"
                    | "https_proxy"
                    | "all_proxy"
            );
            let reported = if is_proxy && value.contains('@') {
                "<redacted>".into()
            } else {
                value.clone()
            };
            (key.clone(), reported)
        })
        .collect()
}
