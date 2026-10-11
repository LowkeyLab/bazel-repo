#!/usr/bin/env python3
"""Validate declared Cargo publishing inputs with isolated pinned Cargo/rustc."""

import argparse
import datetime
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import uuid
from pathlib import Path

import tomllib
from tools.cargo_publish.report import (
    REPORT_KIND,
    REQUIRED_CHECKS,
    SCHEMA_VERSION,
    TARGET,
    digest,
    inspect_archive,
    package_path,
    selected_targets,
    validate_report,
)


def timestamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def write_report(path, report):
    temporary = path.with_suffix(".pending")
    temporary.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


class ValidationRun:
    def __init__(self, args):
        self.args = args
        self.run_id = str(uuid.uuid4())
        self.output = args.output_root.resolve() / self.run_id
        self.output.mkdir(mode=0o700, parents=True)
        self.path = self.output / "report.json"
        self.phase = "input_fidelity"
        self.env = {}
        self.report = {
            "schema_version": SCHEMA_VERSION,
            "report_kind": REPORT_KIND,
            "run_id": self.run_id,
            "status": "incomplete",
            "completed": False,
            "started_at": timestamp(),
            "completed_at": None,
            "checks": {
                name: {"status": "pending", "completed": False}
                for name in REQUIRED_CHECKS
            },
            "commands": [],
            "diagnostics": None,
        }
        self.save()

    def save(self):
        write_report(self.path, self.report)

    def check(self, name, evidence):
        self.report["checks"][name] = {
            "status": "passed",
            "completed": True,
            "evidence": evidence,
        }
        self.save()

    def command(self, argv, cwd=None):
        argv = [str(value) for value in argv]
        identity = str(len(self.report["commands"]) + 1)
        log = self.output / (identity + "-" + self.phase + ".log")
        command = {
            "id": identity,
            "phase": self.phase,
            "argv": argv,
            "cwd": str(cwd or self.output),
            "log": str(log),
            "started_at": timestamp(),
            "completed": False,
            "returncode": None,
        }
        self.report["commands"].append(command)
        self.save()
        try:
            result = subprocess.run(
                argv,
                cwd=cwd or self.output,
                env=self.env,
                capture_output=True,
                check=False,
            )
            log.write_bytes(result.stdout + b"\n--- stderr ---\n" + result.stderr)
            command.update(
                completed=True, returncode=result.returncode, completed_at=timestamp()
            )
            self.save()
            if result.returncode:
                message = (
                    result.stderr.decode(errors="replace").strip()
                    or result.stdout.decode(errors="replace").strip()
                )
                raise ValueError(
                    f"command exited {result.returncode}: {shlex.join(argv)}: {message}; log: {log}"
                )
            return result.stdout, identity
        except OSError as error:
            log.write_text(str(error) + "\n")
            command.update(completed=True, error=str(error), completed_at=timestamp())
            self.save()
            raise

    def configure(self):
        # CARGO_HOME isolates global files. Configuration in any invocation-CWD
        # ancestor cannot be neutralized with --config because Cargo merges it.
        for parent in (self.output, *self.output.parents):
            for name in ("config", "config.toml"):
                path = parent / ".cargo" / name
                if path.exists():
                    raise ValueError(
                        f"ambient Cargo configuration affects validation: {path}; choose an isolated Bazel output root"
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
        self.env = {key: value for key, value in os.environ.items() if key in allowed}
        for name in ("home", "cargo-home", "tmp"):
            (self.output / name).mkdir(mode=0o700)
        # Only registry data and its shared locks are managed outside this run;
        # ambient config and credential files are never loaded from the cache.
        cache = (
            self.args.registry_cache or self.args.output_root / "registry-cache"
        ).resolve()
        cache.mkdir(mode=0o700, parents=True, exist_ok=True)
        (cache / "registry").mkdir(mode=0o700, exist_ok=True)
        home = self.output / "cargo-home"
        (home / "registry").symlink_to(cache / "registry", target_is_directory=True)
        for name in (".package-cache", ".package-cache-mutate", ".global-cache"):
            (cache / name).touch(mode=0o600, exist_ok=True)
            (home / name).symlink_to(cache / name)
        wrapper = self.output / "rustc-pinned.sh"
        wrapper.write_text(
            "#!/usr/bin/env bash\nexec "
            + shlex.quote(str(self.args.rustc))
            + " --sysroot "
            + shlex.quote(str(self.args.sysroot))
            + ' "$@"\n'
        )
        wrapper.chmod(0o700)
        self.env.update(
            HOME=str(self.output / "home"),
            TMPDIR=str(self.output / "tmp"),
            CARGO_HOME=str(home),
            RUSTC=str(wrapper),
            CARGO_TARGET_DIR=str(self.output / "target"),
            CARGO_BUILD_JOBS="2",
            LANG="C.UTF-8",
            LC_ALL="C.UTF-8",
        )
        config = home / "config.toml"
        config.write_text(
            '[registries.crates-io]\nprotocol = "sparse"\n[net]\noffline = '
            + str(self.args.offline).lower()
            + "\n"
        )
        self.report["configuration"] = {
            "target": TARGET,
            "network_mode": "offline" if self.args.offline else "online",
            "cargo_home": str(home),
            "target_directory": self.env["CARGO_TARGET_DIR"],
            "registry_cache": str(cache),
            "environment": self.env,
            "config_path": str(config),
            "config_sha256": digest(config),
            "rustc_wrapper": str(wrapper),
            "rustc_wrapper_sha256": digest(wrapper),
            "cwd": str(self.output),
        }

    def cargo(self, command, manifest, *extra):
        argv = [self.args.cargo, command, "--manifest-path", manifest, *extra]
        if self.args.offline:
            argv.append("--offline")
        return self.command(argv)

    def execute(self):
        contract = json.loads(self.args.contract.read_text())
        self.report.update(
            package=contract["package"],
            selection={
                key: contract.get(key) for key in ("library", "binaries", "tests")
            },
            inputs=contract["inputs"],
            provenance=contract.get("provenance", {}),
            generation_contract={
                "path": str(self.args.contract),
                "sha256": digest(self.args.contract),
            },
        )
        staged = self.output / "staged"
        staged.mkdir(mode=0o700)
        paths = set()
        for item in contract["inputs"]:
            name = str(package_path(item["path"]))
            if name in paths:
                raise ValueError(f"conflicting declared input path: {name}")
            paths.add(name)
            source = self.args.package / name
            if (
                not source.is_file()
                or source.is_symlink()
                or digest(source) != item["sha256"]
                or source.stat().st_size != item["size"]
            ):
                raise ValueError(f"declared input fidelity mismatch: {name}")
            destination = staged / name
            destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
        generated = self.args.package / "Cargo.toml"
        if digest(generated) != contract["manifest_sha256"]:
            raise ValueError("generated Cargo.toml fingerprint mismatch")
        shutil.copyfile(generated, staged / "Cargo.toml")
        self.check(
            "input_fidelity",
            {"inputs": contract["inputs"], "manifest_sha256": digest(generated)},
        )
        self.phase = "toolchain"
        # Tool probes use only a minimal native-loader environment. Configuration
        # is then established for every registry/metadata/build operation.
        self.env = {
            key: value
            for key, value in os.environ.items()
            if key in ("PATH", "NIX_LD", "NIX_LD_LIBRARY_PATH")
        }
        rust_bytes, rust_id = self.command([self.args.rustc, "-Vv"])
        cargo_bytes, cargo_id = self.command([self.args.cargo, "-V"])
        rust, cargo = rust_bytes.decode(), cargo_bytes.decode()
        if (
            "release: 1.96.0\n" not in rust
            or "host: " + TARGET + "\n" not in rust
            or not cargo.startswith("cargo 1.96.0 ")
        ):
            raise ValueError(f"unsupported pinned toolchain: {rust} {cargo}")
        if platform.system() != "Linux" or platform.machine() != "x86_64":
            raise ValueError(f"unsupported runtime host: {platform.platform()}")
        self.report["toolchain"] = {
            "rustc": rust,
            "cargo": cargo,
            "host": TARGET,
            "target": TARGET,
            "rustc_path": str(self.args.rustc),
            "cargo_path": str(self.args.cargo),
            "sysroot": str(self.args.sysroot),
        }
        self.check(
            "toolchain",
            {"command_ids": [rust_id, cargo_id], "versions": self.report["toolchain"]},
        )
        self.phase = "configuration"
        self.configure()
        self.check("configuration", self.report["configuration"])
        self.phase = "generated_manifest"
        if contract.get("schema_version") != 1 or contract.get("target") != TARGET:
            raise ValueError("unsupported generation contract schema_version or target")
        selected_targets(contract)
        compare_manifest(staged / "Cargo.toml", contract)
        self.report["manifests"] = {
            "generated": {
                "path": str(staged / "Cargo.toml"),
                "sha256": digest(staged / "Cargo.toml"),
            }
        }
        self.check(
            "generated_manifest",
            {
                "package": contract["package"],
                "targets": selected_targets(contract),
                "dependencies": contract["dependencies"],
            },
        )
        self.phase = "cargo_package"
        _, package_id = self.cargo(
            "package", staged / "Cargo.toml", "--target", TARGET, "--allow-dirty"
        )
        archives = list((self.output / "target" / "package").glob("*.crate"))
        expected_name = (
            contract["package"]["name"]
            + "-"
            + contract["package"]["version"]
            + ".crate"
        )
        if len(archives) != 1 or archives[0].name != expected_name:
            raise ValueError(
                f"missing or unexpected packaged archive: expected {expected_name}, found {archives}"
            )
        archive = archives[0]
        self.check(
            "cargo_package",
            {
                "command_ids": [package_id],
                "archive": str(archive),
                "verification_enabled": True,
            },
        )
        self.phase = "archive_policy"
        extracted, inventory = inspect_archive(
            archive, contract["package"], contract["inputs"], self.output / "extracted"
        )
        self.report["archive"] = {
            "path": str(archive),
            "sha256": digest(archive),
            "size": archive.stat().st_size,
            "inventory": inventory,
            "extracted_root": str(extracted),
            "fidelity": "passed",
        }
        self.check("archive_policy", {"inventory": inventory, "root": extracted.name})
        self.phase = "archive_fidelity"
        self.check("archive_fidelity", {"inputs": contract["inputs"], "verified": True})
        self.phase = "original_manifest"
        if digest(extracted / "Cargo.toml.orig") != contract["manifest_sha256"]:
            raise ValueError("Cargo.toml.orig differs from generated manifest")
        self.report["manifests"]["original"] = {
            "path": str(extracted / "Cargo.toml.orig"),
            "sha256": digest(extracted / "Cargo.toml.orig"),
        }
        self.check("original_manifest", self.report["manifests"]["original"])
        self.phase = "normalized_manifest"
        compare_manifest(extracted / "Cargo.toml", contract)
        self.report["manifests"]["normalized"] = {
            "path": str(extracted / "Cargo.toml"),
            "sha256": digest(extracted / "Cargo.toml"),
        }
        self.report["lockfile"] = {
            "path": str(extracted / "Cargo.lock"),
            "sha256": digest(extracted / "Cargo.lock"),
        }
        self.check(
            "normalized_manifest",
            {
                "manifest": self.report["manifests"]["normalized"],
                "package": contract["package"],
                "dependencies": contract["dependencies"],
            },
        )
        self.phase = "locked_metadata"
        metadata_bytes, metadata_id = self.cargo(
            "metadata",
            extracted / "Cargo.toml",
            "--locked",
            "--filter-platform",
            TARGET,
            "--format-version",
            "1",
        )
        metadata_path = self.output / "metadata.json"
        metadata_path.write_bytes(metadata_bytes)
        metadata = json.loads(metadata_bytes)
        self.check(
            "locked_metadata",
            {
                "command_ids": [metadata_id],
                "path": str(metadata_path),
                "sha256": digest(metadata_path),
                "lockfile_sha256": self.report["lockfile"]["sha256"],
            },
        )
        self.phase = "direct_dependencies"
        resolved = compare_resolution(metadata, contract, extracted)
        self.report["dependencies"] = {
            "requested": contract["dependencies"],
            "observed": resolved,
            "metadata_path": str(metadata_path),
            "metadata_sha256": digest(metadata_path),
        }
        self.check(
            "direct_dependencies",
            {
                "requested": contract["dependencies"],
                "resolved_graph": resolved,
                "compared": True,
            },
        )
        self.phase = "locked_builds"
        build_ids = []
        for selected in selected_targets(contract):
            selector = (
                ["--lib"] if selected.startswith("lib:") else ["--bin", selected[4:]]
            )
            _, build_id = self.cargo(
                "build",
                extracted / "Cargo.toml",
                "--locked",
                "--target",
                TARGET,
                *selector,
            )
            build_ids.append(build_id)
        if digest(extracted / "Cargo.lock") != self.report["lockfile"]["sha256"]:
            raise ValueError("packaged Cargo.lock changed during locked validation")
        self.check(
            "locked_builds",
            {"command_ids": build_ids, "targets": selected_targets(contract)},
        )
        self.report.update(status="success", completed=True, completed_at=timestamp())
        validate_report(self.report)
        self.save()
        print(
            json.dumps(
                {
                    "status": "success",
                    "run_id": self.run_id,
                    "archive": str(archive),
                    "report": str(self.path),
                }
            )
        )


def dependency_fields(dependency, alias):
    if isinstance(dependency, str):
        dependency = {"version": dependency}
    if any(
        key in dependency
        for key in (
            "path",
            "workspace",
            "git",
            "registry",
            "registry-index",
            "branch",
            "tag",
            "rev",
        )
    ) or dependency.get("optional", False):
        raise ValueError(f"unsupported dependency requirement: {alias}: {dependency}")
    return {
        "package": dependency.get("package", alias),
        "version": dependency.get("version"),
        "default_features": dependency.get("default-features", True),
        "features": sorted(dependency.get("features", [])),
    }


def compare_manifest(path, contract):
    manifest = tomllib.loads(path.read_text())
    for key, value in contract["package"].items():
        if manifest.get("package", {}).get(key) != value:
            raise ValueError(
                f"{path.name}: package.{key} differs from declared metadata"
            )
    for key in (
        "workspace",
        "patch",
        "replace",
        "target",
        "features",
        "build-dependencies",
    ):
        if manifest.get(key):
            raise ValueError(f"{path.name}: unsupported {key} requirement")
    if manifest["package"].get("build", False) is not False:
        raise ValueError(f"{path.name}: publisher build script is unsupported")
    library = contract.get("library")
    actual = manifest.get("lib", {})
    if library:
        if (
            actual.get("name") != library["crate_name"]
            or actual.get("path") != library["path"]
            or actual.get("edition", manifest["package"]["edition"])
            != library["edition"]
        ):
            raise ValueError(
                f"{path.name}: selected library name/path/edition differs: {library['label']}"
            )
        if actual.get("crate-type", ["lib"]) not in (["lib"], ["rlib"]):
            raise ValueError(f"{path.name}: unsupported library crate-type")
    elif actual:
        raise ValueError(f"{path.name}: undeclared package library")
    expected_binaries = {
        (binary["name"], binary["path"], binary["edition"])
        for binary in contract.get("binaries", [])
    }
    actual_binaries = {
        (
            binary.get("name"),
            binary.get("path"),
            binary.get("edition", manifest["package"]["edition"]),
        )
        for binary in manifest.get("bin", [])
    }
    if (
        len(actual_binaries) != len(manifest.get("bin", []))
        or actual_binaries != expected_binaries
    ):
        raise ValueError(
            f"{path.name}: selected binary name/path/edition differs: expected {expected_binaries}; actual {actual_binaries}"
        )
    if manifest.get("test") or manifest.get("example") or manifest.get("bench"):
        raise ValueError(f"{path.name}: undeclared package target")
    for kind, group in (("normal", "dependencies"), ("dev", "dev-dependencies")):
        expected = {
            item["alias"]: {
                "package": item["package"],
                "version": "=" + item["version"],
                "default_features": item["default_features"],
                "features": sorted(
                    feature for feature in item["features"] if feature != "default"
                ),
            }
            for item in contract["dependencies"]
            if item["kind"] == kind
        }
        observed = {
            alias: dependency_fields(dep, alias)
            for alias, dep in manifest.get(group, {}).items()
        }
        if expected != observed:
            raise ValueError(
                f"{path.name}: {group} identity/version/alias/features differ: expected {expected}; actual {observed}"
            )


def compare_resolution(metadata, contract, extracted):
    packages = {item["id"]: item for item in metadata["packages"]}
    root_id = metadata["resolve"]["root"]
    root = packages[root_id]
    if (
        root["name"] != contract["package"]["name"]
        or root["version"] != contract["package"]["version"]
    ):
        raise ValueError("Cargo metadata root package identity/version differs")
    expected_declarations = {
        (item["alias"], item["kind"]): {
            "package": item["package"],
            "version": "=" + item["version"],
            "default_features": item["default_features"],
            "features": sorted(
                feature for feature in item["features"] if feature != "default"
            ),
        }
        for item in contract["dependencies"]
    }
    actual_declarations = {}
    for dependency in root["dependencies"]:
        alias = dependency.get("rename") or dependency["name"]
        key = (alias, dependency.get("kind") or "normal")
        if (
            key in actual_declarations
            or dependency.get("optional")
            or dependency.get("target") is not None
            or dependency.get("source")
            != "registry+https://github.com/rust-lang/crates.io-index"
        ):
            raise ValueError(
                f"unsupported or conflicting direct metadata declaration: {alias}: {dependency}"
            )
        actual_declarations[key] = {
            "package": dependency["name"],
            "version": dependency["req"],
            "default_features": dependency["uses_default_features"],
            "features": sorted(dependency["features"]),
        }
    if actual_declarations != expected_declarations:
        raise ValueError(
            f"Cargo metadata direct identity/alias/kind/requirement/defaults/features differ: expected {expected_declarations}; actual {actual_declarations}"
        )
    expected_targets = set()
    library = contract.get("library")
    if library:
        expected_targets.add(
            ("lib", library["crate_name"], library["path"], library["edition"])
        )
    expected_targets.update(
        ("bin", binary["name"], binary["path"], binary["edition"])
        for binary in contract.get("binaries", [])
    )
    actual_targets = set()
    for target in root["targets"]:
        kind = "lib" if target["kind"] in (["lib"], ["rlib"]) else target["kind"][0]
        actual_targets.add(
            (
                kind,
                target["name"],
                Path(target["src_path"]).relative_to(extracted).as_posix(),
                target["edition"],
            )
        )
    if (
        len(actual_targets) != len(root.get("targets", []))
        or actual_targets != expected_targets
    ):
        raise ValueError(f"Cargo metadata selected targets differ: {actual_targets}")
    nodes = metadata["resolve"]["nodes"]
    root_node = next(node for node in nodes if node["id"] == root_id)
    expected = {
        (
            item["alias"].replace("-", "_"),
            item["kind"],
            item["package"],
            item["version"],
        )
        for item in contract["dependencies"]
    }
    actual = set()
    for edge in root_node["deps"]:
        dependency = packages[edge["pkg"]]
        if (
            dependency.get("source")
            != "registry+https://github.com/rust-lang/crates.io-index"
        ):
            raise ValueError(
                f"unsupported direct resolved source: {edge['name']}: {dependency.get('source')}"
            )
        for kind in edge["dep_kinds"]:
            if kind.get("target") is not None:
                raise ValueError(
                    f"unsupported conditional direct dependency: {edge['name']}"
                )
            actual.add(
                (
                    edge["name"],
                    kind["kind"] or "normal",
                    dependency["name"],
                    dependency["version"],
                )
            )
    if actual != expected:
        raise ValueError(
            f"resolved direct identity/version/alias/kind mismatch: expected {expected}; actual {actual}"
        )
    return [
        {
            "id": node["id"],
            "name": packages[node["id"]]["name"],
            "version": packages[node["id"]]["version"],
            "source": packages[node["id"]].get("source"),
            "features": sorted(node["features"]),
            "deps": node["deps"],
        }
        for node in nodes
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("package", "contract", "cargo", "rustc", "sysroot", "output-root"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--registry-cache", type=Path)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    for name in ("package", "contract", "cargo", "rustc", "sysroot"):
        setattr(args, name, getattr(args, name).absolute())
    run = None
    try:
        os.umask(0o077)
        run = ValidationRun(args)
        run.execute()
        return 0
    except (
        ValueError,
        OSError,
        KeyError,
        TypeError,
        StopIteration,
        RuntimeError,
        tarfile.TarError,
    ) as error:
        if run:
            run.report.update(
                status="failure",
                completed=False,
                completed_at=timestamp(),
                diagnostics={"phase": run.phase, "message": str(error)},
            )
            run.report["checks"][run.phase] = {
                "status": "failed",
                "completed": False,
                "error": str(error),
            }
            run.save()
            print(f"{run.phase}: {error}; retained report: {run.path}", file=sys.stderr)
        else:
            print(f"runtime initialization: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
