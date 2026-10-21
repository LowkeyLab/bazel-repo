"""Check real Node and Angular behavior through the existing coverage command."""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execution", choices=["local", "configured"], default="local")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())).resolve()
    evidence = args.evidence or Path(
        tempfile.mkdtemp(prefix="javascript-coverage-evidence-")
    )
    if args.evidence:
        evidence.mkdir(mode=0o700)
    evidence = evidence.resolve()
    if evidence.is_relative_to(root):
        parser.error("The evidence directory must be outside the checkout.")
    options = (
        ["--remote_cache=", "--remote_executor=", "--bes_backend="]
        if args.execution == "local"
        else ["--config=remote-linux"]
    )
    environment = {
        key: value
        for key, value in os.environ.items()
        if key not in {"PYTHONHOME", "PYTHONEXECUTABLE"}
    }
    sequence = 0

    def run(*arguments: str) -> str:
        nonlocal sequence
        command = ["bazel", arguments[0], *options, *arguments[1:]]
        result = subprocess.run(
            command,
            cwd=root,
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        sequence += 1
        output = re.sub(
            r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
            r"\1<REDACTED>",
            result.stdout + result.stderr,
        )
        (evidence / f"command-{sequence}.log").write_text(output)
        with (evidence / "commands.jsonl").open("a") as stream:
            stream.write(
                json.dumps({"arguments": command, "exit": result.returncode}) + "\n"
            )
        print(f"Command {sequence}: exit {result.returncode}", flush=True)
        assert result.returncode == 0, output
        return result.stdout

    def write(path: Path, text: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        assert not path.exists(), path
        path.write_text(text)
        snapshot = evidence / "sources" / path.relative_to(root)
        snapshot.parent.mkdir(parents=True, exist_ok=True)
        snapshot.write_text(text)
        run("run", "//:gazelle")

    def cover(targets: list[str], name: str) -> dict[str, str]:
        events = evidence / f"{name}.bep.json"
        run(
            "run",
            "//tools/coverage",
            "--",
            *targets,
            *options,
            "--nocache_test_results",
            f"--build_event_json_file={events}",
        )
        events.write_text(
            re.sub(
                r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
                r"\1<REDACTED>",
                events.read_text(),
            )
        )
        shutil.copyfile(root / "coverage-report.lcov", evidence / f"{name}.lcov")
        shutil.copyfile(
            root / "coverage-inventory.json", evidence / f"{name}-inventory.json"
        )
        shutil.copytree(root / "coverage-javascript", evidence / f"{name}-javascript")
        found = {}
        for record in (
            (root / "coverage-report.lcov").read_text().split("end_of_record")
        ):
            if record.strip():
                source = next(
                    line[3:] for line in record.splitlines() if line.startswith("SF:")
                )
                assert source not in found, source
                found[source] = record
        return found

    (evidence / "revision.txt").write_text(
        subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=root,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )
    angular = root / "angular/projects/nicknamer/src"
    created = []
    try:
        with (
            tempfile.TemporaryDirectory(prefix="coverage_js_", dir=root) as temporary,
            tempfile.TemporaryDirectory(
                prefix="coverage_js_", dir=root / ".github"
            ) as hidden_temporary,
        ):
            package_path = Path(temporary)
            package = package_path.name
            hidden = Path(hidden_temporary)
            write(
                package_path / "library.ts",
                "export function choose(flag: boolean): number {\n  if (flag) { return 7; }\n  return 3;\n}\n",
            )
            write(
                package_path / "child.mjs",
                'if (process.argv.length > 1) {\n  process.stdout.write("23");\n}\n',
            )
            write(
                package_path / "unused.mjs",
                "export function unused(flag) {\n  if (flag) { return 7; }\n  return 3;\n}\n",
            )
            write(
                package_path / "execution.mjs",
                """import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { choose } from './library.js';
assert.equal(choose(true), 7);
const child = spawnSync(process.execPath, [fileURLToPath(new URL('./child.mjs', import.meta.url))], { encoding: 'utf8' });
assert.equal(child.status, 0);
assert.equal(child.stdout, '23');
""",
            )
            (
                package_path / "BUILD.bazel"
            ).write_text("""load("//tools/coverage:javascript.bzl", "js_test")
load("@aspect_rules_ts//ts:defs.bzl", "ts_project")
ts_project(name="library", srcs=["library.ts"], source_map=True, tsconfig={"compilerOptions": {"target": "ES2022", "module": "NodeNext", "moduleResolution": "NodeNext", "strict": True, "inlineSources": True}})
js_test(name="execution", size="small", entry_point="execution.mjs", data=[":library", "child.mjs", "unused.mjs"])
""")
            write(hidden / "hidden.mjs", "export const hidden = () => 19;\n")
            (hidden / "BUILD.bazel").write_text(
                'filegroup(name="sources", srcs=["hidden.mjs"], tags=["manual", "no-remote", "no-coverage"])\n'
            )
            token = package.removeprefix("coverage_js_")
            child_name = f"coverage-child-{token}.mjs"
            unused_name = f"coverage-unused-{token}.ts"
            spec_name = f"coverage-acceptance-{token}.spec.ts"
            for filename, text in [
                (
                    child_name,
                    'if (process.argv.length > 1) {\n  process.stdout.write("23");\n}\n',
                ),
                (
                    unused_name,
                    "export function disconnected(flag: boolean): number {\n  return flag ? 7 : 3;\n}\n",
                ),
                (
                    spec_name,
                    f"""/// <reference types="node" />
import {{ expect, test }} from 'vitest';
import {{ spawnSync }} from 'node:child_process';
import {{ join }} from 'node:path';
test('the declared child executes', () => {{
  const child = spawnSync(process.execPath, [join(process.env['TEST_SRCDIR'] ?? '', process.env['TEST_WORKSPACE'] ?? '', 'angular/projects/nicknamer/src/{child_name}')], {{ encoding: 'utf8' }});
  expect(child.status).toBe(0);
  expect(child.stdout).toBe('23');
}});
""",
                ),
            ]:
                path = angular / filename
                created.append(path)
                write(path, text)
            run(
                "test",
                f"//{package}:execution",
                "//angular/projects/nicknamer:test",
                "--nocache_test_results",
            )
            found = cover(
                [
                    f"//{package}:execution",
                    "//angular/projects/nicknamer:test",
                    "//tools/coverage:vitest_adapter_test",
                ],
                "measured",
            )
            for source in [
                f"{package}/child.mjs",
                f"angular/projects/nicknamer/src/{child_name}",
            ]:
                assert any(
                    line.startswith("DA:") and int(line.split(",")[1]) > 0
                    for line in found[source].splitlines()
                ), source
            for source in [
                f"{package}/unused.mjs",
                str((hidden / "hidden.mjs").relative_to(root)),
                f"angular/projects/nicknamer/src/{unused_name}",
            ]:
                counts = [
                    int(line.split(",")[1])
                    for line in found[source].splitlines()
                    if line.startswith("DA:")
                ]
                assert counts and all(count == 0 for count in counts), source
            receipt = json.loads(
                (root / "coverage-javascript/receipt.json").read_text()
            )
            assert any(
                limit["source"].endswith(spec_name)
                and "test sources" in limit["missing_guarantee"]
                for limit in receipt["limits"]
            )
            native_typescript = f"{package}/library.ts"
            if native_typescript in found:
                assert any(
                    line.startswith("DA:") and int(line.split(",")[1]) > 0
                    for line in found[native_typescript].splitlines()
                )
            else:
                assert any(
                    limit["source"] == native_typescript
                    and "original-manifest" in limit["missing_guarantee"]
                    for limit in receipt["limits"]
                )
            assert any(
                line.startswith("BRDA:")
                for line in found[f"{package}/child.mjs"].splitlines()
            ), "Native branches are absent."
            assert not any(
                name.endswith(".graphql") or "/generated/graphql.ts" in name
                for name in found
            )
            assert any(name.endswith(".py") for name in found)
            assert any(name.endswith(".go") for name in found)
            assert any(name.endswith(".rs") for name in found)
            assert any(name.endswith(".java") for name in found)
            (evidence / "result.json").write_text(
                json.dumps(
                    {
                        "passed": True,
                        "commands": sequence,
                        "native_child": f"{package}/child.mjs",
                        "angular_child": child_name,
                        "report": str(evidence / "measured.lcov"),
                    },
                    indent=2,
                )
                + "\n"
            )
    finally:
        for path in created:
            path.unlink(missing_ok=True)
        run("run", "//:gazelle")


if __name__ == "__main__":
    main()
