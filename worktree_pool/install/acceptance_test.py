#!/usr/bin/env python3
"""Exercise artifact handoffs through the public installation command.

The portable pair below is controlled report evidence, not certification by Cargo.
The separate real acceptance run certifies the pinned producer and installed CLI.
"""

import copy
import hashlib
import io
import json
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

CHECKS = (
    "input_fidelity",
    "toolchain",
    "configuration",
    "generated_manifest",
    "cargo_package",
    "archive_policy",
    "archive_fidelity",
    "original_manifest",
    "normalized_manifest",
    "locked_metadata",
    "direct_dependencies",
    "locked_builds",
)
TARGET = "x86_64-unknown-linux-gnu"


def fingerprint(contents):
    return hashlib.sha256(contents).hexdigest()


def portable_pair(root):
    """Create a complete contract fixture whose diagnostic paths no longer exist."""
    package = {"name": "worktree-pool", "version": "0.1.0"}
    manifest = b"""[package]
name = "worktree-pool"
version = "0.1.0"
edition = "2024"
[lib]
name = "worktree_pool"
path = "lib.rs"
[[bin]]
name = "worktree-pool"
path = "main.rs"
"""
    declared = {
        "lib.rs": b"pub fn answer() -> u32 { 42 }\n",
        "main.rs": b"fn main() {}\n",
    }
    declared.update(
        {
            name: name.encode()
            for name in (
                "LICENSE",
                "README.md",
                "CONTRIBUTING.md",
                "WIRE_SCHEMA.md",
                "DEPENDENCIES.md",
            )
        }
    )
    lock = b'version = 4\n[[package]]\nname = "worktree-pool"\nversion = "0.1.0"\n'
    files = {
        **declared,
        "Cargo.toml": manifest,
        "Cargo.toml.orig": manifest,
        "Cargo.lock": lock,
    }
    archive = root / "relocated.crate"
    inventory = []
    with tarfile.open(archive, "w:gz") as stream:
        for name, contents in sorted(files.items()):
            member = tarfile.TarInfo("worktree-pool-0.1.0/" + name)
            member.size = len(contents)
            stream.addfile(member, io.BytesIO(contents))
            inventory.append(
                {
                    "path": name,
                    "type": "file",
                    "size": len(contents),
                    "sha256": fingerprint(contents),
                }
            )
    inputs = [
        {"path": name, "size": len(contents), "sha256": fingerprint(contents)}
        for name, contents in sorted(declared.items())
    ]
    manifests = {
        name: {"path": "/gone/" + name, "sha256": fingerprint(manifest)}
        for name in ("generated", "original", "normalized")
    }
    toolchain = {
        "host": TARGET,
        "target": TARGET,
        "rustc": "rustc 1.96.0\nrelease: 1.96.0\n",
        "cargo": "cargo 1.96.0 (fixture)",
    }
    configuration = {"target": TARGET}
    archive_evidence = {
        "path": "/gone/package.crate",
        "sha256": fingerprint(archive.read_bytes()),
        "size": archive.stat().st_size,
        "inventory": inventory,
    }
    dependencies = {
        "requested": [],
        "observed": [{"name": "worktree-pool", "version": "0.1.0"}],
        "metadata_path": "/gone/metadata.json",
        "metadata_sha256": fingerprint(b"metadata"),
    }
    targets = ["bin:worktree-pool", "lib:worktree_pool"]
    evidence = {
        "input_fidelity": {"inputs": inputs, "manifest_sha256": fingerprint(manifest)},
        "toolchain": {"versions": toolchain, "command_ids": [1, 2]},
        "configuration": configuration,
        "generated_manifest": {
            "package": package,
            "targets": targets,
            "dependencies": [],
        },
        "cargo_package": {
            "archive": archive_evidence["path"],
            "verification_enabled": True,
            "command_ids": [3],
        },
        "archive_policy": {"inventory": inventory, "root": "worktree-pool-0.1.0"},
        "archive_fidelity": {"inputs": inputs, "verified": True},
        "original_manifest": manifests["original"],
        "normalized_manifest": {
            "manifest": manifests["normalized"],
            "package": package,
            "dependencies": [],
        },
        "locked_metadata": {
            "path": dependencies["metadata_path"],
            "sha256": dependencies["metadata_sha256"],
            "lockfile_sha256": fingerprint(lock),
            "command_ids": [4],
        },
        "direct_dependencies": {
            "requested": [],
            "resolved_graph": dependencies["observed"],
            "compared": True,
        },
        "locked_builds": {"targets": targets, "command_ids": [5, 6]},
    }
    commands = [
        ("toolchain", ["rustc", "-Vv"]),
        ("toolchain", ["cargo", "-V"]),
        ("cargo_package", ["cargo", "package"]),
        ("locked_metadata", ["cargo", "metadata", "--locked"]),
        ("locked_builds", ["cargo", "build", "--lib", "--locked"]),
        ("locked_builds", ["cargo", "build", "--bin", "worktree-pool", "--locked"]),
    ]
    report = {
        "schema_version": 1,
        "report_kind": "cargo_publish_validation",
        "status": "success",
        "completed": True,
        "run_id": "controlled-portable-handoff",
        "started_at": "2026-10-11T00:00:00Z",
        "completed_at": "2026-10-11T00:00:01Z",
        "package": package,
        "selection": {
            "library": {"crate_name": "worktree_pool"},
            "binaries": [{"name": "worktree-pool"}],
        },
        "configuration": configuration,
        "inputs": inputs,
        "toolchain": toolchain,
        "manifests": manifests,
        "archive": archive_evidence,
        "lockfile": {"sha256": fingerprint(lock)},
        "dependencies": dependencies,
        "commands": [
            {
                "id": index,
                "phase": phase,
                "argv": argv,
                "completed": True,
                "returncode": 0,
                "log": "/gone/command.log",
            }
            for index, (phase, argv) in enumerate(commands, 1)
        ],
        "checks": {
            name: {"completed": True, "status": "passed", "evidence": data}
            for name, data in evidence.items()
        },
    }
    return archive, report


class AcceptanceCommandTest(unittest.TestCase):
    def invoke(self, root, archive, report, *, output="acceptance", environment=None):
        report_path = root / "relocated-report.json"
        if isinstance(report, bytes):
            report_path.write_bytes(report)
        else:
            report_path.write_text(json.dumps(report))
        return subprocess.run(
            [
                str(Path(os.environ["TEST_SRCDIR"]) / "_main" / os.environ["DRIVER"]),
                "/missing/cargo",
                "/missing/rustc",
                "/missing/git-source",
                "/missing/zlib-source",
                "/missing/cli-tests",
                "--archive",
                str(archive),
                "--report",
                str(report_path),
                "--expected-version",
                "0.1.0",
                "--sysroot",
                "/missing/sysroot",
                "--runfiles-root",
                str(root),
                "--workspace",
                "_main",
                "--output",
                str(root / output),
            ],
            cwd=root,
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )

    def rejected(self, root, archive, report, diagnostic, *, output="acceptance"):
        result = self.invoke(root, archive, report, output=output)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("artifact_validation", result.stderr)
        self.assertIn(diagnostic, result.stderr)
        self.assertNotIn("/missing/rustc", result.stderr)
        self.assertFalse((root / output / "installed").exists())

    def test_unsupported_report_is_rejected_before_any_tool_runs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "package.crate"
            archive.write_bytes(b"unverified")
            self.rejected(
                root,
                archive,
                {"schema_version": 99},
                "unsupported cargo_publish validation report contract",
            )

    def test_relocated_verified_pair_reaches_tools_and_retains_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, report = portable_pair(root)
            hostile = dict(
                os.environ,
                RUSTFLAGS="--cfg=ambient",
                RUSTC_WRAPPER="/missing/wrapper",
                CARGO_HOME="/missing/home",
                CC="/missing/cc",
                HOME="/missing/ambient-home",
            )
            result = self.invoke(root, archive, report, environment=hostile)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("artifact_validation", result.stderr)
            self.assertIn("/missing/rustc", result.stderr)
            artifact = json.loads((root / "acceptance/artifact.json").read_text())
            self.assertEqual(
                Path(artifact["verified_archive"]).read_bytes(), archive.read_bytes()
            )
            self.assertEqual(artifact["generic_run_id"], report["run_id"])
            configuration = json.loads(
                (root / "acceptance/installation-configuration.json").read_text()
            )
            environment = configuration["environment"]
            for key in ("RUSTFLAGS", "RUSTC_WRAPPER", "CC"):
                self.assertNotIn(key, environment)
            for key in ("HOME", "CARGO_HOME", "TMPDIR", "RUSTC", "CARGO_TARGET_DIR"):
                self.assertTrue(
                    Path(environment[key]).is_relative_to(root / "acceptance"), key
                )
            self.assertEqual(
                (Path(artifact["extracted_package"]) / "Cargo.lock").read_bytes(),
                b'version = 4\n[[package]]\nname = "worktree-pool"\nversion = "0.1.0"\n',
            )

    def test_unsuccessful_or_incomplete_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, baseline = portable_pair(root)
            cases = []
            for key, value, message in (
                ("report_kind", "unknown", "unsupported"),
                ("status", "failure", "unsuccessful"),
                ("completed", False, "incomplete"),
                ("completed_at", "", "timestamp"),
            ):
                report = copy.deepcopy(baseline)
                report[key] = value
                cases.append((key, report, message))
            missing_completion = copy.deepcopy(baseline)
            del missing_completion["completed"]
            cases.append(("missing_completion", missing_completion, "incomplete"))
            missing_commands = copy.deepcopy(baseline)
            missing_commands["commands"] = []
            cases.append(
                ("missing_commands", missing_commands, "unsuccessful command evidence")
            )
            for name in CHECKS:
                absent = copy.deepcopy(baseline)
                del absent["checks"][name]
                cases.append((name + "absent", absent, name))
                for field, value in (
                    ("completed", False),
                    ("status", "failed"),
                    ("evidence", {}),
                ):
                    report = copy.deepcopy(baseline)
                    report["checks"][name][field] = value
                    cases.append((name + field, report, name))
            for index, (name, report, message) in enumerate(cases):
                with self.subTest(name=name):
                    self.rejected(
                        root, archive, report, message, output=f"rejection-{index}"
                    )

    def test_fabricated_commands_and_mismatched_hashes_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, baseline = portable_pair(root)
            fabricated = copy.deepcopy(baseline)
            fabricated["checks"]["direct_dependencies"]["evidence"] = {
                "fabricated": True
            }
            no_binary = copy.deepcopy(baseline)
            no_binary["commands"][-1]["argv"] = ["cargo", "build", "--lib", "--locked"]
            failed_command = copy.deepcopy(baseline)
            failed_command["commands"][2]["returncode"] = 1
            bad_hash = copy.deepcopy(baseline)
            bad_hash["archive"]["sha256"] = "0" * 64
            for index, (report, message) in enumerate(
                (
                    (fabricated, "direct_dependencies"),
                    (no_binary, "every selected library/binary"),
                    (failed_command, "unsuccessful command evidence"),
                    (bad_hash, "SHA256 or size mismatch"),
                )
            ):
                with self.subTest(message=message):
                    self.rejected(
                        root, archive, report, message, output=f"rejection-{index}"
                    )

    def test_malformed_report_and_absent_archive_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, report = portable_pair(root)
            self.rejected(
                root, archive, b"{", "Expecting property name", output="malformed"
            )
            self.rejected(root, archive, [], "JSON object", output="wrong-shape")
            self.rejected(
                root, root / "absent.crate", report, "absent.crate", output="absent"
            )

    def test_wrong_consumer_identity_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, baseline = portable_pair(root)
            for index, (key, value) in enumerate(
                (("name", "other-package"), ("version", "99.0.0"))
            ):
                report = copy.deepcopy(baseline)
                report["package"][key] = value
                with self.subTest(key=key):
                    self.rejected(
                        root,
                        archive,
                        report,
                        "identity/version",
                        output=f"rejection-{index}",
                    )

    def test_ambient_ancestor_cargo_configuration_is_rejected_before_tools(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, report = portable_pair(root)
            config = root / ".cargo/config.toml"
            config.parent.mkdir()
            config.write_text('[build]\nrustflags = ["--cfg=ambient"]\n')
            result = self.invoke(root, archive, report)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("installation_configuration", result.stderr)
            self.assertIn(str(config), result.stderr)
            self.assertNotIn("/missing/rustc", result.stderr)


if __name__ == "__main__":
    unittest.main()
