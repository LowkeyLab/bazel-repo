//! Real locked source installation and installed public CLI acceptance.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use cargo_publish_core::{
    ExpectedPackage, PackageMetadata, Sha256, VerifiedArchive, digest_file, read_json,
    validate_verified_archive, write_json,
};
use clap::Parser;
use installer_native::{GIT_SHA256, ZLIB_SHA256, extract_pinned_source};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(
    about = "Install an explicit verified archive with the pinned tools and exercise the installed CLI"
)]
struct Args {
    cargo: PathBuf,
    rustc: PathBuf,
    git_source: PathBuf,
    zlib_source: PathBuf,
    cli_tests: PathBuf,
    #[arg(long)]
    sysroot: PathBuf,
    #[arg(long)]
    runfiles_root: PathBuf,
    #[arg(long)]
    workspace: String,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    archive: PathBuf,
    #[arg(long)]
    report: PathBuf,
    #[arg(long)]
    expected_version: String,
}
#[derive(Deserialize)]
struct ReportHeader {
    schema_version: u32,
    #[serde(default)]
    report_kind: String,
}
#[derive(Deserialize)]
struct Library {
    name: String,
    path: String,
}
#[derive(Deserialize)]
struct Binary {
    name: String,
    path: String,
}
#[derive(Deserialize)]
struct Manifest {
    package: PackageMetadata,
    lib: Option<Library>,
    #[serde(default, rename = "bin")]
    binaries: Vec<Binary>,
}
#[derive(Serialize)]
struct Artifact<'a> {
    generic_run_id: &'a str,
    archive_sha256: &'a Sha256,
    verified_archive: &'a Path,
    validation_report: &'a Path,
    extracted_package: &'a Path,
}
#[derive(Serialize)]
struct InstallationConfiguration<'a> {
    environment: &'a BTreeMap<String, String>,
    cwd: &'a Path,
    cargo_config: &'a Path,
    cargo_config_sha256: Sha256,
    rustc_wrapper: &'a Path,
    rustc_wrapper_sha256: Sha256,
}
#[derive(Serialize)]
struct Runtime<'a> {
    git: &'a str,
    binary_sha256: Sha256,
    absent_from_path: [&'static str; 3],
    cwd: &'a Path,
    helper_scope: &'static str,
}
fn private_directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .with_context(|| format!("create new private directory {}", path.display()))
}
fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .with_context(|| format!("non-UTF-8 tool/evidence path {}", path.display()))
}
fn shell_quote(path: &Path) -> Result<String> {
    Ok(format!("'{}'", text(path)?.replace('\'', "'\\''")))
}
fn strings(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}
fn run_logged(
    program: impl AsRef<OsStr>,
    arguments: &[OsString],
    cwd: &Path,
    environment: &BTreeMap<String, String>,
    log: &Path,
) -> Result<()> {
    let program = program.as_ref();
    let mut stream = fs::OpenOptions::new().create(true).append(true).open(log)?;
    let argv: Vec<_> = std::iter::once(program)
        .chain(arguments.iter().map(OsString::as_os_str))
        .map(|item| item.to_string_lossy())
        .collect();
    serde_json::to_writer(&mut stream, &argv)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let status = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stream.try_clone()?))
        .stderr(Stdio::from(stream))
        .status()
        .with_context(|| {
            format!(
                "execute {} (log {})",
                program.to_string_lossy(),
                log.display()
            )
        })?;
    ensure!(
        status.success(),
        "{} failed ({status}); retained log {}",
        program.to_string_lossy(),
        log.display()
    );
    Ok(())
}
fn captured(
    program: impl AsRef<OsStr>,
    arguments: &[&str],
    cwd: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    let program = program.as_ref();
    let result = Command::new(program)
        .args(arguments)
        .current_dir(cwd)
        .env_clear()
        .envs(environment)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("execute {}", program.to_string_lossy()))?;
    ensure!(
        result.status.success(),
        "{} failed ({}): {}",
        program.to_string_lossy(),
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).context("tool stdout is not UTF-8")
}
fn certified_artifact(args: &Args, output: &Path) -> Result<VerifiedArchive> {
    let report_path = output.join("validation-report.json");
    fs::copy(&args.report, &report_path)?;
    let header: ReportHeader = read_json(&report_path)?;
    ensure!(
        header.schema_version == 1 && header.report_kind == "cargo_publish_validation",
        "unsupported cargo_publish validation report contract"
    );
    let verified = validate_verified_archive(
        &report_path,
        &args.archive,
        output.join("verified-handoff"),
        Some(&ExpectedPackage {
            name: "worktree-pool".into(),
            version: args.expected_version.clone(),
        }),
    )?;
    let selected = verified
        .report
        .selection
        .as_ref()
        .context("missing verified selection")?
        .selected_targets()?
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    ensure!(
        selected == ["bin:worktree-pool", "lib:worktree_pool"],
        "verified selection must cover worktree_pool library and worktree-pool binary"
    );
    let inputs: BTreeSet<_> = verified
        .report
        .inputs
        .as_ref()
        .context("missing verified inputs")?
        .iter()
        .map(|item| item.path.as_str())
        .collect();
    for required in [
        "LICENSE",
        "README.md",
        "CONTRIBUTING.md",
        "WIRE_SCHEMA.md",
        "DEPENDENCIES.md",
        "lib.rs",
        "main.rs",
    ] {
        ensure!(
            inputs.contains(required),
            "missing worktree-pool package input: {required}"
        );
    }
    let manifest: Manifest = toml::from_str(&fs::read_to_string(
        verified.extracted_root.join("Cargo.toml"),
    )?)?;
    ensure!(
        manifest.package.name == "worktree-pool"
            && manifest.package.version == args.expected_version,
        "packaged worktree-pool identity/version differs"
    );
    let library = manifest.lib.context("missing worktree-pool library")?;
    ensure!(
        library.name == "worktree_pool" && library.path == "lib.rs",
        "worktree-pool library name/path differs"
    );
    ensure!(
        manifest.binaries.len() == 1
            && manifest.binaries[0].name == "worktree-pool"
            && manifest.binaries[0].path == "main.rs",
        "worktree-pool binary name/path differs"
    );
    write_json(
        output.join("artifact.json"),
        &Artifact {
            generic_run_id: &verified.report.run_id,
            archive_sha256: &verified
                .report
                .archive
                .as_ref()
                .context("missing verified archive")?
                .sha256,
            verified_archive: &verified.copied_archive,
            validation_report: &report_path,
            extracted_package: &verified.extracted_root,
        },
    )?;
    Ok(verified)
}
fn isolated_environment(args: &Args, output: &Path) -> Result<BTreeMap<String, String>> {
    for parent in output.ancestors() {
        for name in ["config", "config.toml"] {
            let config = parent.join(".cargo").join(name);
            ensure!(
                !config.exists(),
                "installation_configuration: ambient Cargo configuration: {}; choose an isolated output directory",
                config.display()
            );
        }
    }
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
        if let Ok(value) = env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    for name in ["home", "cargo-home", "tmp"] {
        private_directory(&output.join(name))?;
    }
    let wrapper = output.join("rustc-pinned.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/usr/bin/env bash\nexec {} --sysroot {} \"$@\"\n",
            shell_quote(&args.rustc)?,
            shell_quote(&args.sysroot)?
        ),
    )?;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700))?;
    for (key, path) in [
        ("HOME", output.join("home")),
        ("TMPDIR", output.join("tmp")),
        ("RUSTC", wrapper.clone()),
        ("CARGO_HOME", output.join("cargo-home")),
        ("CARGO_TARGET_DIR", output.join("cargo-target")),
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
    let config = output.join("cargo-home/config.toml");
    fs::write(&config, "[registries.crates-io]\nprotocol = \"sparse\"\n")?;
    write_json(
        output.join("installation-configuration.json"),
        &InstallationConfiguration {
            environment: &environment,
            cwd: output,
            cargo_config: &config,
            cargo_config_sha256: digest_file(&config)?,
            rustc_wrapper: &wrapper,
            rustc_wrapper_sha256: digest_file(&wrapper)?,
        },
    )?;
    Ok(environment)
}
fn locked_install(
    args: &Args,
    output: &Path,
    package: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let rust = captured(&args.rustc, &["-Vv"], output, environment)?;
    let cargo = captured(&args.cargo, &["-V"], output, environment)?;
    ensure!(
        rust.contains("release: 1.96.0\n") && rust.contains("host: x86_64-unknown-linux-gnu\n"),
        "unsupported pinned rustc: {rust}"
    );
    ensure!(
        cargo.starts_with("cargo 1.96.0 "),
        "unsupported pinned Cargo: {cargo}"
    );
    ensure!(
        env::consts::OS == "linux" && env::consts::ARCH == "x86_64",
        "installed acceptance requires Linux x86_64 GNU host"
    );
    fs::write(output.join("toolchain.txt"), format!("{rust}{cargo}"))?;
    let mut arguments = strings(&["install", "--locked", "--path"]);
    arguments.push(package.as_os_str().into());
    arguments.push("--root".into());
    arguments.push(output.join("installed").into_os_string());
    run_logged(
        &args.cargo,
        &arguments,
        output,
        environment,
        &output.join("commands.log"),
    )?;
    let binary = output.join("installed/bin/worktree-pool");
    let mut magic = [0; 4];
    fs::File::open(&binary)?.read_exact(&mut magic)?;
    ensure!(
        &magic == b"\x7fELF",
        "installed worktree-pool must be a native ELF executable"
    );
    for program in ["file", "ldd"] {
        run_logged(
            program,
            &[binary.as_os_str().into()],
            output,
            environment,
            &output.join("native-closure.log"),
        )?;
    }
    Ok(binary)
}
fn native_runtime(
    args: &Args,
    output: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    for program in ["cc", "make"] {
        run_logged(
            program,
            &strings(&["--version"]),
            output,
            environment,
            &output.join("native-build-tools.log"),
        )?;
    }
    let zlib = extract_pinned_source(
        &args.zlib_source,
        &Sha256::new(ZLIB_SHA256)?,
        &output.join("zlib-source"),
    )?;
    let mut configure = strings(&["configure", "--static"]);
    configure.push(format!("--prefix={}", text(&output.join("zlib"))?).into());
    run_logged(
        "sh",
        &configure,
        &zlib,
        environment,
        &output.join("commands.log"),
    )?;
    run_logged(
        "make",
        &strings(&["-j2", "install"]),
        &zlib,
        environment,
        &output.join("commands.log"),
    )?;
    let git = extract_pinned_source(
        &args.git_source,
        &Sha256::new(GIT_SHA256)?,
        &output.join("git-source"),
    )?;
    run_logged(
        "xz",
        &strings(&["--version"]),
        output,
        environment,
        &output.join("native-build-tools.log"),
    )?;
    let mut flags = strings(&["-j2", "CFLAGS=-O2 -std=gnu99"]);
    flags.push(format!("prefix={}", text(&output.join("git"))?).into());
    flags.push(format!("ZLIB_PATH={}", text(&output.join("zlib"))?).into());
    flags.extend(strings(&[
        "NO_GETTEXT=YesPlease",
        "NO_TCLTK=YesPlease",
        "NO_PERL=YesPlease",
        "NO_PYTHON=YesPlease",
        "NO_CURL=YesPlease",
        "NO_EXPAT=YesPlease",
        "NO_OPENSSL=YesPlease",
        "NO_ICONV=YesPlease",
        "install",
    ]));
    run_logged(
        "make",
        &flags,
        &git,
        environment,
        &output.join("commands.log"),
    )
}
fn find_tool(name: &str, path: &str) -> Option<PathBuf> {
    env::split_paths(path)
        .map(|directory| directory.join(name))
        .find(|candidate| {
            candidate.is_file()
                && fs::metadata(candidate)
                    .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
        })
}
fn installed_cli(
    args: &Args,
    output: &Path,
    binary: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    let path = output.join("runtime-bin");
    private_directory(&path)?;
    for name in ["sh", "sleep"] {
        let tool = find_tool(
            name,
            environment
                .get("PATH")
                .context("native build PATH is missing")?,
        )
        .with_context(|| format!("missing native runtime tool {name}"))?;
        symlink(tool, path.join(name))?;
    }
    symlink(output.join("git/bin/git"), path.join("git"))?;
    private_directory(&output.join("runtime-home"))?;
    let runtime = BTreeMap::from([
        ("PATH".into(), text(&path)?),
        ("HOME".into(), text(&output.join("runtime-home"))?),
        (
            "GIT_EXEC_PATH".into(),
            text(&output.join("git/libexec/git-core"))?,
        ),
        ("TEST_SRCDIR".into(), text(&args.runfiles_root)?),
        ("TEST_WORKSPACE".into(), args.workspace.clone()),
        ("POOL_INSTALLED_BINARY".into(), text(binary)?),
    ]);
    for name in ["cargo", "rustc", "bazel"] {
        ensure!(
            find_tool(name, &runtime["PATH"]).is_none(),
            "{name} must be absent from runtime PATH"
        );
    }
    let git = captured(path.join("git"), &["--version"], output, &runtime)?
        .trim()
        .to_owned();
    ensure!(
        git == "git version 2.36.0",
        "minimum Git runtime differs: {git}"
    );
    let unrelated = output.join("unrelated-working-directory");
    private_directory(&unrelated)?;
    let version = captured(binary, &["--version"], &unrelated, &runtime)?
        .trim()
        .to_owned();
    ensure!(
        version == format!("worktree-pool {}", args.expected_version),
        "installed CLI version differs: {version}"
    );
    fs::write(output.join("installed-version.log"), format!("{version}\n"))?;
    run_logged(
        &args.cli_tests,
        &strings(&["--test-threads=1"]),
        &unrelated,
        &runtime,
        &output.join("installed-cli.log"),
    )?;
    write_json(
        output.join("runtime.json"),
        &Runtime {
            git: &git,
            binary_sha256: digest_file(binary)?,
            absent_from_path: ["cargo", "rustc", "bazel"],
            cwd: &unrelated,
            helper_scope: "Bazel test_support children; public CLI commands use Cargo-installed artifact",
        },
    )?;
    Ok(())
}
fn acceptance(mut args: Args) -> Result<()> {
    for path in [
        &mut args.cargo,
        &mut args.rustc,
        &mut args.git_source,
        &mut args.zlib_source,
        &mut args.cli_tests,
        &mut args.sysroot,
        &mut args.runfiles_root,
        &mut args.archive,
        &mut args.report,
    ] {
        *path = absolute(path)?;
    }
    let output = absolute(&args.output)?;
    let parent = output
        .parent()
        .context("output must have a parent")?
        .canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .context("output must name a new directory")?,
    );
    private_directory(&output)?;
    let verified = certified_artifact(&args, &output).with_context(|| {
        format!(
            "artifact_validation: archive {}, report {}",
            args.archive.display(),
            args.report.display()
        )
    })?;
    let environment = isolated_environment(&args, &output)?;
    let binary = locked_install(&args, &output, &verified.extracted_root, &environment)?;
    native_runtime(&args, &output, &environment)?;
    installed_cli(&args, &output, &binary, &environment)?;
    println!(
        "Verified installed public CLI and minimum runtime: {}",
        output.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = acceptance(Args::parse()) {
        eprintln!("acceptance failed: {error:#}");
        std::process::exit(1);
    }
}
