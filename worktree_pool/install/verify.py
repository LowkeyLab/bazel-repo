#!/usr/bin/env python3
"""Verify the real standalone archive, locked source install and installed public CLI.

Run through :acceptance so Cargo/rustc come from the native Bazel toolchain. All build
and runtime fixtures are private to the explicitly supplied new output directory.
"""

import argparse
import hashlib
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

from tools.cargo_publish.report import selected_targets, validate_verified_archive


def run(argv, *, cwd, env, log):
    """Retain each real command and its complete output, failing on actual status."""
    with log.open("ab") as stream:
        stream.write((json.dumps([str(arg) for arg in argv]) + "\n").encode())
        stream.flush()
        subprocess.run(argv, cwd=cwd, env=env, stdout=stream, stderr=stream, check=True)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract(archive, destination, *, allow_links=False):
    # Application packages contain no links. Upstream data filtering permits only
    # safe internal links and rejects traversal for checksum-pinned source inputs.
    with tarfile.open(archive) as source:
        for member in source.getmembers():
            if not allow_links and (member.issym() or member.islnk()):
                raise ValueError(f"archive link: {member.name}")
        source.extractall(destination, filter="data")
    return next(destination.iterdir())


def main():
    if sys.flags.optimize:
        raise RuntimeError("acceptance requires enabled assertions")
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "cargo",
        "rustc",
        "git_source",
        "zlib_source",
        "cli_tests",
    ):
        parser.add_argument(name, type=Path)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--runfiles-root", type=Path, required=True)
    parser.add_argument("--workspace", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--expected-version", required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(mode=0o700)  # Refuse accidental reuse or cleanup of caller data.
    log = output / "commands.log"
    try:
        report_path = output / "validation-report.json"
        shutil.copyfile(args.report, report_path)
        unpacked = validate_verified_archive(
            report_path,
            args.archive,
            output / "verified-handoff",
            expected_package={
                "name": "worktree-pool",
                "version": args.expected_version,
            },
        )
        report = json.loads(report_path.read_text())
        if selected_targets(report["selection"]) != [
            "bin:worktree-pool",
            "lib:worktree_pool",
        ]:
            raise ValueError(
                "verified selection must cover worktree_pool library and worktree-pool binary"
            )
        required = {
            "LICENSE",
            "README.md",
            "CONTRIBUTING.md",
            "WIRE_SCHEMA.md",
            "DEPENDENCIES.md",
            "lib.rs",
            "main.rs",
        }
        missing = required - {item["path"] for item in report["inputs"]}
        if missing:
            raise ValueError(f"missing worktree-pool package inputs: {sorted(missing)}")
        normalized = tomllib.loads((unpacked / "Cargo.toml").read_text())
        if (
            normalized["lib"].get("name") != "worktree_pool"
            or normalized["lib"].get("path") != "lib.rs"
        ):
            raise ValueError("worktree-pool library name/path differs")
        if [
            (binary["name"], binary["path"]) for binary in normalized.get("bin", [])
        ] != [("worktree-pool", "main.rs")]:
            raise ValueError("worktree-pool binary name/path differs")
        (output / "artifact.json").write_text(
            json.dumps(
                {
                    "generic_run_id": report["run_id"],
                    "archive_sha256": report["archive"]["sha256"],
                    "verified_archive": str(output / "verified-handoff/verified.crate"),
                    "validation_report": str(report_path),
                    "extracted_package": str(unpacked),
                },
                indent=2,
            )
            + "\n"
        )
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise ValueError(
            f"artifact_validation: archive {args.archive}, report {args.report}: {error}"
        ) from error
    for parent in (output, *output.parents):
        for name in ("config", "config.toml"):
            config = parent / ".cargo" / name
            if config.exists():
                raise ValueError(
                    f"installation_configuration: ambient Cargo configuration: {config}; choose an isolated output directory"
                )
    allowed = (
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
    )
    env = {key: value for key, value in os.environ.items() if key in allowed}
    for name in ("home", "cargo-home", "tmp"):
        (output / name).mkdir(mode=0o700)
    wrapper = output / "rustc-pinned.sh"
    wrapper.write_text(
        "#!/usr/bin/env bash\nexec "
        + shlex.quote(str(args.rustc))
        + " --sysroot "
        + shlex.quote(str(args.sysroot))
        + ' "$@"\n'
    )
    wrapper.chmod(0o700)
    env.update(
        HOME=str(output / "home"),
        TMPDIR=str(output / "tmp"),
        RUSTC=str(wrapper),
        CARGO_HOME=str(output / "cargo-home"),
        CARGO_TARGET_DIR=str(output / "cargo-target"),
        CARGO_BUILD_JOBS="2",
        LANG="C.UTF-8",
        LC_ALL="C.UTF-8",
    )
    config = output / "cargo-home/config.toml"
    config.write_text('[registries.crates-io]\nprotocol = "sparse"\n')
    (output / "installation-configuration.json").write_text(
        json.dumps(
            {
                "environment": env,
                "cwd": str(output),
                "cargo_config": str(config),
                "cargo_config_sha256": digest(config),
                "rustc_wrapper": str(wrapper),
                "rustc_wrapper_sha256": digest(wrapper),
            },
            indent=2,
        )
        + "\n"
    )
    rust = subprocess.check_output([args.rustc, "-Vv"], env=env, text=True)
    cargo = subprocess.check_output([args.cargo, "-V"], env=env, text=True)
    assert "release: 1.96.0\n" in rust, rust
    assert cargo.startswith("cargo 1.96.0 "), cargo
    assert "host: x86_64-unknown-linux-gnu\n" in rust, rust
    assert platform.system() == "Linux" and platform.machine() == "x86_64"
    (output / "toolchain.txt").write_text(rust + cargo)
    run(
        [
            args.cargo,
            "install",
            "--locked",
            "--path",
            unpacked,
            "--root",
            output / "installed",
        ],
        cwd=output,
        env=env,
        log=log,
    )
    binary = output / "installed/bin/worktree-pool"
    assert binary.read_bytes()[:4] == b"\x7fELF"
    run(["file", binary], cwd=output, env=env, log=output / "native-closure.log")
    run(["ldd", binary], cwd=output, env=env, log=output / "native-closure.log")
    run(["cc", "--version"], cwd=output, env=env, log=output / "native-build-tools.log")
    run(
        ["make", "--version"],
        cwd=output,
        env=env,
        log=output / "native-build-tools.log",
    )
    zlib = extract(args.zlib_source, output / "zlib-source", allow_links=True)
    run(
        ["sh", "configure", "--static", f"--prefix={output / 'zlib'}"],
        cwd=zlib,
        env=env,
        log=log,
    )
    run(["make", "-j2", "install"], cwd=zlib, env=env, log=log)
    git = extract(args.git_source, output / "git-source", allow_links=True)
    flags = [
        "CFLAGS=-O2 -std=gnu99",
        f"prefix={output / 'git'}",
        f"ZLIB_PATH={output / 'zlib'}",
        "NO_GETTEXT=YesPlease",
        "NO_TCLTK=YesPlease",
        "NO_PERL=YesPlease",
        "NO_PYTHON=YesPlease",
        "NO_CURL=YesPlease",
        "NO_EXPAT=YesPlease",
        "NO_OPENSSL=YesPlease",
        "NO_ICONV=YesPlease",
    ]
    run(["make", "-j2", *flags, "install"], cwd=git, env=env, log=log)
    private_path = output / "runtime-bin"
    private_path.mkdir()
    for tool in ("sh", "sleep"):
        (private_path / tool).symlink_to(shutil.which(tool))
    (private_path / "git").symlink_to(output / "git/bin/git")
    runtime = {
        "PATH": str(private_path),
        "HOME": str(output / "runtime-home"),
        "GIT_EXEC_PATH": str(output / "git/libexec/git-core"),
        "TEST_SRCDIR": str(args.runfiles_root),
        "TEST_WORKSPACE": args.workspace,
        "POOL_INSTALLED_BINARY": str(binary),
    }
    (output / "runtime-home").mkdir()
    for tool in ("cargo", "rustc", "bazel"):
        assert shutil.which(tool, path=runtime["PATH"]) is None
    version = subprocess.check_output(
        [private_path / "git", "--version"], env=runtime, text=True
    ).strip()
    assert version == "git version 2.36.0", version
    unrelated = output / "unrelated-working-directory"
    unrelated.mkdir()
    installed_version = subprocess.check_output(
        [binary, "--version"], cwd=unrelated, env=runtime, text=True
    ).strip()
    assert installed_version == "worktree-pool " + args.expected_version, (
        installed_version
    )
    (output / "installed-version.log").write_text(installed_version + "\n")
    run(
        [args.cli_tests, "--test-threads=1"],
        cwd=unrelated,
        env=runtime,
        log=output / "installed-cli.log",
    )
    (output / "runtime.json").write_text(
        json.dumps(
            {
                "git": version,
                "binary_sha256": digest(binary),
                "absent_from_path": ["cargo", "rustc", "bazel"],
                "cwd": str(unrelated),
                "helper_scope": "Bazel test_support children; public CLI commands use Cargo-installed artifact",
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Verified installed public CLI and minimum runtime: {output}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"acceptance failed: {error}", file=sys.stderr)
        sys.exit(1)
