#!/usr/bin/env python3
"""Exercise generated files through the public cargo_publish macro."""

import hashlib
import json
import os
import unittest
from pathlib import Path

import tomllib


class PublishingFilesTest(unittest.TestCase):
    def test_binary_selection_preserves_mapped_name_and_configured_dependency(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/tools/cargo_publish/fixtures"
        files = root / "binary_publish_files.package"
        manifest = tomllib.loads((files / "Cargo.toml").read_text())
        self.assertNotIn("lib", manifest)
        self.assertEqual(
            manifest["bin"],
            [{"name": "fixture-cli", "path": "src/bin/main.rs", "edition": "2024"}],
        )
        self.assertEqual(manifest["dependencies"]["anyhow"]["version"], "=1.0.104")
        contract = json.loads((root / "binary_publish_files.json").read_text())
        self.assertIsNone(contract["library"])
        self.assertEqual(contract["binaries"][0]["crate_name"], "bazel_binary")

    def test_combined_selection_preserves_test_dependency_kinds_and_ignores_runtime_wiring(
        self,
    ):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/tools/cargo_publish/fixtures"
        files = root / "combined_publish_files.package"
        manifest = tomllib.loads((files / "Cargo.toml").read_text())
        self.assertEqual(
            manifest["lib"], {"name": "fixture_library", "path": "src/lib.rs"}
        )
        self.assertEqual(
            [(b["name"], b["edition"]) for b in manifest["bin"]],
            [("fixture-cli", "2024"), ("fixture-other", "2021")],
        )
        self.assertEqual(
            set(manifest["dependencies"]), {"anyhow", "clap", "renamed_json", "sha2"}
        )
        self.assertEqual(
            set(manifest["dev-dependencies"]),
            {"anyhow", "googletest", "renamed_json", "tempfile"},
        )
        self.assertEqual(
            manifest["dependencies"]["anyhow"], manifest["dev-dependencies"]["anyhow"]
        )
        self.assertTrue(manifest["dependencies"]["renamed_json"]["default-features"])
        self.assertIn("derive", manifest["dependencies"]["clap"]["features"])
        self.assertIn("getrandom", manifest["dev-dependencies"]["tempfile"]["features"])
        self.assertTrue(manifest["dev-dependencies"]["googletest"]["default-features"])
        self.assertFalse((files / "src/tests/helper.rs").exists())
        self.assertFalse((files / "src/tests/cli.rs").exists())
        self.assertNotIn("test", manifest)
        contract = json.loads((root / "combined_publish_files.json").read_text())
        self.assertEqual(
            {d["kind"] for d in contract["dependencies"]}, {"normal", "dev"}
        )
        self.assertEqual(len(contract["tests"]), 2)
        self.assertEqual(
            (root / "combined_repeat_files.json").read_bytes(),
            (root / "combined_publish_files.json").read_bytes(),
        )
        for path in files.rglob("*"):
            if path.is_file():
                self.assertEqual(
                    path.read_bytes(),
                    (
                        root / "combined_repeat_files.package" / path.relative_to(files)
                    ).read_bytes(),
                )

    def test_library_has_standalone_exact_dependencies(self):
        files = (
            Path(os.environ["TEST_SRCDIR"])
            / "_main/tools/cargo_publish/fixtures/publish_files.package"
        )
        manifest = tomllib.loads((files / "Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["name"], "publishing-fixture")
        self.assertEqual(manifest["package"]["edition"], "2024")
        self.assertEqual(
            manifest["lib"], {"name": "fixture_library", "path": "src/lib.rs"}
        )
        self.assertEqual(manifest["dependencies"]["anyhow"]["version"], "=1.0.104")
        self.assertFalse((files / "Cargo.lock").exists())

    def test_alias_preserves_configured_default_and_explicit_features(self):
        files = (
            Path(os.environ["TEST_SRCDIR"])
            / "_main/tools/cargo_publish/fixtures/publish_files.package"
        )
        manifest = tomllib.loads((files / "Cargo.toml").read_text())
        dependency = manifest["dependencies"]["renamed_json"]
        self.assertEqual(dependency["package"], "serde_json")
        self.assertEqual(dependency["version"], "=1.0.152")
        self.assertTrue(dependency["default-features"])
        self.assertIn("raw_value", dependency["features"])
        self.assertIn("unbounded_depth", dependency["features"])
        self.assertNotIn("serde_json", manifest["dependencies"])

    def test_declared_nested_sources_and_mapped_assets_are_staged(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/tools/cargo_publish/fixtures"
        files = root / "publish_files.package"
        expected_paths = [
            "Cargo.toml",
            "README.md",
            "assets/license.txt",
            "src/lib.rs",
            "src/nested/value.rs",
        ]
        self.assertEqual(
            sorted(str(p.relative_to(files)) for p in files.rglob("*") if p.is_file()),
            expected_paths,
        )
        self.assertEqual(
            (files / "src/nested/value.rs").read_text(), "pub const ANSWER: u32 = 42;\n"
        )
        self.assertEqual(
            (files / "assets/license.txt").read_text(), "Fixture license asset.\n"
        )
        contract = json.loads((root / "publish_files.json").read_text())
        self.assertEqual(contract["schema_version"], 1)
        self.assertEqual(contract["target"], "x86_64-unknown-linux-gnu")
        for item in contract["inputs"]:
            self.assertEqual(
                item["sha256"],
                hashlib.sha256((files / item["path"]).read_bytes()).hexdigest(),
            )

    def test_dependency_with_disabled_defaults_remains_disabled(self):
        files = (
            Path(os.environ["TEST_SRCDIR"])
            / "_main/tools/cargo_publish/fixtures/publish_files.package"
        )
        manifest = tomllib.loads((files / "Cargo.toml").read_text())
        self.assertEqual(
            manifest["dependencies"]["sha2"],
            {
                "package": "sha2",
                "version": "=0.11.0",
                "default-features": False,
                "features": [],
            },
        )

    def test_identical_public_macro_inputs_generate_identical_files(self):
        root = Path(os.environ["TEST_SRCDIR"]) / "_main/tools/cargo_publish/fixtures"
        first = root / "publish_files.package"
        second = root / "repeat_files.package"
        self.assertEqual(
            (root / "publish_files.json").read_bytes(),
            (root / "repeat_files.json").read_bytes(),
        )
        for path in first.rglob("*"):
            if path.is_file():
                self.assertEqual(
                    path.read_bytes(), (second / path.relative_to(first)).read_bytes()
                )


if __name__ == "__main__":
    unittest.main()
