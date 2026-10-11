#!/usr/bin/env python3
"""Test offline generation failure diagnostics through its executable seam."""

import copy
import hashlib
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


class GenerationCommandTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        (self.root / "lib.rs").write_text("pub fn answer() -> u32 { 42 }\n")
        (self.root / "README.md").write_text("# Fixture\n")
        (self.root / "Cargo.toml").write_text(
            '[package]\nname="catalog"\nversion="0.1.0"\n'
        )
        (self.root / "Cargo.lock").write_text("version=4\n")
        (self.root / "MODULE.bazel").write_text("")
        self.spec = {
            "schema_version": 1,
            "target": "x86_64-unknown-linux-gnu",
            "package": {
                "name": "fixture",
                "version": "0.1.0",
                "edition": "2024",
                "description": "A fixture",
                "license": "MIT",
                "repository": "https://example.com/repo",
                "rust-version": "1.96",
                "readme": "README.md",
            },
            "library": {
                "label": "//fixture:library",
                "crate_name": "fixture",
                "crate_type": "rlib",
                "edition": "2024",
                "path": "lib.rs",
                "dependencies": [],
            },
            "binaries": [],
            "tests": [],
            "dependency_closure": [],
            "inputs": [
                {"path": path, "source": str(self.root / path)}
                for path in ("lib.rs", "README.md")
            ],
        }

    def run_generation(self, spec=None, suffix=""):
        spec_path = self.root / ("analysis" + suffix + ".json")
        spec_path.write_text(json.dumps(spec if spec is not None else self.spec))
        executable = Path(os.environ["TEST_SRCDIR"]) / "_main" / os.environ["GENERATOR"]
        return subprocess.run(
            [
                str(executable),
                "--spec",
                str(spec_path),
                "--root-manifest",
                str(self.root / "Cargo.toml"),
                "--root-lockfile",
                str(self.root / "Cargo.lock"),
                "--module",
                str(self.root / "MODULE.bazel"),
                "--output",
                str(self.root / ("output" + suffix)),
                "--contract",
                str(self.root / ("contract" + suffix + ".json")),
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_contract_fingerprints_declared_catalog_inputs(self):
        result = self.run_generation()
        self.assertEqual(result.returncode, 0, result.stderr)
        contract = json.loads((self.root / "contract.json").read_text())
        for name, path in [
            ("root_manifest_sha256", "Cargo.toml"),
            ("root_lockfile_sha256", "Cargo.lock"),
            ("module_sha256", "MODULE.bazel"),
        ]:
            self.assertEqual(
                contract["provenance"][name],
                hashlib.sha256((self.root / path).read_bytes()).hexdigest(),
            )

    def test_rejects_malformed_package_version_before_staging(self):
        self.spec["package"]["version"] = "not-a-version"
        result = self.run_generation()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("//fixture:library", result.stderr)
        self.assertIn("version", result.stderr)
        self.assertFalse((self.root / "output").exists())

    def test_rejects_malformed_or_missing_required_metadata(self):
        for key, value in [
            ("version", "1.2.3-alpha.01"),
            ("name", "../bad"),
            ("edition", "2099"),
            ("rust-version", "latest"),
            ("description", ""),
        ]:
            with self.subTest(key=key):
                spec = copy.deepcopy(self.spec)
                spec["package"][key] = value
                result = self.run_generation(spec, key)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("//fixture:library", result.stderr)
                self.assertIn(key, result.stderr)

    def test_missing_escaping_and_conflicting_declared_inputs_fail_offline(self):
        cases = [
            (
                "escaping",
                [{"path": "../outside", "source": str(self.root / "lib.rs")}],
                "escape",
            ),
            (
                "missing",
                [{"path": "lib.rs", "source": str(self.root / "absent")}],
                "missing",
            ),
            (
                "conflicting",
                [self.spec["inputs"][0], self.spec["inputs"][0]],
                "conflicting",
            ),
            (
                "prefix",
                [
                    self.spec["inputs"][0],
                    {
                        "path": "lib.rs/README.md",
                        "source": str(self.root / "README.md"),
                    },
                ],
                "file/directory",
            ),
            ("required", [self.spec["inputs"][0]], "README.md"),
        ]
        for name, inputs, diagnostic in cases:
            with self.subTest(name=name):
                spec = copy.deepcopy(self.spec)
                spec["inputs"] = inputs
                result = self.run_generation(spec, name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)
                self.assertFalse((self.root / ("output" + name)).exists())

    def test_rejects_ambiguous_and_unsupported_dependency_provenance(self):
        upstream = self.root / "upstream.toml"
        upstream.write_text(
            '[package]\nname="anyhow"\nversion="1.0.104"\n[features]\ndefault=["std"]\nstd=[]\n'
        )
        metadata = {
            "label": "@@rules_rs++crate+crates__anyhow-1.0.104//:anyhow",
            "crate_name": "anyhow",
            "crate_type": "rlib",
            "edition": "2021",
            "version": "1.0.104",
            "features": ["default", "std"],
            "external": True,
            "repository": "rules_rs++crate+crates__anyhow-1.0.104",
            "manifest": str(upstream),
            "compiler_settings": {},
            "alias": "friendly",
        }
        locked = (
            '[[package]]\nname="anyhow"\nversion="1.0.104"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="'
            + "a" * 64
            + '"\n'
        )
        cases = [
            ("local", {"external": False}, "local dependency"),
            ("version", {"version": "1.0.103"}, "identity/version"),
            ("missing_metadata", {"manifest": None}, "missing Cargo manifest"),
            ("custom_repository", {"repository": "custom"}, "repository provenance"),
            ("features", {"features": ["unsupported"]}, "configured features"),
            ("git", {}, "crates.io lock provenance"),
            ("ambiguous_lock", {}, "ambiguous"),
            ("missing_catalog", {}, "root Cargo manifest"),
            ("select_flags", {}, "annotation/override"),
            ("select_environment", {}, "annotation/override"),
            ("patch", {}, "annotation/override"),
            ("override", {}, "annotation/override"),
        ]
        for name, changes, diagnostic in cases:
            with self.subTest(name=name):
                spec = copy.deepcopy(self.spec)
                direct = dict(metadata, **changes)
                spec["library"]["dependencies"] = [direct]
                spec["dependency_closure"] = [direct]
                (self.root / "Cargo.toml").write_text(
                    '[dependencies]\nanyhow="1.0.100"\n'
                    if name != "missing_catalog"
                    else ""
                )
                (self.root / "Cargo.lock").write_text(
                    locked.replace(
                        "registry+https://github.com/rust-lang/crates.io-index",
                        "git+https://example.com/repo",
                    )
                    if name == "git"
                    else locked * (2 if name == "ambiguous_lock" else 1)
                )
                annotation = (
                    'crate.annotation_select(crate="anyhow", triples=["x86_64-unknown-linux-gnu"], rustc_flags=["--cfg=custom"])'
                    if name == "select_flags"
                    else 'crate.annotation_select(crate="anyhow", triples=["x86_64-unknown-linux-gnu"], build_script_env={"CUSTOM":"value"})'
                    if name == "select_environment"
                    else 'crate.annotation(crate="anyhow", patches=["//fixture:patch"])'
                    if name == "patch"
                    else 'crate.override(crate="anyhow", path="//fixture")'
                    if name == "override"
                    else ""
                )
                (self.root / "MODULE.bazel").write_text(annotation)
                result = self.run_generation(spec, name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("anyhow", result.stderr)
                self.assertIn(diagnostic, result.stderr)
                self.assertFalse((self.root / ("output" + name)).exists())

        # Ordinary Cargo build scripts and inactive annotations do not require
        # translating a Bazel mutation into the consumer's manifest.
        (self.root / "Cargo.toml").write_text('[dependencies]\nanyhow="1.0.100"\n')
        (self.root / "Cargo.lock").write_text(locked)
        spec = copy.deepcopy(self.spec)
        spec["library"]["dependencies"] = [metadata]
        spec["dependency_closure"] = [metadata]
        for index, annotation in enumerate(
            [
                'crate.annotation(crate="anyhow", gen_build_script="on", crate_features=["std"])',
                'crate.annotation(crate="anyhow", version="1.0.103", patches=["//fixture:patch"])',
                'crate.annotation(crate="anyhow", repositories=["another_hub"], patches=["//fixture:patch"])',
                'crate.annotation_select(crate="anyhow", triples=["aarch64-apple-darwin"], rustc_flags=["--cfg=custom"])',
                'crate.annotation_select(crate="anyhow", triples=["x86_64-unknown-linux-gnu"], crate_features=["std"])',
            ]
        ):
            with self.subTest(annotation=annotation):
                (self.root / "MODULE.bazel").write_text(annotation)
                result = self.run_generation(spec, "supported" + str(index))
                self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
