"""Check Go membership through the existing coverage command."""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

EVIDENCE: Path | None = None
COMMAND_COUNT = 0


def redact_output(output: str) -> str:
    return re.sub(
        r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
        r"\1<REDACTED>",
        output,
    )


def command(root: Path, *arguments: str, success: bool = True) -> str:
    global COMMAND_COUNT
    result = subprocess.run(
        ["bazel", *arguments], cwd=root, capture_output=True, text=True, check=False
    )
    output = redact_output(result.stdout + result.stderr)
    if EVIDENCE is not None:
        for path in EVIDENCE.glob("*.bep.json"):
            path.write_text(redact_output(path.read_text()))
        COMMAND_COUNT += 1
        (EVIDENCE / f"command-{COMMAND_COUNT}.log").write_text(output)
        with (EVIDENCE / "commands.jsonl").open("a") as log:
            log.write(
                json.dumps({"arguments": arguments, "exit": result.returncode}) + "\n"
            )
    assert (result.returncode == 0) == success, output
    return output


def source(root: Path, path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    command(root, "run", "//:gazelle")


def report(root: Path, name: str) -> dict[str, list[str]]:
    text = (root / "coverage-report.lcov").read_text()
    if EVIDENCE is not None:
        (EVIDENCE / f"{name}.lcov").write_text(text)
        shutil.copyfile(
            root / "coverage-inventory.json", EVIDENCE / f"{name}-inventory.json"
        )
    records: dict[str, list[str]] = {}
    for record in text.split("end_of_record"):
        lines = record.strip().splitlines()
        if not lines:
            continue
        path = next(line[3:] for line in lines if line.startswith("SF:"))
        assert path not in records, path
        records[path] = lines
    return records


def main() -> None:
    global EVIDENCE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    EVIDENCE = args.evidence
    if EVIDENCE is not None:
        EVIDENCE.mkdir(parents=True, exist_ok=True)
    root = Path(os.environ["BUILD_WORKSPACE_DIRECTORY"])
    with tempfile.TemporaryDirectory(prefix="go_membership_", dir=root) as ordinary:
        with tempfile.TemporaryDirectory(
            prefix="go_membership_", dir=root / ".github"
        ) as hidden:
            packages = [Path(ordinary), Path(hidden)]
            prefix = packages[0].relative_to(root).as_posix()
            for package in packages:
                (package / ".gitattributes").write_text(
                    "generated.go coverage-scope=generated\n"
                )
                for name, text in {
                    "generated.go": "package fixture\n\nfunc Generated() int { return 42 }\n",
                    "library.go": "package fixture\n\nfunc Unused() int {\n return 7\n}\n",
                    "internal_test.go": """package fixture
import ("testing"; "os"; "os/exec"; "path/filepath"; "strings")
func unusedInternal() int {
 return 8
}
func TestMeasured(t *testing.T) {
 if Unused() != 7 { t.Fatal("Unexpected library result.") }
 if child := os.Getenv("GO_MEMBERSHIP_CHILD"); child != "" {
  path := filepath.Join(os.Getenv("TEST_SRCDIR"), os.Getenv("TEST_WORKSPACE"), child)
  child := exec.Command(path)
  child.Env = append(os.Environ(), "GOCOVERDIR=" + os.Getenv("COVERAGE_DIR"))
  output, err := child.Output()
  if err != nil || strings.TrimSpace(string(output)) != "7" { t.Fatalf("Child result: %s, %v", output, err) }
 }
}
""",
                    "external_test.go": "package fixture_test\n\nfunc unusedExternal() int {\n return 9\n}\n",
                    "declarations.go": "package fixture\n\ntype Identifier int\n",
                    "excluded.go": "//go:build never_coverage_fixture\n\npackage fixture\n\nfunc Excluded() int { return 10 }\n",
                }.items():
                    source(root, package / name, text)
            child_label = f"//{prefix}/child:child"
            source(
                root,
                packages[0] / "child/main.go",
                f"""package main
import ("fmt"; fixture "github.com/lowkeylab/bazel-repo/{prefix}")
func main() {{
 fmt.Println(fixture.Unused())
}}
""",
            )
            source(
                root,
                packages[0] / "fault/fault_test.go",
                """package fault
import ("flag"; "os"; "testing")
func TestFunctional(t *testing.T) {
 if os.Getenv("COVERAGE_DIR") != "" {
  if path := os.Getenv("GO_BAD_PROFILE"); path != "" {
   if err := flag.Set("test.coverprofile", path); err != nil { t.Fatal(err) }
  }
 }
}
""",
            )
            for package in packages:
                build = package / "BUILD.bazel"
                text = (
                    build.read_text()
                    .replace(
                        '"declarations.go",',
                        '"declarations.go",\n        "excluded.go",  # keep',
                    )
                    .replace(
                        "srcs = [",
                        'tags = ["manual", "no-remote", "no-coverage"],\n    srcs = [',
                    )
                )
                if package == packages[0]:
                    text = text.replace(
                        'name = "' + package.name + '_test",',
                        'name = "' + package.name + '_test",\n'
                        f'    data = ["{child_label}"],\n'
                        f'    env = {{"GO_MEMBERSHIP_CHILD": "$(rootpath {child_label})"}},',
                    )
                build.write_text(text)
            if EVIDENCE is not None:
                shutil.copytree(packages[0], EVIDENCE / "ordinary", dirs_exist_ok=True)
                shutil.copytree(packages[1], EVIDENCE / "hidden", dirs_exist_ok=True)
            command(root, "run", "//tools/coverage", "--", "//python_sample/...")
            baseline = report(root, "baseline")
            for package in packages:
                name = package.relative_to(root).as_posix()
                for filename in ("library.go", "internal_test.go", "external_test.go"):
                    lines = baseline.get(f"{name}/{filename}", [])
                    assert "DA:4,0" in lines, (name, filename, lines)
                    assert not any(line.startswith("BRDA:") for line in lines), lines
                assert "LF:0" in baseline[f"{name}/declarations.go"]
                assert f"{name}/excluded.go" not in baseline
                assert f"{name}/generated.go" not in baseline
            target = f"//{prefix}:{packages[0].name}_test"
            options = (
                []
                if EVIDENCE is None
                else [f"--build_event_json_file={EVIDENCE / 'measured.bep.json'}"]
            )
            command(
                root,
                "run",
                "//tools/coverage",
                "--",
                "//python_sample/...",
                target,
                "--test_tag_filters=",
                *options,
            )
            measured = report(root, "measured")
            assert any(
                line.startswith("DA:4,") and line != "DA:4,0"
                for line in measured[f"{prefix}/library.go"]
            )
            assert any(
                line.startswith("DA:4,") and line != "DA:4,0"
                for line in measured[f"{prefix}/child/main.go"]
            )
            assert "DA:4,0" in measured[f"{prefix}/internal_test.go"]
            assert any(
                line.startswith("DA:7,") and line != "DA:7,0"
                for line in measured[f"{prefix}/internal_test.go"]
            )
            for name, lines in baseline.items():
                if not name.startswith(prefix + "/"):
                    assert measured[name] == lines, name
            fault = f"//{prefix}/fault:fault_test"
            command(root, "test", fault, "--test_env=GO_BAD_PROFILE=/dev/null")
            for path in ("/dev/null", "/missing-go-coverage-directory/profile"):
                output = command(
                    root,
                    "run",
                    "//tools/coverage",
                    "--",
                    fault,
                    f"--test_env=GO_BAD_PROFILE={path}",
                    success=False,
                )
                assert (
                    "PASS\n" in output and "error generating coverage report:" in output
                ), output
            print(
                "Go membership, measured execution, child execution, and failure acceptance passed."
            )


if __name__ == "__main__":
    main()
