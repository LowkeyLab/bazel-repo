"""Public archive policy and versioned verified-artifact handoff contract."""

import hashlib
import json
import shutil
import tarfile
from pathlib import Path, PurePosixPath

SCHEMA_VERSION = 1
REPORT_KIND = "cargo_publish_validation"
TARGET = "x86_64-unknown-linux-gnu"
REQUIRED_CHECKS = (
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
GENERATED_FILES = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock"}
OPTIONAL_FILES = {".cargo_vcs_info.json"}


def digest(path):
    """Fingerprint actual bytes, independent of timestamps and filesystem metadata."""
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def package_path(value):
    """Accept only canonical relative package paths without traversal or ambiguity."""
    if (
        not isinstance(value, str)
        or not value
        or value.startswith("/")
        or "\\" in value
        or "\x00" in value
    ):
        raise ValueError(f"unsafe package path: {value!r}")
    if any(part in ("", ".", "..") for part in value.split("/")):
        raise ValueError(f"unsafe package path: {value!r}")
    return PurePosixPath(value)


def selected_targets(selection):
    """Cargo targets which successful generic validation must have built."""
    targets = []
    library = selection.get("library")
    if library:
        targets.append("lib:" + library["crate_name"])
    targets.extend("bin:" + binary["name"] for binary in selection.get("binaries", []))
    if not targets or len(set(targets)) != len(targets):
        raise ValueError("missing or conflicting selected library/binary targets")
    return sorted(targets)


def inspect_archive(archive, package, inputs, destination):
    """Validate every member before extracting, and check declared source/asset bytes.

    The destination must be new. Only declared inputs and known Cargo-generated
    files may appear; links, duplicate paths and special objects are rejected.
    Returns the extracted package directory and an inventory including directories.
    """
    root_name = str(package_path(package["name"] + "-" + package["version"]))
    if "/" in root_name:
        raise ValueError(f"unsafe package archive root: {root_name}")
    declared = {}
    for item in inputs:
        path = str(package_path(item["path"]))
        if path in declared or path in GENERATED_FILES | OPTIONAL_FILES:
            raise ValueError(f"conflicting declared package path: {path}")
        declared[path] = item
    allowed = set(declared) | GENERATED_FILES | OPTIONAL_FILES
    directories = {
        str(parent)
        for name in allowed
        for parent in PurePosixPath(name).parents
        if str(parent) != "."
    }
    seen = set()
    inventory = []
    data = {}
    with tarfile.open(archive) as source:
        for member in source.getmembers():
            raw_name = (
                member.name[:-1]
                if member.isdir() and member.name.endswith("/")
                else member.name
            )
            name = str(package_path(raw_name))
            if name in seen:
                raise ValueError(f"duplicate archive path: {name}")
            seen.add(name)
            if name != root_name and not name.startswith(root_name + "/"):
                raise ValueError(f"escaping or unexpected archive root: {name}")
            relative = name[len(root_name) :].lstrip("/")
            if not member.isfile() and not member.isdir():
                raise ValueError(f"archive link or special entry: {name}")
            if member.isdir():
                if relative and relative not in directories:
                    raise ValueError(f"unexpected archive directory: {relative}")
                if relative:
                    inventory.append({"path": relative, "type": "directory"})
                continue
            if relative not in allowed:
                raise ValueError(f"unexpected archive file: {relative or name}")
            stream = source.extractfile(member)
            if stream is None:
                raise ValueError(f"unreadable archive file: {name}")
            contents = stream.read()
            fingerprint = hashlib.sha256(contents).hexdigest()
            if relative in declared:
                expected = declared[relative]
                if (
                    len(contents) != expected["size"]
                    or fingerprint != expected["sha256"]
                ):
                    raise ValueError(f"archive input fidelity mismatch: {relative}")
            data[relative] = contents
            inventory.append(
                {
                    "path": relative,
                    "type": "file",
                    "size": len(contents),
                    "sha256": fingerprint,
                }
            )
    missing = (set(declared) | GENERATED_FILES) - set(data)
    if missing:
        raise ValueError(f"missing required archive files: {sorted(missing)}")
    for name in data:
        if any(str(parent) in data for parent in PurePosixPath(name).parents):
            raise ValueError(f"conflicting archive file/directory path: {name}")
    destination = Path(destination)
    destination.mkdir(mode=0o700)
    extracted = destination / root_name
    extracted.mkdir(mode=0o700)
    for name, contents in sorted(data.items()):
        path = extracted / name
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(contents)
    return extracted, sorted(inventory, key=lambda item: item["path"])


def _validate_report(report, expected_package=None):
    """Require the complete known report contract, not a caller-selected check list."""
    if (
        report.get("schema_version") != SCHEMA_VERSION
        or report.get("report_kind") != REPORT_KIND
    ):
        raise ValueError("unsupported cargo_publish validation report contract")
    if report.get("status") != "success" or report.get("completed") is not True:
        raise ValueError("validation report is unsuccessful or incomplete")
    for key in (
        "run_id",
        "started_at",
        "completed_at",
        "package",
        "selection",
        "configuration",
        "inputs",
        "toolchain",
        "manifests",
        "archive",
        "lockfile",
        "dependencies",
        "commands",
    ):
        if key not in report or report[key] is None:
            raise ValueError(f"missing validation report evidence: {key}")
    if not report["run_id"] or not report["completed_at"]:
        raise ValueError("missing completed run identity/timestamp")
    if expected_package and any(
        report["package"].get(key) != value for key, value in expected_package.items()
    ):
        raise ValueError(
            "verified package identity/version does not match expected consumer"
        )
    checks = report.get("checks", {})
    for name in REQUIRED_CHECKS:
        check = checks.get(name, {})
        if (
            check.get("completed") is not True
            or check.get("status") != "passed"
            or not isinstance(check.get("evidence"), dict)
            or not check["evidence"]
        ):
            raise ValueError(
                f"mandatory validation check incomplete or missing evidence: {name}"
            )
    targets = selected_targets(report["selection"])
    if checks["locked_builds"]["evidence"].get("targets") != targets:
        raise ValueError(
            "locked build evidence does not cover every selected library/binary"
        )
    toolchain = report["toolchain"]
    if (
        toolchain.get("host") != TARGET
        or toolchain.get("target") != TARGET
        or "release: 1.96.0\n" not in toolchain.get("rustc", "")
        or not toolchain.get("cargo", "").startswith("cargo 1.96.0 ")
    ):
        raise ValueError("unsupported pinned toolchain report evidence")
    if report["configuration"].get("target") != TARGET:
        raise ValueError("unsupported validation target configuration")
    manifests = report["manifests"]
    dependencies = report["dependencies"]
    expected_evidence = {
        "input_fidelity": {
            "inputs": report["inputs"],
            "manifest_sha256": manifests["generated"]["sha256"],
        },
        "configuration": report["configuration"],
        "cargo_package": {
            "archive": report["archive"]["path"],
            "verification_enabled": True,
        },
        "generated_manifest": {
            "package": report["package"],
            "targets": targets,
            "dependencies": dependencies["requested"],
        },
        "archive_policy": {
            "inventory": report["archive"]["inventory"],
            "root": report["package"]["name"] + "-" + report["package"]["version"],
        },
        "archive_fidelity": {"inputs": report["inputs"], "verified": True},
        "original_manifest": manifests["original"],
        "normalized_manifest": {
            "manifest": manifests["normalized"],
            "package": report["package"],
            "dependencies": dependencies["requested"],
        },
        "direct_dependencies": {
            "requested": dependencies["requested"],
            "resolved_graph": dependencies["observed"],
            "compared": True,
        },
    }
    for phase, expected in expected_evidence.items():
        evidence = checks[phase]["evidence"]
        if any(evidence.get(key) != value for key, value in expected.items()):
            raise ValueError(f"missing or inconsistent mandatory evidence: {phase}")
    if checks["toolchain"]["evidence"].get("versions") != toolchain:
        raise ValueError("missing or inconsistent mandatory evidence: toolchain")
    metadata = checks["locked_metadata"]["evidence"]
    if (
        metadata.get("sha256") != dependencies["metadata_sha256"]
        or metadata.get("path") != dependencies["metadata_path"]
        or metadata.get("lockfile_sha256") != report["lockfile"]["sha256"]
    ):
        raise ValueError("missing or inconsistent mandatory evidence: locked_metadata")
    if not dependencies["observed"]:
        raise ValueError("missing Cargo resolved graph evidence")
    commands = {command["id"]: command for command in report["commands"]}
    if len(commands) != len(report["commands"]):
        raise ValueError("duplicate validation command identities")
    for phase in ("toolchain", "cargo_package", "locked_metadata", "locked_builds"):
        identities = checks[phase]["evidence"].get("command_ids", [])
        if not identities:
            raise ValueError(f"missing successful command evidence: {phase}")
        for identity in identities:
            command = commands.get(identity, {})
            if (
                command.get("completed") is not True
                or command.get("returncode") != 0
                or command.get("phase") != phase
                or not command.get("argv")
                or not command.get("log")
            ):
                raise ValueError(f"unsuccessful command evidence: {phase}/{identity}")
            argv = command["argv"]
            if phase in ("locked_metadata", "locked_builds") and "--locked" not in argv:
                raise ValueError(f"missing locked command evidence: {phase}")
            if phase == "cargo_package" and (
                "--locked" in argv or "--no-verify" in argv or "package" not in argv
            ):
                raise ValueError(
                    "package command must resolve and verify the extracted package"
                )
    built = []
    for identity in checks["locked_builds"]["evidence"]["command_ids"]:
        argv = commands[identity]["argv"]
        if "build" not in argv:
            raise ValueError("locked build command does not build selected targets")
        if "--lib" in argv and report["selection"].get("library"):
            built.append("lib:" + report["selection"]["library"]["crate_name"])
        for index, argument in enumerate(argv[:-1]):
            if argument == "--bin":
                built.append("bin:" + argv[index + 1])
    if sorted(built) != targets:
        raise ValueError(
            "successful commands do not cover every selected library/binary"
        )
    for key in ("path", "sha256", "size", "inventory"):
        if key not in report["archive"]:
            raise ValueError(f"missing archive report evidence: {key}")
    if not report["archive"]["inventory"] or not report["lockfile"].get("sha256"):
        raise ValueError("missing archive inventory or packaged lockfile evidence")
    return report


def validate_report(report, expected_package=None):
    """Validate caller-supplied evidence with actionable errors for malformed JSON."""
    if not isinstance(report, dict):
        raise ValueError("validation report must be a JSON object")
    try:
        return _validate_report(report, expected_package)
    except (KeyError, TypeError, AttributeError, IndexError) as error:
        raise ValueError(
            f"missing or malformed validation report evidence: {error}"
        ) from error


def validate_verified_archive(
    report_path, archive_path, destination, expected_package=None
):
    """Consume explicit report/archive paths and return the exact verified extraction.

    Copy first, then hash and inspect that same private copy. Relocated artifacts
    are supported; report archive paths are diagnostic, while SHA256 binds bytes.
    """
    report = validate_report(
        json.loads(Path(report_path).read_text()), expected_package
    )
    destination = Path(destination)
    destination.mkdir(mode=0o700)
    copied = destination / "verified.crate"
    shutil.copyfile(archive_path, copied)
    archive = report["archive"]
    if digest(copied) != archive["sha256"] or copied.stat().st_size != archive["size"]:
        raise ValueError("archive/report SHA256 or size mismatch")
    try:
        extracted, inventory = inspect_archive(
            copied, report["package"], report["inputs"], destination / "extracted"
        )
    except tarfile.TarError as error:
        raise ValueError(f"invalid package archive: {error}") from error
    if inventory != archive["inventory"]:
        raise ValueError("archive/report inventory or file hash mismatch")
    if digest(extracted / "Cargo.lock") != report["lockfile"]["sha256"]:
        raise ValueError("packaged lockfile/report hash mismatch")
    for name, filename in (
        ("original", "Cargo.toml.orig"),
        ("normalized", "Cargo.toml"),
    ):
        if digest(extracted / filename) != report["manifests"][name]["sha256"]:
            raise ValueError(f"{filename}/report manifest hash mismatch")
    if (
        report["manifests"]["generated"]["sha256"]
        != report["manifests"]["original"]["sha256"]
    ):
        raise ValueError("original/generated manifest fidelity mismatch")
    return extracted
