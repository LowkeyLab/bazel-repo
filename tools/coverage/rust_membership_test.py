"""Check native Rust baselines and child execution through the coverage command."""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import TypedDict


class ObjectMetadata(TypedDict):
    object: str
    llvm_cov: str
    llvm_profdata: str
    no_mapping: bool
    diagnostic: str


BAZEL_OPTIONS = ["--remote_cache=", "--remote_executor=", "--bes_backend="]
EVIDENCE: Path | None = None
COMMAND_COUNT = 0


def redact_output(text: str) -> str:
    return re.sub(r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+", r"\1<REDACTED>", text)


def command(
    root: Path,
    *arguments: str,
    success: bool = True,
    environment: dict[str, str] | None = None,
) -> str:
    global COMMAND_COUNT
    env = {
        key: value
        for key, value in os.environ.items()
        if key not in {"PYTHONHOME", "PYTHONEXECUTABLE"}
    }
    env.update(environment or {})
    result = subprocess.run(
        ["bazel", arguments[0], *BAZEL_OPTIONS, *arguments[1:]],
        cwd=root,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    output = redact_output(result.stdout + result.stderr)
    COMMAND_COUNT += 1
    if EVIDENCE is not None:
        for events in EVIDENCE.glob("*.bep.json"):
            events.write_text(redact_output(events.read_text()))
        (EVIDENCE / f"command-{COMMAND_COUNT}.log").write_text(output)
        with (EVIDENCE / "commands.jsonl").open("a") as log:
            log.write(
                json.dumps(
                    {
                        "arguments": [
                            "bazel",
                            arguments[0],
                            *BAZEL_OPTIONS,
                            *arguments[1:],
                        ],
                        "exit": result.returncode,
                    }
                )
                + "\n"
            )
    assert (result.returncode == 0) == success, output
    return result.stdout


def source(root: Path, path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    command(root, "run", "//:gazelle")


def records(root: Path) -> dict[str, dict[int, int]]:
    text = (root / "coverage-report.lcov").read_text()
    if EVIDENCE is not None:
        (EVIDENCE / "membership.lcov").write_text(text)
        for name in [
            "coverage-inventory.json",
            "coverage-rust-baselines.json",
            "coverage-go-baselines.json",
        ]:
            shutil.copyfile(root / name, EVIDENCE / name)
    found = {}
    for record in text.split("end_of_record"):
        if not record.strip():
            continue
        lines = record.splitlines()
        name = next(line[3:] for line in lines if line.startswith("SF:"))
        assert name not in found, name
        found[name] = {
            int(line[3:].split(",")[0]): int(line[3:].split(",")[1])
            for line in lines
            if line.startswith("DA:")
        }
    return found


def object_metadata(events_file: Path, label: str, execroot: Path) -> ObjectMetadata:
    events = [json.loads(line) for line in events_file.read_text().splitlines()]
    sets = {
        event["id"]["namedSet"]["id"]: event["namedSetOfFiles"]
        for event in events
        if "namedSetOfFiles" in event
    }

    def files(identifier: str) -> list[Path]:
        entry = sets[identifier]
        paths = [
            execroot.joinpath(*file.get("pathPrefix", []), file["name"])
            for file in entry.get("files", [])
        ]
        for nested in entry.get("fileSets", []):
            paths.extend(files(nested["id"]))
        return paths

    completed = next(
        event["completed"]
        for event in events
        if event.get("id", {}).get("targetCompleted", {}).get("label") == label
    )
    group = next(
        group
        for group in completed["outputGroup"]
        if group["name"] == "rust_coverage_baselines"
    )
    paths = [
        path
        for entry in group["fileSets"]
        for path in files(entry["id"])
        if path.suffix == ".json"
    ]
    assert len(paths) == 1, paths
    return json.loads(paths[0].read_text())


def collector_failures(
    root: Path, runtime: Path, events: Path, test_label: str, child_label: str
) -> None:
    execroot = Path(command(root, "info", "execution_root").strip())
    main = object_metadata(events, test_label + ".rust_coverage", execroot)
    child = object_metadata(events, child_label + ".rust_coverage", execroot)
    binary = execroot / main["object"]
    runfiles = runtime / "runfiles"
    child_runfile = runfiles / "_main" / child_label[2:].replace(":", "/")
    child_runfile.parent.mkdir(parents=True)
    child_runfile.symlink_to(execroot / child["object"])
    raw = runtime / "raw-profiles"
    raw.mkdir()
    environment = {
        **os.environ,
        "RUNFILES_DIR": str(runfiles),
        "COVERAGE_DIR": str(raw),
        "LLVM_PROFILE_FILE": str(raw / "profile-%p-%m.profraw"),
    }
    process = subprocess.Popen(
        [str(binary)],
        cwd=runfiles / "_main",
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    stdout, stderr = process.communicate()
    if EVIDENCE is not None:
        (EVIDENCE / "functional-profile-run.log").write_text(stdout + stderr)
        (EVIDENCE / "functional-profile-run.json").write_text(
            json.dumps(
                {
                    "arguments": [str(binary)],
                    "cwd": str(runfiles / "_main"),
                    "profile_pattern": environment["LLVM_PROFILE_FILE"],
                    "pid": process.pid,
                    "exit": process.returncode,
                }
            )
            + "\n"
        )
    assert process.returncode == 0, stdout + stderr
    profiles = list(raw.glob("*.profraw"))
    own_profiles = [
        path for path in profiles if path.name.startswith(f"profile-{process.pid}-")
    ]
    assert own_profiles and len(profiles) > len(own_profiles), profiles
    command(
        root,
        "build",
        "@rules_rust//rust/coverage:collect_rust_coverage",
        "//tools/coverage:llvm-readobj",
    )
    collector = (
        execroot
        / command(
            root,
            "cquery",
            "@rules_rust//rust/coverage:collect_rust_coverage",
            "--output=files",
        ).strip()
    )
    reader = (
        execroot
        / command(
            root, "cquery", "//tools/coverage:llvm-readobj", "--output=files"
        ).strip()
    )
    for fault in [
        "valid",
        "missing",
        "empty",
        "corrupt",
        "truncated",
        "missing-object",
        "wrong-object",
        "empty-object",
        "corrupt-object",
        "truncated-object",
        "missing-child-object",
        "missing-main-profile",
    ]:
        case = runtime / fault
        case.mkdir()
        for profile in profiles:
            if fault == "missing" or (
                fault == "missing-main-profile" and profile in own_profiles
            ):
                continue
            shutil.copyfile(profile, case / profile.name)
        if fault in {"empty", "corrupt", "truncated"}:
            profile = case / profiles[0].name
            profile.write_bytes(
                b""
                if fault == "empty"
                else b"invalid profile"
                if fault == "corrupt"
                else profile.read_bytes()[:32]
            )
        env = {
            **os.environ,
            "COVERAGE_DIR": str(case),
            "ROOT": str(execroot),
            "RUNFILES_DIR": str(runfiles),
            "TEST_WORKSPACE": "_main",
            "TEST_BINARY": str(binary),
            "RUST_COVERAGE_REQUIRED": "1",
            "RUST_LLVM_READOBJ": str(reader),
            "RUST_COVERAGE_OBJECTS": str(execroot / child["object"]),
            "RUST_LLVM_COV": str(execroot / main["llvm_cov"]),
            "RUST_LLVM_PROFDATA": str(execroot / main["llvm_profdata"]),
        }
        if fault == "missing-object":
            env["TEST_BINARY"] = str(case / "missing")
        elif fault == "wrong-object":
            env["TEST_BINARY"] = str(execroot / child["object"])
        elif fault in {"empty-object", "corrupt-object", "truncated-object"}:
            object_copy = case / "object"
            object_copy.write_bytes(
                b""
                if fault == "empty-object"
                else b"invalid object"
                if fault == "corrupt-object"
                else binary.read_bytes()[:32]
            )
            env["TEST_BINARY"] = str(object_copy)
        elif fault == "missing-child-object":
            env["RUST_COVERAGE_OBJECTS"] = str(case / "missing")
        result = subprocess.run(
            [str(collector)],
            cwd=execroot,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        if EVIDENCE is not None:
            (EVIDENCE / f"collector-{fault}.log").write_text(
                result.stdout + result.stderr
            )
            with (EVIDENCE / "collector-results.jsonl").open("a") as log:
                log.write(
                    json.dumps(
                        {
                            "fault": fault,
                            "arguments": [str(collector)],
                            "cwd": str(execroot),
                            "environment": {
                                key: value
                                for key, value in env.items()
                                if key
                                in {
                                    "ROOT",
                                    "RUNFILES_DIR",
                                    "TEST_WORKSPACE",
                                    "TEST_BINARY",
                                    "COVERAGE_DIR",
                                    "RUST_COVERAGE_REQUIRED",
                                    "RUST_COVERAGE_OBJECTS",
                                    "RUST_LLVM_COV",
                                    "RUST_LLVM_PROFDATA",
                                    "RUST_LLVM_READOBJ",
                                }
                            },
                            "exit": result.returncode,
                        }
                    )
                    + "\n"
                )
        assert (result.returncode == 0) == (fault == "valid"), (
            f"{fault}: {result.stdout}{result.stderr}"
        )


def retain_baselines(events: Path, root: Path) -> None:
    if EVIDENCE is None:
        return
    for line in events.read_text().splitlines():
        entry = json.loads(line).get("namedSetOfFiles", {})
        for file in entry.get("files", []):
            if not file["name"].endswith(
                (".rust_coverage.json", ".rust_coverage.lcov")
            ):
                continue
            relative = Path(*file.get("pathPrefix", []), file["name"])
            assert not relative.is_absolute() and ".." not in relative.parts, relative
            destination = EVIDENCE / "baselines" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / relative, destination)


def main() -> None:
    global EVIDENCE, BAZEL_OPTIONS
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--execution", choices=["local", "configured"], default="local")
    args = parser.parse_args()
    root = Path(os.environ["BUILD_WORKSPACE_DIRECTORY"])
    EVIDENCE = args.evidence.resolve() if args.evidence is not None else None
    if EVIDENCE is not None and EVIDENCE.is_relative_to(root.resolve()):
        parser.error("Use an evidence directory outside the checkout.")
    if args.execution == "configured":
        BAZEL_OPTIONS = []
    if EVIDENCE is not None:
        EVIDENCE.mkdir(mode=0o700, parents=True, exist_ok=False)
    if EVIDENCE is not None:
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True
        ).strip()
        changes = subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=root, text=True
        )
        (EVIDENCE / "configuration.json").write_text(
            json.dumps(
                {
                    "revision": revision,
                    "changes": changes,
                    "execution": args.execution,
                    "bazel_options": BAZEL_OPTIONS,
                }
            )
            + "\n"
        )
    with tempfile.TemporaryDirectory(prefix="rust_membership_", dir=root) as ordinary:
        with (
            tempfile.TemporaryDirectory(
                prefix="rust_membership_", dir=root / ".github"
            ) as hidden,
            tempfile.TemporaryDirectory(prefix="rust_runtime_") as runtime_directory,
        ):
            package = Path(ordinary)
            baseline = package / "unexecuted/lib.rs"
            child = package / "child/main.rs"
            runner = package / "runner/test.rs"
            empty = package / "empty/test.rs"
            generated = package / "generated/lib.rs"
            hidden_source = Path(hidden) / "lib.rs"
            for path in [baseline, hidden_source]:
                source(root, path, "pub fn disconnected() -> u32 {\n    73\n}\n")
                (path.parent / "BUILD.bazel").write_text(
                    'load("//tools/coverage:rust.bzl", "rust_library")\nrust_library(name = "disconnected", srcs = ["lib.rs"], tags = ["manual", "no-coverage"])\n'
                )
            source(root, child, "fn main() {\n    std::hint::black_box(91);\n}\n")
            (child.parent / "BUILD.bazel").write_text(
                'load("//tools/coverage:rust.bzl", "rust_binary")\nrust_binary(name = "child", srcs = ["main.rs"], visibility = ["//visibility:public"])\n'
            )
            source(root, empty, "// This source has no executable lines.\n")
            (empty.parent / "BUILD.bazel").write_text(
                'load("//tools/coverage:rust.bzl", "rust_test")\nrust_test(name = "empty", srcs = ["test.rs"], tags = ["manual"])\n'
            )
            generated.parent.mkdir(parents=True)
            generated_label = (
                "//" + str(generated.parent.relative_to(root)) + ":generated_lib"
            )
            (generated.parent / "BUILD.bazel").write_text(
                '# gazelle:ignore\nload("//tools/coverage:rust.bzl", "rust_library")\n'
                'genrule(name = "generate", outs = ["generated.rs"], cmd = '
                + json.dumps("echo 'pub fn generated() -> u32 { 999 }' > $@")
                + ")\n"
                'rust_library(name = "generated_lib", crate_name = "generated_lib", crate_root = "lib.rs", srcs = ["lib.rs", ":generated.rs"], visibility = ["//visibility:public"])\n'
            )
            source(
                root,
                generated,
                'include!("generated.rs");\npub fn kept() -> u32 { generated() }\n',
            )
            runner.parent.mkdir(parents=True)
            resolution = (
                "# gazelle:resolve rust generated_lib " + generated_label + "\n"
            )
            (runner.parent / "BUILD.bazel").write_text(resolution)
            source(
                root,
                runner,
                'use googletest::{verify_that, matchers::eq};\n#[test]\nfn runs_child() -> googletest::Result<()> {\n    let status = std::process::Command::new(env!("CARGO_BIN_EXE_child")).status()?;\n    verify_that!(status.success(), eq(true))?;\n    verify_that!(generated_lib::kept(), eq(999))?;\n    Ok(())\n}\n',
            )
            child_label = "//" + str(child.parent.relative_to(root)) + ":child"
            test_label = "//" + str(runner.parent.relative_to(root)) + ":runner"
            empty_label = "//" + str(empty.parent.relative_to(root)) + ":empty"
            (runner.parent / "BUILD.bazel").write_text(
                resolution
                + 'load("//tools/coverage:rust.bzl", "rust_test")\nrust_test(name = "runner", srcs = ["test.rs"], data = '
                + json.dumps([child_label])
                + ", deps = "
                + json.dumps(["@crates//:googletest", generated_label])
                + ")\n"
            )
            command(root, "run", "//:gazelle")
            if EVIDENCE is not None:
                for fixture in [package, Path(hidden)]:
                    shutil.copytree(
                        fixture, EVIDENCE / "fixtures" / fixture.relative_to(root)
                    )
            command(root, "test", test_label, empty_label)
            runtime = Path(runtime_directory)
            events = (EVIDENCE or runtime) / "coverage.bep.json"
            command(
                root,
                "run",
                "//tools/coverage",
                "--",
                test_label,
                empty_label,
                *BAZEL_OPTIONS,
                "--build_event_json_file=" + str(events),
            )
            report = records(root)
            retain_baselines(events, root)
            for path in [baseline, hidden_source]:
                assert report[str(path.relative_to(root))][2] == 0
            assert report[str(runner.relative_to(root))][4] > 0
            assert report[str(child.relative_to(root))][2] > 0, (
                "The child process must retain measured hits after baseline merging."
            )
            assert report[str(generated.relative_to(root))][2] > 0
            assert (
                str(generated.parent.relative_to(root) / "generated.rs") not in report
            )
            receipt = json.loads((root / "coverage-rust-baselines.json").read_text())
            assert any(
                limit["source"] == str(empty.relative_to(root))
                for limit in receipt["limits"]
            )
            collector_failures(
                root, EVIDENCE or runtime, events, test_label, child_label
            )
            output = command(
                root,
                "run",
                "//tools/coverage",
                "--",
                test_label,
                *BAZEL_OPTIONS,
                "--instrumentation_filter=^//tools/coverage:",
                success=False,
            )
            assert "outside the instrumentation filter" in output


if __name__ == "__main__":
    main()
