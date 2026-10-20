"""Run the bounded shell collector experiment and preserve each observation."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import TypedDict

collector = Path(sys.argv[1]).resolve()
out = Path(sys.argv[5]).resolve()
out.mkdir(parents=True, exist_ok=True)
fixtures = out / "fixtures"
fixtures.mkdir()
fixture_root = Path(sys.argv[4]).resolve().parent
contents = {
    name: (fixture_root / name).read_text()
    for name in ["main.sh", "helper.sh", "child.sh", "disconnected.sh"]
}
for name, source in contents.items():
    (fixtures / name).write_text(source)

workflow = Path(sys.argv[2]).read_text().splitlines()
start = next(i for i, line in enumerate(workflow) if "install -m755" in line)
workflow_script = fixtures / "workflow.sh"
workflow_script.write_text("#!/usr/bin/env bash\n" + workflow[start].lstrip() + "\n")
launcher = Path(sys.argv[3]).read_text().splitlines()
launcher_start = next(
    i for i, line in enumerate(launcher) if 'ctx.actions.write(script, """#!/' in line
)
launcher_lines = [launcher[launcher_start].split('"""', 1)[1]]
for line in launcher[launcher_start + 1 :]:
    if '""".format(' in line:
        break
    launcher_lines.append(line)
launcher_script = fixtures / "launcher.sh"
launcher_script.write_text(
    "\n".join(launcher_lines)
    .replace("{{", "{")
    .replace("}}", "}")
    .replace("{tool}", "mock-tool")
    .replace("{rustfmt}", "mock-rustfmt")
    + "\n"
)
runfiles = out / "runfiles"
runfiles.mkdir()
(runfiles / "mock-tool").write_text(
    '#!/usr/bin/env bash\n[[ "$1" == */mock-rustfmt ]]\nprintf "%s\\n" LAUNCHER_FUNCTIONAL_PASS\n'
)
(runfiles / "mock-tool").chmod(0o755)

# Stub the workflow's external install command. Preserve the original shell line.
# This isolates shell position mapping from privileged installation effects.
install_env = out / "install-env.sh"
install_env.write_text(
    'install() { printf "%s\\n" "WORKFLOW_FUNCTIONAL_PASS $*"; }; export -f install\n'
)
base_env = dict(
    os.environ,
    MODE="yes",
    SECOND="yes",
    RUNFILES_DIR=str(runfiles),
    LCOV_PATH="/fixture/lcov",
)


class RunObservation(TypedDict, total=False):
    argv: list[str]
    exit: int
    records: dict[str, dict[int, int]]
    collector_rejected_loss: bool


runs: dict[str, RunObservation] = {}
observations: dict[str, object] = {}
bash_candidate = shutil.which("bash")
if bash_candidate is None:
    raise RuntimeError("The Bash interpreter is missing")
bash: str = bash_candidate


def run(
    name: str,
    script: Path,
    options: tuple[str, ...] = (),
    env: dict[str, str] | None = None,
) -> tuple[subprocess.CompletedProcess[str], Path]:
    destination = out / name
    command = [
        str(collector),
        "--bash-parser=" + bash,
        "--include-path=" + str(fixtures),
        "--bash-parse-files-in-dirs=" + str(fixtures),
        *options,
        str(destination),
        str(script),
    ]
    result = subprocess.run(
        command,
        text=True,
        capture_output=True,
        env=env or base_env,
        timeout=30,
        check=False,
    )
    (out / (name + ".stdout")).write_text(result.stdout)
    (out / (name + ".stderr")).write_text(result.stderr)
    runs[name] = {"argv": command, "exit": result.returncode}
    return result, destination


def records(destination: Path) -> dict[str, dict[int, int]]:
    reports = list(destination.glob("*/cobertura.xml"))
    if not reports:
        raise RuntimeError("Collector emitted no Cobertura report")
    tree = ET.parse(reports[0])
    values = {}
    for item in tree.findall(".//class"):
        path = item.attrib["filename"]
        values[path] = {
            int(line.attrib["number"]): int(line.attrib["hits"])
            for line in item.findall("./lines/line")
        }
    return values


driver = out / "baseline-driver.sh"
driver.write_text("#!/usr/bin/env bash\n:\n")
baseline_result, baseline_dir = run("baseline", driver)
assert baseline_result.returncode == 0
runs["baseline"]["records"] = records(baseline_dir)
assert all(
    hit == 0 for lines in runs["baseline"]["records"].values() for hit in lines.values()
)
regular, regular_dir = run("regular", fixtures / "main.sh")
assert regular.returncode == 0 and "FUNCTIONAL_PASS" in regular.stdout
runs["regular"]["records"] = records(regular_dir)
# A child can suppress collection while its functional work succeeds.
original_child = (fixtures / "child.sh").read_text()
(fixtures / "child.sh").write_text(
    '#!/usr/bin/env bash\nset +x\nprintf "%s\\n" CHILD_FUNCTIONAL_PASS\n'
)
loss, loss_dir = run("child-trace-loss", fixtures / "main.sh")
assert (
    loss.returncode == 0
    and "CHILD_FUNCTIONAL_PASS" in loss.stdout
    and "FUNCTIONAL_PASS" in loss.stdout
)
loss_records = records(loss_dir)
runs["child-trace-loss"]["records"] = loss_records
runs["child-trace-loss"]["collector_rejected_loss"] = loss.returncode != 0
(fixtures / "child.sh").write_text(original_child)
# Establish explicit child report ownership, then remove it after assertions pass.
child, child_dir = run("child-owned", fixtures / "child.sh")
assert child.returncode == 0 and "CHILD_FUNCTIONAL_PASS" in child.stdout
owned_report = next(child_dir.glob("*/cobertura.xml"))
runs["child-owned"]["records"] = records(child_dir)
shutil.copyfile(owned_report, out / "child-owned.raw.xml")
owned_report.unlink()
try:
    records(child_dir)
except RuntimeError as error:
    observations["deleted-child-report"] = {
        "functional_exit": child.returncode,
        "adapter_rejected": True,
        "error": str(error),
    }
else:
    raise AssertionError("Missing child report passed validation")
owned_report.write_text("corrupt XML")
try:
    records(child_dir)
except ET.ParseError as error:
    observations["corrupt-child-report"] = {
        "functional_exit": child.returncode,
        "adapter_rejected": True,
        "error": str(error),
    }
else:
    raise AssertionError("Corrupt child report passed validation")
workflow_env = dict(
    base_env,
    **{"BASH_FUNC_install%%": '() { printf "%s\\n" "WORKFLOW_FUNCTIONAL_PASS $*"; }'},
)
workflow_result, workflow_dir = run("workflow", workflow_script, env=workflow_env)
assert (
    workflow_result.returncode == 0
    and "WORKFLOW_FUNCTIONAL_PASS" in workflow_result.stdout
)
launcher_result, launcher_dir = run("launcher", launcher_script)
assert (
    launcher_result.returncode == 0
    and "LAUNCHER_FUNCTIONAL_PASS" in launcher_result.stdout
)
maps = {
    str(workflow_script): {
        2: {"path": ".github/workflows/coverage.yml", "line": start + 1}
    },
    str(launcher_script): {
        i + 1: {"path": "tools/topcoat/format.bzl", "line": launcher_start + i + 1}
        for i in range(len(launcher_lines))
    },
}
(out / "source-maps.json").write_text(json.dumps(maps, indent=2) + "\n")
combined = {}
for name, directory in [
    ("regular", regular_dir),
    ("workflow", workflow_dir),
    ("launcher", launcher_dir),
]:
    measured = records(directory)
    runs[name]["records"] = measured
    for path, lines in measured.items():
        canonical = "docs/research/shell-2453/fixtures/" + Path(path).name
        mapping = maps.get(path) or maps.get(str(fixtures / path))
        for line, hits in lines.items():
            if mapping is not None:
                if line not in mapping:
                    continue
                canonical = mapping[line]["path"]
                line = mapping[line]["line"]
            combined.setdefault(canonical, {})[line] = (
                combined.get(canonical, {}).get(line, 0) + hits
            )
with (out / "combined.lcov").open("w") as report:
    for path, lines in sorted(combined.items()):
        report.write("TN:shell-feasibility\nSF:" + path + "\n")
        for line, hits in sorted(lines.items()):
            report.write(f"DA:{line},{hits}\n")
        report.write(
            f"LF:{len(lines)}\nLH:{sum(value > 0 for value in lines.values())}\nend_of_record\n"
        )
baseline_rows = {}
for path, lines in records(baseline_dir).items():
    canonical = "docs/research/shell-2453/fixtures/" + Path(path).name
    mapping = maps.get(path) or maps.get(str(fixtures / path))
    for line, hits in lines.items():
        original_line = line
        if mapping is not None:
            if line not in mapping:
                continue
            canonical = mapping[line]["path"]
            original_line = mapping[line]["line"]
        baseline_rows.setdefault(canonical, {})[original_line] = hits
with (out / "baseline.lcov").open("w") as report:
    for path, lines in sorted(baseline_rows.items()):
        report.write("SF:" + path + "\n")
        for line, hits in sorted(lines.items()):
            report.write(f"DA:{line},{hits}\n")
        report.write("end_of_record\n")
observations["bash_version"] = subprocess.run(
    [bash, "--version"], capture_output=True, text=True, check=True
).stdout.splitlines()[0]
observations["branch_records"] = (
    "No BRDA export. Collector source emits only line hits."
)
observations["interpolation"] = {
    "workflow_values": "Environment-variable interpolation preserves the line.",
    "launcher_values": "Two literal path replacements preserve the line.",
    "multiline_replacement": "Not tested. A candidate needs a segment map and must reject ambiguous mapping.",
}
observations["collector_sha256"] = hashlib.sha256(collector.read_bytes()).hexdigest()
observations.update(runs)
(out / "observations.json").write_text(json.dumps(observations, indent=2) + "\n")
print(
    json.dumps({"status": "complete", "observations": str(out / "observations.json")})
)
