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
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

import tomllib


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
        "manifest",
        "git_source",
        "zlib_source",
        "cli_tests",
    ):
        parser.add_argument(name, type=Path)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--runfiles-root", type=Path, required=True)
    parser.add_argument("--workspace", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--phase", choices=("package", "all"), default="all")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(mode=0o700)  # Refuse accidental reuse or cleanup of caller data.
    log = output / "commands.log"
    package = output / "source"
    package.mkdir()
    for path in args.manifest.parent.iterdir():
        if path.is_file() and (
            path.suffix in (".rs", ".md")
            or path.name.startswith("Cargo")
            or path.name in ("LICENSE", "BUILD.bazel")
        ):
            shutil.copyfile(path, package / path.name)
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("CARGO_", "RUST", "PYTHON"))
    }
    env.update(
        RUSTC=str(args.rustc),
        CARGO_HOME=str(output / "cargo-home"),
        CARGO_TARGET_DIR=str(output / "cargo-target"),
        CARGO_BUILD_JOBS="2",
        RUSTFLAGS=f"--sysroot={args.sysroot}",
    )
    rust = subprocess.check_output([args.rustc, "-Vv"], env=env, text=True)
    cargo = subprocess.check_output([args.cargo, "-V"], env=env, text=True)
    assert "release: 1.96.0\n" in rust, rust
    assert cargo.startswith("cargo 1.96.0 "), cargo
    assert "host: x86_64-unknown-linux-gnu\n" in rust, rust
    assert platform.system() == "Linux" and platform.machine() == "x86_64"
    (output / "toolchain.txt").write_text(rust + cargo)
    if not (package / "Cargo.lock").exists():
        run([args.cargo, "generate-lockfile"], cwd=package, env=env, log=log)
    run(
        [args.cargo, "package", "--locked", "--allow-dirty"],
        cwd=package,
        env=env,
        log=log,
    )
    archive = next((output / "cargo-target/package").glob("*.crate"))
    unpacked = extract(archive, output / "extracted")
    contents = sorted(
        p.relative_to(unpacked).as_posix() for p in unpacked.rglob("*") if p.is_file()
    )
    (output / "archive-contents.json").write_text(json.dumps(contents, indent=2) + "\n")
    # This policy is the distribution contract, independent of Cargo include logic.
    required = {
        "Cargo.lock",
        "Cargo.toml",
        "Cargo.toml.orig",
        "LICENSE",
        "README.md",
        "WIRE_SCHEMA.md",
        "DEPENDENCIES.md",
        "lib.rs",
        "main.rs",
    }
    assert required <= set(contents), required - set(contents)
    assert all(
        name.endswith(".rs") or name in required or name == ".cargo_vcs_info.json"
        for name in contents
    ), contents
    normalized = tomllib.loads((unpacked / "Cargo.toml").read_text())
    assert normalized["package"]["name"] == "worktree-pool"
    assert normalized["bin"][0]["name"] == "worktree-pool"
    for group in ("dependencies", "dev-dependencies"):
        assert all(
            "path" not in dep and "workspace" not in dep
            for dep in normalized[group].values()
        )
    assert digest(unpacked / "Cargo.toml.orig") == digest(package / "Cargo.toml")
    hashes = {name: digest(unpacked / name) for name in contents}
    for name in contents:
        if (package / name).is_file() and name != "Cargo.toml":
            assert digest(package / name) == hashes[name], name
    (output / "artifact.json").write_text(
        json.dumps({"archive_sha256": digest(archive), "files": hashes}, indent=2)
        + "\n"
    )
    metadata_bytes = subprocess.check_output(
        [
            args.cargo,
            "metadata",
            "--locked",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
            "--format-version",
            "1",
        ],
        cwd=unpacked,
        env=env,
    )
    metadata = json.loads(metadata_bytes)
    (output / "metadata.json").write_bytes(metadata_bytes)
    by_id = {p["id"]: p for p in metadata["packages"]}
    resolved = [
        {
            "name": by_id[node["id"]]["name"],
            "version": by_id[node["id"]]["version"],
            "features": node["features"],
        }
        for node in metadata["resolve"]["nodes"]
    ]
    (output / "resolved-dependencies.json").write_text(
        json.dumps(sorted(resolved, key=lambda p: (p["name"], p["version"])), indent=2)
        + "\n"
    )
    original = tomllib.loads((package / "Cargo.toml").read_text())
    for group in ("dependencies", "dev-dependencies"):
        for name, dependency in original[group].items():
            version = (
                dependency if isinstance(dependency, str) else dependency["version"]
            )
            assert version.startswith("="), (name, version)
            assert normalized[group][name]["version"] == version, name
            requested = {} if isinstance(dependency, str) else dependency
            actual = normalized[group][name]
            assert actual.get("default-features", True) == requested.get(
                "default-features", True
            ), name
            assert sorted(actual.get("features", [])) == sorted(
                requested.get("features", [])
            ), name
    run(
        [
            args.cargo,
            "tree",
            "--locked",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--edges",
            "normal,build,features",
        ],
        cwd=unpacked,
        env=env,
        log=output / "production-features.log",
    )
    shutil.copyfile(unpacked / "Cargo.lock", output / "Cargo.lock")
    shutil.copyfile(unpacked / "Cargo.toml", output / "Cargo.toml.normalized")
    if args.phase == "package":
        print(f"Verified standalone archive: {archive}")
        return
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
    assert installed_version == "worktree-pool 0.1.0", installed_version
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
    main()
