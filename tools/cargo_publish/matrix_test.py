#!/usr/bin/env python3
"""Validate binary/archive behavior through the public validation command."""

import copy
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

from tools.cargo_publish.report import validate_verified_archive


class MatrixValidationTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.package = self.root / "package"
        self.package.mkdir()
        self.fields = {
            "name": "matrix-fixture",
            "version": "0.1.0",
            "edition": "2024",
            "rust-version": "1.96",
            "description": "Matrix fixture",
            "license": "MIT",
            "repository": "https://example.com/fixture",
            "readme": "README.md",
        }
        self.binary = {
            "label": "//fixture:binary",
            "crate_name": "bazel_binary",
            "name": "fixture-cli",
            "crate_type": "bin",
            "edition": "2024",
            "path": "src/bin/main.rs",
        }
        self.contract = {
            "schema_version": 1,
            "package": self.fields,
            "target": "x86_64-unknown-linux-gnu",
            "library": None,
            "binaries": [self.binary],
            "tests": [],
            "dependencies": [],
            "inputs": [],
        }
        self.manifest = (
            "[package]\n"
            + "\n".join(
                f"{key}={json.dumps(value)}" for key, value in self.fields.items()
            )
            + '\nbuild=false\nautolib=false\nautobins=false\nautotests=false\nautoexamples=false\nautobenches=false\n[[bin]]\nname="fixture-cli"\npath="src/bin/main.rs"\nedition="2024"\n'
        )
        for name, contents in [
            ("README.md", "Fixture\n"),
            ("src/bin/main.rs", "fn main() {}\n"),
        ]:
            path = self.package / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents)
        self.rustc = self.root / "rustc"
        self.rustc.write_text(
            f"#!{sys.executable}\nprint('rustc 1.96.0 (fixture)\\nrelease: 1.96.0\\nhost: x86_64-unknown-linux-gnu')\n"
        )
        self.rustc.chmod(0o700)
        self.cargo = self.root / "cargo"
        self.cargo.write_text(f"#!{sys.executable}\n" + TOOL)
        self.cargo.chmod(0o700)

    def run_validation(self, suffix=""):
        (self.package / "Cargo.toml").write_text(self.manifest)
        self.contract["inputs"] = [
            {
                "path": path.relative_to(self.package).as_posix(),
                "size": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            for path in sorted(self.package.rglob("*"))
            if path.is_file() and path.name != "Cargo.toml"
        ]
        self.contract["manifest_sha256"] = hashlib.sha256(
            self.manifest.encode()
        ).hexdigest()
        contract_path = self.root / "contract.json"
        contract_path.write_text(json.dumps(self.contract))
        output = self.root / ("outputs" + suffix)
        result = subprocess.run(
            [
                os.environ["VERIFIER"],
                "--package",
                str(self.package),
                "--contract",
                str(contract_path),
                "--cargo",
                str(self.cargo),
                "--rustc",
                str(self.rustc),
                "--sysroot",
                str(self.root),
                "--output-root",
                str(output),
                "--offline",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        report_path = next(output.glob("*/report.json"))
        return result, json.loads(report_path.read_text()), report_path

    def test_binary_only_builds_the_mapped_binary_from_the_archive(self):
        result, report, report_path = self.run_validation()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            report["checks"]["locked_builds"]["evidence"]["targets"],
            ["bin:fixture-cli"],
        )
        builds = [
            command
            for command in report["commands"]
            if command["phase"] == "locked_builds"
        ]
        self.assertEqual(len(builds), 1)
        self.assertIn("--locked", builds[0]["argv"])
        self.assertEqual(
            builds[0]["argv"][builds[0]["argv"].index("--bin") + 1], "fixture-cli"
        )
        self.assertNotIn("--lib", builds[0]["argv"])
        extracted = validate_verified_archive(
            report_path, report["archive"]["path"], self.root / "accepted"
        )
        self.assertEqual((extracted / "src/bin/main.rs").read_text(), "fn main() {}\n")

    def select_combined(self):
        self.contract["library"] = {
            "label": "//fixture:library",
            "crate_name": "fixture_library",
            "crate_type": "rlib",
            "edition": "2024",
            "path": "src/lib.rs",
        }
        self.contract["binaries"].append(
            dict(
                self.binary,
                label="//fixture:other",
                crate_name="bazel_other",
                name="fixture-other",
                edition="2021",
                path="src/bin/other.rs",
            )
        )
        self.contract["tests"] = [{"label": "//fixture:unit_test"}]
        self.manifest += '\n[lib]\nname="fixture_library"\npath="src/lib.rs"\n[[bin]]\nname="fixture-other"\npath="src/bin/other.rs"\nedition="2021"\n'
        (self.package / "src/lib.rs").write_text("pub fn answer() -> u32 { 42 }\n")
        (self.package / "src/bin/other.rs").write_text(
            "fn main() { assert_eq!(fixture_library::answer(), 42); }\n"
        )

    def test_combined_archive_requires_the_library_and_every_mapped_binary(self):
        self.select_combined()
        result, report, report_path = self.run_validation()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            report["checks"]["locked_builds"]["evidence"]["targets"],
            ["bin:fixture-cli", "bin:fixture-other", "lib:fixture_library"],
        )
        builds = [
            command
            for command in report["commands"]
            if command["phase"] == "locked_builds"
        ]
        self.assertEqual(len(builds), 3)
        self.assertTrue(all("--locked" in command["argv"] for command in builds))
        extracted = validate_verified_archive(
            report_path, report["archive"]["path"], self.root / "accepted"
        )
        self.assertTrue((extracted / "src/bin/other.rs").is_file())
        self.assertFalse(
            any("test" in command["argv"] for command in report["commands"])
        )
        incomplete = copy.deepcopy(report)
        omitted = next(
            command["id"] for command in builds if "fixture-other" in command["argv"]
        )
        incomplete["checks"]["locked_builds"]["evidence"]["command_ids"].remove(omitted)
        bad_report = self.root / "incomplete.json"
        bad_report.write_text(json.dumps(incomplete))
        with self.assertRaisesRegex(ValueError, "every selected"):
            validate_verified_archive(
                bad_report, report["archive"]["path"], self.root / "incomplete"
            )

    def test_failing_second_binary_retains_failed_build_evidence(self):
        self.select_combined()
        self.cargo.write_text(
            self.cargo.read_text().replace(
                "elif 'build' in args:",
                "elif 'build' in args:\n    if '--bin' in args and args[args.index('--bin') + 1] == 'fixture-other':\n        sys.stderr.write('fixture-other compilation failed')\n        sys.exit(14)",
            )
        )
        result, report, _ = self.run_validation()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(report["completed"])
        self.assertEqual(report["diagnostics"]["phase"], "locked_builds")
        self.assertIn("fixture-other", report["diagnostics"]["message"])
        self.assertEqual(report["commands"][-1]["returncode"], 14)
        self.assertTrue(Path(report["commands"][-1]["log"]).is_file())

    def test_duplicate_binary_manifest_does_not_receive_success(self):
        self.manifest += (
            '\n[[bin]]\nname="fixture-cli"\npath="src/bin/main.rs"\nedition="2024"\n'
        )
        result, report, _ = self.run_validation()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(report["completed"])
        self.assertEqual(report["diagnostics"]["phase"], "generated_manifest")

    def test_requested_dependency_metadata_preserves_kind_alias_requirement_defaults_and_features(
        self,
    ):
        self.manifest += '\n[dependencies]\nerrors={package="anyhow",version="=1.0.0",default-features=false,features=["std"]}\n'
        self.contract["dependencies"] = [
            {
                "alias": "errors",
                "kind": "normal",
                "package": "anyhow",
                "version": "1.0.0",
                "default_features": False,
                "features": ["std"],
            }
        ]
        metadata = {
            "packages": [
                {
                    "id": "root",
                    "name": "matrix-fixture",
                    "version": "0.1.0",
                    "source": None,
                    "targets": [
                        {
                            "name": "fixture-cli",
                            "kind": ["bin"],
                            "edition": "2024",
                            "src_path": "src/bin/main.rs",
                        }
                    ],
                    "dependencies": [
                        {
                            "name": "anyhow",
                            "source": "registry+https://github.com/rust-lang/crates.io-index",
                            "req": "=1.0.0",
                            "kind": None,
                            "rename": "errors",
                            "optional": False,
                            "uses_default_features": False,
                            "features": ["std"],
                            "target": None,
                        }
                    ],
                },
                {
                    "id": "direct",
                    "name": "anyhow",
                    "version": "1.0.0",
                    "source": "registry+https://github.com/rust-lang/crates.io-index",
                },
            ],
            "resolve": {
                "root": "root",
                "nodes": [
                    {
                        "id": "root",
                        "deps": [
                            {
                                "name": "errors",
                                "pkg": "direct",
                                "dep_kinds": [{"kind": None, "target": None}],
                            }
                        ],
                        "features": [],
                    },
                    {"id": "direct", "deps": [], "features": ["std"]},
                ],
            },
        }
        self.manifest += '\n[dev-dependencies]\nerrors={package="anyhow",version="=1.0.0",default-features=true,features=["std"]}\n'
        self.contract["dependencies"].append(
            dict(self.contract["dependencies"][0], kind="dev", default_features=True)
        )
        metadata["packages"][0]["dependencies"].append(
            dict(
                metadata["packages"][0]["dependencies"][0],
                kind="dev",
                uses_default_features=True,
            )
        )
        metadata["resolve"]["nodes"][0]["deps"][0]["dep_kinds"].append(
            {"kind": "dev", "target": None}
        )
        (self.root / "metadata.json").write_text(json.dumps(metadata))
        result, report, _ = self.run_validation("faithful")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            {dependency["kind"] for dependency in report["dependencies"]["requested"]},
            {"normal", "dev"},
        )
        for index, change in enumerate(
            [
                {"uses_default_features": True},
                {"features": []},
                {"req": "^1.0.0"},
                {"rename": "changed"},
                {"kind": "dev"},
                {"name": "different"},
                {"optional": True},
                {"target": "cfg(unix)"},
                {"source": "registry+https://example.com/index"},
            ]
        ):
            with self.subTest(metadata_declaration=change):
                changed = copy.deepcopy(metadata)
                changed["packages"][0]["dependencies"][0].update(change)
                (self.root / "metadata.json").write_text(json.dumps(changed))
                result, report, _ = self.run_validation(str(index))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["diagnostics"]["phase"], "direct_dependencies")
                self.assertIn("errors", report["diagnostics"]["message"])
                self.assertFalse(report["completed"])

    def test_missing_extra_duplicate_or_changed_binary_metadata_fails(self):
        target = {
            "name": "fixture-cli",
            "kind": ["bin"],
            "edition": "2024",
            "src_path": "src/bin/main.rs",
        }
        for index, targets in enumerate(
            [
                [],
                [target, dict(target, name="extra")],
                [target, target],
                [dict(target, name="changed")],
                [dict(target, edition="2021")],
                [dict(target, src_path="src/missing.rs")],
                [dict(target, kind=["lib"])],
            ]
        ):
            with self.subTest(targets=targets):
                metadata = {
                    "packages": [
                        {
                            "id": "root",
                            "name": "matrix-fixture",
                            "version": "0.1.0",
                            "targets": targets,
                            "dependencies": [],
                        }
                    ],
                    "resolve": {
                        "root": "root",
                        "nodes": [{"id": "root", "features": [], "deps": []}],
                    },
                }
                (self.root / "metadata.json").write_text(json.dumps(metadata))
                result, report, _ = self.run_validation(str(index))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["diagnostics"]["phase"], "direct_dependencies")
                self.assertIn("selected targets", report["diagnostics"]["message"])
                self.assertFalse(report["completed"])

    def test_normalized_binary_targets_must_preserve_exact_selection(self):
        binary = '[[bin]]\nname="fixture-cli"\npath="src/bin/main.rs"\nedition="2024"\n'
        for index, changed in enumerate(
            [
                "",
                binary + binary,
                binary.replace("fixture-cli", "extra") + binary,
                binary.replace("fixture-cli", "changed"),
                binary.replace("main.rs", "missing.rs"),
                binary.replace('edition="2024"', 'edition="2021"'),
            ]
        ):
            with self.subTest(normalized_binary=changed):
                normalized = self.manifest.replace(binary, changed)
                tool = TOOL.replace(
                    "tar.add(path, arcname='matrix-fixture-0.1.0/' + path.relative_to(manifest.parent).as_posix())",
                    "contents = "
                    + repr(normalized.encode())
                    + " if path.name == 'Cargo.toml' else path.read_bytes()\n                info = tarfile.TarInfo('matrix-fixture-0.1.0/' + path.relative_to(manifest.parent).as_posix())\n                info.size = len(contents)\n                tar.addfile(info, io.BytesIO(contents))",
                )
                self.cargo.write_text(f"#!{sys.executable}\n" + tool)
                result, report, _ = self.run_validation(str(index))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(report["diagnostics"]["phase"], "normalized_manifest")
                self.assertIn("selected binary", report["diagnostics"]["message"])
                self.assertFalse(report["completed"])

    def test_locked_build_cannot_change_the_packaged_lockfile(self):
        self.cargo.write_text(
            self.cargo.read_text().replace(
                "elif 'build' in args:",
                "elif 'build' in args:\n    (manifest.parent / 'Cargo.lock').write_text('version=4\\n# changed')",
            )
        )
        result, report, _ = self.run_validation()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(report["completed"])
        self.assertEqual(report["diagnostics"]["phase"], "locked_builds")
        self.assertIn("Cargo.lock changed", report["diagnostics"]["message"])
        self.assertTrue(Path(report["commands"][-1]["log"]).is_file())

    def test_public_archive_consumer_rejects_file_directory_prefix_conflicts(self):
        result, report, _ = self.run_validation()
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = self.root / "conflict.crate"
        with (
            tarfile.open(report["archive"]["path"]) as original,
            tarfile.open(archive, "w:gz") as replacement,
        ):
            for member in original.getmembers():
                replacement.addfile(
                    member, original.extractfile(member) if member.isfile() else None
                )
            member = tarfile.TarInfo("matrix-fixture-0.1.0/src")
            member.size = 4
            replacement.addfile(member, io.BytesIO(b"evil"))
        report["inputs"].append(
            {"path": "src", "size": 4, "sha256": hashlib.sha256(b"evil").hexdigest()}
        )
        for phase in ["input_fidelity", "archive_fidelity"]:
            report["checks"][phase]["evidence"]["inputs"] = report["inputs"]
        report["archive"].update(
            sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
            size=archive.stat().st_size,
        )
        changed = self.root / "conflict.json"
        changed.write_text(json.dumps(report))
        with self.assertRaisesRegex(ValueError, "conflicting archive file/directory"):
            validate_verified_archive(changed, archive, self.root / "rejected-conflict")
        self.assertFalse((self.root / "rejected-conflict/extracted").exists())


TOOL = r"""
import io, json, os, sys, tarfile, tomllib
from pathlib import Path
args = sys.argv[1:]
if args == ['-V']:
    print('cargo 1.96.0 (fixture)')
    sys.exit(0)
manifest = Path(args[args.index('--manifest-path') + 1])
data = tomllib.loads(manifest.read_text())
if 'package' in args:
    if '--locked' in args or '--no-verify' in args:
        sys.exit('first package must resolve and verify')
    archive = Path(os.environ['CARGO_TARGET_DIR']) / 'package/matrix-fixture-0.1.0.crate'
    archive.parent.mkdir(parents=True)
    with tarfile.open(archive, 'w:gz') as tar:
        for path in sorted(manifest.parent.rglob('*')):
            if path.is_file():
                tar.add(path, arcname='matrix-fixture-0.1.0/' + path.relative_to(manifest.parent).as_posix())
        for name, contents in {'Cargo.toml.orig': manifest.read_bytes(), 'Cargo.lock': b'version=4\n'}.items():
            info = tarfile.TarInfo('matrix-fixture-0.1.0/' + name)
            info.size = len(contents)
            tar.addfile(info, io.BytesIO(contents))
elif 'metadata' in args:
    if '--locked' not in args:
        sys.exit('metadata must use packaged lock')
    override = Path(__file__).parent / 'metadata.json'
    if override.is_file():
        metadata = json.loads(override.read_text())
        for target in metadata['packages'][0]['targets']:
            target['src_path'] = str(manifest.parent / target['src_path'])
        print(json.dumps(metadata))
        sys.exit(0)
    targets = []
    if 'lib' in data:
        lib = data['lib']
        targets.append({'name': lib['name'], 'kind': ['lib'], 'edition': lib.get('edition', data['package']['edition']), 'src_path': str(manifest.parent / lib['path'])})
    for binary in data.get('bin', []):
        targets.append({'name': binary['name'], 'kind': ['bin'], 'edition': binary.get('edition', data['package']['edition']), 'src_path': str(manifest.parent / binary['path'])})
    print(json.dumps({'packages': [{'id': 'root', 'name': 'matrix-fixture', 'version': '0.1.0', 'source': None, 'targets': targets, 'dependencies': []}], 'resolve': {'root': 'root', 'nodes': [{'id': 'root', 'deps': [], 'features': []}]}}))
elif 'build' in args:
    if '--locked' not in args or 'extracted' not in str(manifest):
        sys.exit('build must use extracted packaged lock')
    selected = []
    if '--lib' in args:
        if 'lib' not in data:
            sys.exit('selected library missing')
        selected.append('lib')
    for index, argument in enumerate(args[:-1]):
        if argument == '--bin':
            if args[index + 1] not in [binary['name'] for binary in data.get('bin', [])]:
                sys.exit('selected binary missing')
            selected.append(args[index + 1])
    if not selected:
        sys.exit('explicit target selection required')
else:
    sys.exit('unexpected Cargo command')
"""

if __name__ == "__main__":
    unittest.main()
