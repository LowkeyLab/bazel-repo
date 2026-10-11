#!/usr/bin/env python3
"""Publishing runtime behavior through the public command and report interfaces."""

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

from tools.cargo_publish.report import validate_report, validate_verified_archive


class RuntimeCommandTest(unittest.TestCase):
    def test_changed_declared_input_is_not_packaged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "package"
            package.mkdir()
            manifest = '[package]\nname="fixture"\nversion="0.1.0"\n'
            (package / "Cargo.toml").write_text(manifest)
            (package / "lib.rs").write_text("pub fn changed() {}\n")
            contract = {
                "schema_version": 1,
                "package": {"name": "fixture", "version": "0.1.0"},
                "target": "x86_64-unknown-linux-gnu",
                "library": {"label": "//fixture:lib", "path": "lib.rs"},
                "binaries": [],
                "dependencies": [],
                "inputs": [{"path": "lib.rs", "size": 1, "sha256": "0" * 64}],
                "manifest_sha256": hashlib.sha256(manifest.encode()).hexdigest(),
            }
            contract_path = root / "contract.json"
            contract_path.write_text(json.dumps(contract))
            result = subprocess.run(
                [
                    os.environ["VERIFIER"],
                    "--package",
                    str(package),
                    "--contract",
                    str(contract_path),
                    "--cargo",
                    "/missing/cargo",
                    "--rustc",
                    "/missing/rustc",
                    "--sysroot",
                    "/missing/sysroot",
                    "--output-root",
                    str(root / "outputs"),
                    "--offline",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("lib.rs", result.stderr)
            reports = list((root / "outputs").glob("*/report.json"))
            self.assertEqual(len(reports), 1)
            report = json.loads(reports[0].read_text())
            self.assertEqual(report["status"], "failure")
            self.assertFalse(report["completed"])
            self.assertEqual(report["diagnostics"]["phase"], "input_fidelity")


class RuntimeFailureTest(unittest.TestCase):
    def test_missing_pinned_compiler_records_failed_toolchain_phase(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "package"
            package.mkdir()
            manifest = '[package]\nname="fixture"\nversion="0.1.0"\n'
            source = b"pub fn fixture() {}\n"
            (package / "Cargo.toml").write_text(manifest)
            (package / "lib.rs").write_bytes(source)
            contract = {
                "schema_version": 1,
                "package": {"name": "fixture", "version": "0.1.0"},
                "target": "x86_64-unknown-linux-gnu",
                "library": {"label": "//fixture:lib", "path": "lib.rs"},
                "binaries": [],
                "dependencies": [],
                "inputs": [
                    {
                        "path": "lib.rs",
                        "size": len(source),
                        "sha256": hashlib.sha256(source).hexdigest(),
                    }
                ],
                "manifest_sha256": hashlib.sha256(manifest.encode()).hexdigest(),
            }
            contract_path = root / "contract.json"
            contract_path.write_text(json.dumps(contract))
            result = subprocess.run(
                [
                    os.environ["VERIFIER"],
                    "--package",
                    str(package),
                    "--contract",
                    str(contract_path),
                    "--cargo",
                    "/missing/cargo",
                    "--rustc",
                    "/missing/rustc",
                    "--sysroot",
                    "/missing/sysroot",
                    "--output-root",
                    str(root / "outputs"),
                    "--offline",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            report = json.loads(
                next((root / "outputs").glob("*/report.json")).read_text()
            )
            self.assertEqual(report["diagnostics"]["phase"], "toolchain")
            self.assertIn("rustc", report["diagnostics"]["message"])
            self.assertFalse(report["completed"])


class PublishingPipelineTest(unittest.TestCase):
    def test_success_report_covers_locked_extracted_library_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            package = root / "package"
            package.mkdir()
            fields = {
                "name": "fixture",
                "version": "0.1.0",
                "edition": "2024",
                "rust-version": "1.96",
                "description": "Fixture library",
                "license": "MIT",
                "repository": "https://example.com/fixture",
                "readme": "README.md",
            }
            manifest = (
                "[package]\n"
                + "\n".join(
                    f"{key}={json.dumps(value)}" for key, value in fields.items()
                )
                + '\nbuild=false\nautolib=false\nautobins=false\nautotests=false\nautoexamples=false\nautobenches=false\n[lib]\nname="fixture_library"\npath="lib.rs"\n'
            )
            (package / "Cargo.toml").write_text(manifest)
            (package / "lib.rs").write_text("pub fn fixture() {}\n")
            (package / "README.md").write_text("Fixture\n")
            inputs = [
                {
                    "path": name,
                    "size": (package / name).stat().st_size,
                    "sha256": hashlib.sha256((package / name).read_bytes()).hexdigest(),
                }
                for name in ("lib.rs", "README.md")
            ]
            contract = {
                "schema_version": 1,
                "package": fields,
                "target": "x86_64-unknown-linux-gnu",
                "library": {
                    "label": "//fixture:lib",
                    "crate_name": "fixture_library",
                    "crate_type": "rlib",
                    "edition": "2024",
                    "path": "lib.rs",
                },
                "binaries": [],
                "tests": [],
                "dependencies": [],
                "inputs": inputs,
                "manifest_sha256": hashlib.sha256(manifest.encode()).hexdigest(),
                "provenance": {"fixture": "declared"},
            }
            contract_path = root / "contract.json"
            contract_path.write_text(json.dumps(contract))
            rustc = root / "rustc"
            rustc.write_text(
                f"#!{sys.executable}\nprint('rustc 1.96.0 (fixture)\\nrelease: 1.96.0\\nhost: x86_64-unknown-linux-gnu')\n"
            )
            rustc.chmod(0o700)
            cargo = root / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                + r"""
import io, json, os, sys, tarfile
from pathlib import Path
args=sys.argv[1:]
if args==["-V"]:
 print("cargo 1.96.0 (fixture)")
 sys.exit(0)
manifest=Path(args[args.index("--manifest-path")+1])
if "package" in args:
 if "--locked" in args or "--no-verify" in args:
  sys.exit("first package must resolve and verify")
 archive=Path(os.environ["CARGO_TARGET_DIR"])/"package/fixture-0.1.0.crate"
 archive.parent.mkdir(parents=True)
 with tarfile.open(archive,"w:gz") as tar:
  for path in sorted(manifest.parent.iterdir()):
   if path.is_file():
    tar.add(path,arcname="fixture-0.1.0/"+path.name)
  for name,data in {"Cargo.toml.orig":manifest.read_bytes(),"Cargo.lock":b'version = 4\n[[package]]\nname="fixture"\nversion="0.1.0"\n'}.items():
   info=tarfile.TarInfo("fixture-0.1.0/"+name);info.size=len(data);tar.addfile(info,io.BytesIO(data))
elif "metadata" in args:
 if "--locked" not in args:sys.exit("metadata requires packaged lock")
 print(json.dumps({"packages":[{"id":"fixture", "name":"fixture", "version":"0.1.0", "source":None,
 "targets":[{"name":"fixture_library", "kind":["lib"], "crate_types":["lib"], "edition":"2024", "src_path":str(manifest.parent/"lib.rs")}], "dependencies":[]}],
 "resolve":{"root":"fixture", "nodes":[{"id":"fixture", "deps":[], "features":[]}]}}))
elif "build" in args:
 if "--locked" not in args or "--lib" not in args or "extracted" not in str(manifest):sys.exit("build must cover extracted locked library")
else:sys.exit("unexpected Cargo command")
"""
            )
            cargo.chmod(0o700)
            result = subprocess.run(
                [
                    os.environ["VERIFIER"],
                    "--package",
                    str(package),
                    "--contract",
                    str(contract_path),
                    "--cargo",
                    str(cargo),
                    "--rustc",
                    str(rustc),
                    "--sysroot",
                    str(root),
                    "--output-root",
                    str(root / "outputs"),
                    "--offline",
                ],
                capture_output=True,
                text=True,
                check=False,
                env=dict(
                    os.environ,
                    CARGO_HOME="ambient-setting",
                    RUSTFLAGS="ambient-setting",
                    RUSTC_WRAPPER="ambient-setting",
                    CC="ambient-setting",
                    CFLAGS="ambient-setting",
                ),
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(
                next((root / "outputs").glob("*/report.json")).read_text()
            )
            self.assertEqual(report["status"], "success")
            self.assertTrue(report["completed"])
            self.assertEqual(
                report["checks"]["locked_builds"]["evidence"]["targets"],
                ["lib:fixture_library"],
            )
            self.assertEqual(
                report["archive"]["sha256"],
                hashlib.sha256(
                    Path(report["archive"]["path"]).read_bytes()
                ).hexdigest(),
            )
            self.assertFalse((package / "Cargo.lock").exists())
            extracted = validate_verified_archive(
                next((root / "outputs").glob("*/report.json")),
                report["archive"]["path"],
                root / "accepted",
            )
            self.assertEqual(
                (extracted / "lib.rs").read_text(), "pub fn fixture() {}\n"
            )
            for key in (
                "CARGO_HOME",
                "RUSTFLAGS",
                "RUSTC_WRAPPER",
                "CC",
                "CFLAGS",
                "LD_PRELOAD",
            ):
                self.assertNotEqual(
                    report["configuration"]["environment"].get(key), "ambient-setting"
                )
            fabricated = copy.deepcopy(report)
            fabricated["checks"]["input_fidelity"]["evidence"] = {"fabricated": True}
            with self.assertRaisesRegex(ValueError, "input_fidelity"):
                validate_report(fabricated)
            for phase in (
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
            ):
                with self.subTest(incomplete_check=phase):
                    incomplete = copy.deepcopy(report)
                    incomplete["checks"][phase]["completed"] = False
                    with self.assertRaises(ValueError):
                        validate_report(incomplete)
            for changes in (
                {"schema_version": 2},
                {"report_kind": "unknown"},
                {"status": "incomplete"},
                {"completed": False},
                {"checks": {}},
                {"commands": []},
            ):
                with self.subTest(report_change=changes):
                    invalid = dict(report, **changes)
                    with self.assertRaises(ValueError):
                        validate_report(invalid)
            no_targets = copy.deepcopy(report)
            no_targets["checks"]["locked_builds"]["evidence"]["targets"] = []
            with self.assertRaises(ValueError):
                validate_report(no_targets)
            for label, name, kind in (
                ("traversal", "fixture-0.1.0/../escape", "file"),
                ("wrong-root", "wrong-0.1.0/lib.rs", "file"),
                ("backslash", "fixture-0.1.0/evil\\name", "file"),
                ("noncanonical", "fixture-0.1.0//extra", "file"),
                ("absolute", "/fixture-0.1.0/lib.rs", "file"),
                ("unexpected", "fixture-0.1.0/extra", "file"),
                ("duplicate", "fixture-0.1.0/lib.rs", "file"),
                ("symlink", "fixture-0.1.0/link", "symlink"),
                ("hardlink", "fixture-0.1.0/link", "hardlink"),
                ("special", "fixture-0.1.0/device", "fifo"),
                ("directory", "fixture-0.1.0/empty", "directory"),
            ):
                with self.subTest(archive_policy=label):
                    modified = root / (label + ".crate")
                    with (
                        tarfile.open(report["archive"]["path"]) as original,
                        tarfile.open(modified, "w:gz") as replacement,
                    ):
                        for member in original.getmembers():
                            replacement.addfile(
                                member,
                                original.extractfile(member)
                                if member.isfile()
                                else None,
                            )
                        member = tarfile.TarInfo(name)
                        if kind == "file":
                            member.size = 4
                            replacement.addfile(member, io.BytesIO(b"evil"))
                        else:
                            member.type = {
                                "symlink": tarfile.SYMTYPE,
                                "hardlink": tarfile.LNKTYPE,
                                "fifo": tarfile.FIFOTYPE,
                                "directory": tarfile.DIRTYPE,
                            }[kind]
                            member.linkname = "lib.rs"
                            replacement.addfile(member)
                    changed = copy.deepcopy(report)
                    changed["archive"].update(
                        sha256=hashlib.sha256(modified.read_bytes()).hexdigest(),
                        size=modified.stat().st_size,
                    )
                    changed_path = root / (label + ".json")
                    changed_path.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):
                        validate_verified_archive(
                            changed_path, modified, root / ("rejected-" + label)
                        )
                    self.assertFalse(
                        (root / ("rejected-" + label) / "extracted").exists()
                    )
            for label, removed, tampered in (
                ("missing-source", "lib.rs", None),
                ("missing-lock", "Cargo.lock", None),
                ("missing-manifest", "Cargo.toml", None),
                ("missing-original", "Cargo.toml.orig", None),
                ("changed-source", None, "lib.rs"),
                ("changed-asset", None, "README.md"),
            ):
                with self.subTest(archive_fidelity=label):
                    modified = root / (label + ".crate")
                    with (
                        tarfile.open(report["archive"]["path"]) as original,
                        tarfile.open(modified, "w:gz") as replacement,
                    ):
                        for member in original.getmembers():
                            relative = member.name.split("/", 1)[1]
                            if relative == removed:
                                continue
                            if relative == tampered:
                                member.size = 4
                                replacement.addfile(member, io.BytesIO(b"evil"))
                            else:
                                replacement.addfile(
                                    member,
                                    original.extractfile(member)
                                    if member.isfile()
                                    else None,
                                )
                    changed = copy.deepcopy(report)
                    changed["archive"].update(
                        sha256=hashlib.sha256(modified.read_bytes()).hexdigest(),
                        size=modified.stat().st_size,
                    )
                    changed_path = root / (label + ".json")
                    changed_path.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):
                        validate_verified_archive(
                            changed_path, modified, root / ("rejected-" + label)
                        )
            for phase, change in (
                ("cargo_package", {"verification_enabled": False}),
                ("cargo_package", {"archive": "/wrong.crate"}),
            ):
                with self.subTest(invalid_phase_evidence=change):
                    invalid = copy.deepcopy(report)
                    invalid["checks"][phase]["evidence"].update(change)
                    with self.assertRaises(ValueError):
                        validate_report(invalid)
            for phase, flag in (
                ("cargo_package", "--no-verify"),
                ("locked_metadata", "--locked"),
                ("locked_builds", "--locked"),
            ):
                with self.subTest(invalid_command_flag=phase):
                    invalid = copy.deepcopy(report)
                    command = next(
                        item for item in invalid["commands"] if item["phase"] == phase
                    )
                    if phase == "cargo_package":
                        command["argv"].append(flag)
                    else:
                        command["argv"].remove(flag)
                    with self.assertRaises(ValueError):
                        validate_report(invalid)
            with self.assertRaisesRegex(ValueError, "identity"):
                validate_report(report, {"name": "another-package", "version": "0.1.0"})
            for label, field, value in (
                ("size", "size", report["archive"]["size"] + 1),
                ("inventory", "inventory", []),
            ):
                with self.subTest(report_archive_mismatch=label):
                    invalid = copy.deepcopy(report)
                    invalid["archive"][field] = value
                    invalid["checks"]["archive_policy"]["evidence"]["inventory"] = (
                        invalid["archive"]["inventory"]
                    )
                    invalid_path = root / ("wrong-" + label + ".json")
                    invalid_path.write_text(json.dumps(invalid))
                    with self.assertRaises(ValueError):
                        validate_verified_archive(
                            invalid_path,
                            report["archive"]["path"],
                            root / ("wrong-" + label),
                        )
            wrong_hash = copy.deepcopy(report)
            wrong_hash["archive"]["sha256"] = "0" * 64
            wrong_hash_path = root / "wrong-hash.json"
            wrong_hash_path.write_text(json.dumps(wrong_hash))
            with self.assertRaisesRegex(ValueError, "SHA256"):
                validate_verified_archive(
                    wrong_hash_path, report["archive"]["path"], root / "wrong-hash"
                )

            original_tool = cargo.read_text()
            for command, phase, response in (
                (
                    "package",
                    "cargo_package",
                    "sys.stderr.write('offline registry cache is missing fixture dependency'); sys.exit(12)",
                ),
                (
                    "metadata",
                    "locked_metadata",
                    "sys.stderr.write('metadata unavailable'); sys.exit(13)",
                ),
                ("metadata", "locked_metadata", "print('not JSON'); sys.exit(0)"),
                (
                    "build",
                    "locked_builds",
                    "sys.stderr.write('library build failed'); sys.exit(14)",
                ),
            ):
                with self.subTest(failed_runtime_command=command, response=response):
                    lines = original_tool.splitlines(keepends=True)
                    injection = (
                        "import sys\nif "
                        + repr(command)
                        + " in sys.argv:\n "
                        + response
                        + "\n"
                    )
                    cargo.write_text(lines[0] + injection + "".join(lines[1:]))
                    failed_output = root / (
                        "failure-"
                        + command
                        + "-"
                        + str(len(list(root.glob("failure-*"))))
                    )
                    failure = subprocess.run(
                        [
                            os.environ["VERIFIER"],
                            "--package",
                            str(package),
                            "--contract",
                            str(contract_path),
                            "--cargo",
                            str(cargo),
                            "--rustc",
                            str(rustc),
                            "--sysroot",
                            str(root),
                            "--output-root",
                            str(failed_output),
                            "--offline",
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertNotEqual(failure.returncode, 0)
                    failed_report = json.loads(
                        next(failed_output.glob("*/report.json")).read_text()
                    )
                    self.assertEqual(failed_report["status"], "failure")
                    self.assertFalse(failed_report["completed"])
                    self.assertEqual(failed_report["diagnostics"]["phase"], phase)
                    self.assertTrue(
                        all(
                            Path(command["log"]).is_file()
                            for command in failed_report["commands"]
                        )
                    )
            cargo.write_text(original_tool)
            for label, modified_tool, expected_phase in (
                (
                    "normalized-edition",
                    original_tool.replace(
                        'tar.add(path,arcname="fixture-0.1.0/"+path.name)',
                        'data=path.read_bytes().replace(b\'edition="2024"\',b\'edition="2021"\') if path.name=="Cargo.toml" else path.read_bytes(); info=tarfile.TarInfo("fixture-0.1.0/"+path.name); info.size=len(data); tar.addfile(info,io.BytesIO(data))',
                    ),
                    "normalized_manifest",
                ),
                (
                    "original-manifest",
                    original_tool.replace(
                        '"Cargo.toml.orig":manifest.read_bytes()',
                        '"Cargo.toml.orig":manifest.read_bytes()+b"changed"',
                    ),
                    "original_manifest",
                ),
                (
                    "metadata-identity",
                    original_tool.replace('"name":"fixture"', '"name":"wrong"'),
                    "direct_dependencies",
                ),
            ):
                with self.subTest(changed_package_evidence=label):
                    cargo.write_text(modified_tool)
                    failed_output = root / label
                    failure = subprocess.run(
                        [
                            os.environ["VERIFIER"],
                            "--package",
                            str(package),
                            "--contract",
                            str(contract_path),
                            "--cargo",
                            str(cargo),
                            "--rustc",
                            str(rustc),
                            "--sysroot",
                            str(root),
                            "--output-root",
                            str(failed_output),
                            "--offline",
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertNotEqual(failure.returncode, 0)
                    failed_report = json.loads(
                        next(failed_output.glob("*/report.json")).read_text()
                    )
                    self.assertEqual(
                        failed_report["diagnostics"]["phase"],
                        expected_phase,
                        failure.stderr,
                    )
                    self.assertFalse(failed_report["completed"])
            cargo.write_text(original_tool)
            original_compiler = rustc.read_text()
            for label, replacement in (
                ("version", original_compiler.replace("1.96.0", "1.95.0")),
                (
                    "host",
                    original_compiler.replace(
                        "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"
                    ),
                ),
            ):
                with self.subTest(unsupported_toolchain=label):
                    rustc.write_text(replacement)
                    failed_output = root / ("toolchain-" + label)
                    failure = subprocess.run(
                        [
                            os.environ["VERIFIER"],
                            "--package",
                            str(package),
                            "--contract",
                            str(contract_path),
                            "--cargo",
                            str(cargo),
                            "--rustc",
                            str(rustc),
                            "--sysroot",
                            str(root),
                            "--output-root",
                            str(failed_output),
                            "--offline",
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertNotEqual(failure.returncode, 0)
                    failed_report = json.loads(
                        next(failed_output.glob("*/report.json")).read_text()
                    )
                    self.assertEqual(failed_report["diagnostics"]["phase"], "toolchain")
                    self.assertIn("unsupported", failure.stderr)
            rustc.write_text(original_compiler)
            dependency_manifest = (
                manifest
                + '\n[dependencies]\nrenamed_dep={package="anyhow",version="=1.0.0",default-features=false,features=[]}\n'
            )
            dependency_contract = copy.deepcopy(contract)
            dependency_contract["dependencies"] = [
                {
                    "alias": "renamed_dep",
                    "kind": "normal",
                    "package": "anyhow",
                    "version": "1.0.0",
                    "default_features": False,
                    "features": [],
                }
            ]
            dependency_contract["manifest_sha256"] = hashlib.sha256(
                dependency_manifest.encode()
            ).hexdigest()
            (package / "Cargo.toml").write_text(dependency_manifest)
            contract_path.write_text(json.dumps(dependency_contract))
            for label, alias, version, kind, source, target in (
                (
                    "direct-version",
                    "renamed_dep",
                    "2.0.0",
                    None,
                    "registry+https://github.com/rust-lang/crates.io-index",
                    None,
                ),
                (
                    "direct-alias",
                    "wrong_alias",
                    "1.0.0",
                    None,
                    "registry+https://github.com/rust-lang/crates.io-index",
                    None,
                ),
                (
                    "direct-kind",
                    "renamed_dep",
                    "1.0.0",
                    "dev",
                    "registry+https://github.com/rust-lang/crates.io-index",
                    None,
                ),
                (
                    "direct-source",
                    "renamed_dep",
                    "1.0.0",
                    None,
                    "git+https://example.com/source",
                    None,
                ),
                (
                    "direct-condition",
                    "renamed_dep",
                    "1.0.0",
                    None,
                    "registry+https://github.com/rust-lang/crates.io-index",
                    "cfg(unix)",
                ),
            ):
                with self.subTest(resolved_direct_dependency=label):
                    metadata = {
                        "packages": [
                            {
                                "id": "fixture",
                                "name": "fixture",
                                "version": "0.1.0",
                                "source": None,
                                "targets": [
                                    {
                                        "name": "fixture_library",
                                        "kind": ["lib"],
                                        "edition": "2024",
                                        "src_path": "",
                                    }
                                ],
                            },
                            {
                                "id": "direct",
                                "name": "anyhow",
                                "version": version,
                                "source": source,
                            },
                            {
                                "id": "transitive",
                                "name": "anyhow",
                                "version": "1.0.0",
                                "source": "registry+https://github.com/rust-lang/crates.io-index",
                            },
                        ],
                        "resolve": {
                            "root": "fixture",
                            "nodes": [
                                {
                                    "id": "fixture",
                                    "deps": [
                                        {
                                            "name": alias,
                                            "pkg": "direct",
                                            "dep_kinds": [
                                                {"kind": kind, "target": target}
                                            ],
                                        }
                                    ],
                                    "features": [],
                                },
                                {"id": "direct", "deps": [], "features": []},
                                {"id": "transitive", "deps": [], "features": []},
                            ],
                        },
                    }
                    lines = original_tool.splitlines(keepends=True)
                    injection = (
                        "import sys, json\nfrom pathlib import Path\nif 'metadata' in sys.argv:\n metadata=json.loads("
                        + repr(json.dumps(metadata))
                        + ")\n metadata['packages'][0]['targets'][0]['src_path']=str(Path(sys.argv[sys.argv.index('--manifest-path')+1]).parent/'lib.rs')\n print(json.dumps(metadata))\n sys.exit(0)\n"
                    )
                    cargo.write_text(lines[0] + injection + "".join(lines[1:]))
                    failed_output = root / label
                    failure = subprocess.run(
                        [
                            os.environ["VERIFIER"],
                            "--package",
                            str(package),
                            "--contract",
                            str(contract_path),
                            "--cargo",
                            str(cargo),
                            "--rustc",
                            str(rustc),
                            "--sysroot",
                            str(root),
                            "--output-root",
                            str(failed_output),
                            "--offline",
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertNotEqual(failure.returncode, 0)
                    failed_report = json.loads(
                        next(failed_output.glob("*/report.json")).read_text()
                    )
                    self.assertEqual(
                        failed_report["diagnostics"]["phase"],
                        "direct_dependencies",
                        failure.stderr,
                    )
                    self.assertFalse(failed_report["completed"])
            (package / "Cargo.toml").write_text(manifest)
            contract_path.write_text(json.dumps(contract))
            cargo.write_text(original_tool)
            for filename in ("config", "config.toml"):
                with self.subTest(ambient_configuration=filename):
                    parent = root / ("ambient-" + filename)
                    (parent / ".cargo").mkdir(parents=True)
                    (parent / ".cargo" / filename).write_text(
                        'paths=["/ambient/override"]\n'
                    )
                    failure = subprocess.run(
                        [
                            os.environ["VERIFIER"],
                            "--package",
                            str(package),
                            "--contract",
                            str(contract_path),
                            "--cargo",
                            str(cargo),
                            "--rustc",
                            str(rustc),
                            "--sysroot",
                            str(root),
                            "--output-root",
                            str(parent / "outputs"),
                            "--offline",
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertNotEqual(failure.returncode, 0)
                    failed_report = json.loads(
                        next((parent / "outputs").glob("*/report.json")).read_text()
                    )
                    self.assertEqual(
                        failed_report["diagnostics"]["phase"], "configuration"
                    )
                    self.assertIn(filename, failed_report["diagnostics"]["message"])


class MalformedReportTest(unittest.TestCase):
    def test_nonobject_report_has_an_actionable_contract_error(self):
        for invalid in (
            None,
            [],
            "success",
            {
                "schema_version": 1,
                "report_kind": "cargo_publish_validation",
                "status": "success",
                "completed": True,
            },
        ):
            with self.subTest(report=invalid), self.assertRaises(ValueError):
                validate_report(invalid)


if __name__ == "__main__":
    unittest.main()
