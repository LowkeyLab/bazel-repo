#!/usr/bin/env python3
"""Verify worktree_pool's public generated Cargo publishing inputs."""

import hashlib
import json
import os
import subprocess
import unittest
from pathlib import Path

import tomllib


class WorktreePoolPublishingTest(unittest.TestCase):
    def test_generated_package_preserves_identity_and_compiled_cli_version(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/worktree_pool"
        manifest = tomllib.loads(
            (root / "publish_files.package/Cargo.toml").read_text()
        )
        self.assertEqual(
            {
                key: manifest["package"][key]
                for key in (
                    "name",
                    "version",
                    "edition",
                    "rust-version",
                    "description",
                    "license",
                    "repository",
                    "readme",
                )
            },
            {
                "name": "worktree-pool",
                "version": "0.1.0",
                "edition": "2024",
                "rust-version": "1.96",
                "description": "Explicit ownership for persistent local Git worktrees",
                "license": "AGPL-3.0-only",
                "repository": "https://github.com/LowkeyLab/bazel-repo",
                "readme": "README.md",
            },
        )
        self.assertEqual(manifest["lib"], {"name": "worktree_pool", "path": "lib.rs"})
        self.assertEqual(
            manifest["bin"],
            [{"name": "worktree-pool", "path": "main.rs", "edition": "2024"}],
        )
        version = subprocess.check_output(
            [str(root / "worktree_pool"), "--version"], text=True
        )
        self.assertEqual(
            version, "worktree-pool " + manifest["package"]["version"] + "\n"
        )

    def test_package_contains_only_declared_production_sources_and_documentation(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/worktree_pool"
        files = root / "publish_files.package"
        expected_sources = {
            "acquisition.rs",
            "acquisition_tests.rs",
            "acquisition_workflow.rs",
            "catalog.rs",
            "catalog_recovery_cli.rs",
            "cli.rs",
            "coordination.rs",
            "creation.rs",
            "creation_workflow.rs",
            "diagnostics.rs",
            "diagnostics_tests.rs",
            "domain.rs",
            "error.rs",
            "git.rs",
            "lib.rs",
            "main.rs",
            "management.rs",
            "paths.rs",
            "rebuild.rs",
            "rebuild_tests.rs",
            "recovery.rs",
            "recovery_inspection.rs",
            "recovery_workflow.rs",
            "release.rs",
            "release_workflow.rs",
            "relocation.rs",
            "relocation_events.rs",
            "relocation_tests.rs",
            "repository_recovery.rs",
            "resources.rs",
            "retirement.rs",
            "retirement_workflow.rs",
            "store.rs",
            "store_tests.rs",
            "workflows.rs",
        }
        assets = {
            "README.md",
            "LICENSE",
            "CONTRIBUTING.md",
            "WIRE_SCHEMA.md",
            "DEPENDENCIES.md",
        }
        self.assertEqual(
            {
                path.relative_to(files).as_posix()
                for path in files.rglob("*")
                if path.is_file()
            },
            expected_sources | assets | {"Cargo.toml"},
        )
        for asset in assets:
            self.assertEqual((files / asset).read_bytes(), (root / asset).read_bytes())
        contract = json.loads((root / "publish_files.json").read_text())
        self.assertEqual(
            {item["path"] for item in contract["inputs"]}, expected_sources | assets
        )
        for item in contract["inputs"]:
            self.assertEqual(
                item["sha256"],
                hashlib.sha256((files / item["path"]).read_bytes()).hexdigest(),
            )
        self.assertEqual(
            {test["label"].split(":")[-1] for test in contract["tests"]},
            {"worktree_pool_lib_test", "cli_test"},
        )
        self.assertNotIn("test", tomllib.loads((files / "Cargo.toml").read_text()))

    def test_dependencies_follow_bazel_kinds_aliases_and_enabled_features(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main"
        manifest = tomllib.loads(
            (root / "worktree_pool/publish_files.package/Cargo.toml").read_text()
        )
        production = manifest["dependencies"]
        development = manifest["dev-dependencies"]
        self.assertEqual(
            set(production),
            {
                "chrono",
                "clap",
                "cloudevents",
                "config",
                "redb",
                "serde",
                "serde_json",
                "sha2",
                "thiserror",
                "tracing",
                "tracing_subscriber",
                "uuid",
            },
        )
        self.assertEqual(
            set(development),
            {
                "googletest",
                "insta",
                "proptest",
                "redb",
                "serde_json",
                "tempfile",
                "uuid",
            },
        )
        self.assertEqual(production["cloudevents"]["package"], "cloudevents-sdk")
        self.assertEqual(
            production["tracing_subscriber"]["package"], "tracing-subscriber"
        )
        locked = tomllib.loads((root / "Cargo.lock").read_text())["package"]
        for dependency in list(production.values()) + list(development.values()):
            self.assertEqual(
                len(
                    [
                        package
                        for package in locked
                        if package["name"] == dependency["package"]
                        and package["version"]
                        == dependency["version"].removeprefix("=")
                        and package.get("source")
                        == "registry+https://github.com/rust-lang/crates.io-index"
                    ]
                ),
                1,
            )
            self.assertTrue(dependency["version"].startswith("="))
        for shared in ("redb", "serde_json", "uuid"):
            self.assertEqual(production[shared], development[shared])
        self.assertTrue(production["config"]["default-features"])
        self.assertTrue(
            {"json", "toml", "yaml"}.issubset(production["config"]["features"])
        )
        self.assertIn("v7", production["uuid"]["features"])
        self.assertIn("env", production["clap"]["features"])
        self.assertIn("derive", production["serde"]["features"])
        self.assertTrue(production["serde_json"]["default-features"])
        self.assertFalse(production["sha2"]["default-features"])
        self.assertFalse(production["cloudevents"]["default-features"])
        self.assertFalse(development["proptest"]["default-features"])
        self.assertIn("std", development["proptest"]["features"])
        contract = json.loads((root / "worktree_pool/publish_files.json").read_text())
        self.assertEqual(
            {(item["kind"], item["alias"]) for item in contract["dependencies"]},
            {("normal", alias) for alias in production}
            | {("dev", alias) for alias in development},
        )

    def test_ordinary_compilation_has_no_publishing_dependencies(self):
        query = (
            Path(os.environ["TEST_SRCDIR"])
            / "_main/worktree_pool/forbidden_compilation_dependencies"
        )
        self.assertEqual(
            query.read_text().strip(),
            "",
            "ordinary compilation acquired publishing-only dependencies",
        )


if __name__ == "__main__":
    unittest.main()
