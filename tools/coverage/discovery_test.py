"""Check coverage discovery through the existing command."""

import json
import os
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    root = Path(os.environ["BUILD_WORKSPACE_DIRECTORY"])
    with tempfile.TemporaryDirectory(
        prefix="coverage_discovery_", dir=root
    ) as directory:
        package = Path(directory)
        source = package / "undeclared.py"
        source.write_text("VALUE = 1\n")
        generated = subprocess.run(
            ["bazel", "run", "//:gazelle"], cwd=root, check=False
        )
        assert generated.returncode == 0
        (package / "BUILD.bazel").write_text('filegroup(name = "empty")\n')
        result = subprocess.run(
            ["bazel", "run", "//tools/coverage", "--", "//python_sample/..."],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
        assert result.returncode != 0, result.stdout + result.stderr
        assert "Eligible source has no Bazel owner:" in result.stdout + result.stderr, (
            result.stdout + result.stderr
        )
        assert str(source.relative_to(root)) in result.stdout + result.stderr

        generated = subprocess.run(
            ["bazel", "run", "//:gazelle"], cwd=root, check=False
        )
        assert generated.returncode == 0
        with tempfile.TemporaryDirectory(
            prefix="coverage_discovery_", dir=root / ".github"
        ) as hidden_directory:
            hidden = Path(hidden_directory)
            (hidden / "untested.py").write_text(
                "def choose(value):\n    if value:\n        return 1\n    return 0\n"
            )
            generated = subprocess.run(
                ["bazel", "run", "//:gazelle"], cwd=root, check=False
            )
            assert generated.returncode == 0
            for path in (package, hidden):
                build = path / "BUILD.bazel"
                build.write_text(
                    build.read_text().replace(
                        "srcs = [",
                        'tags = ["manual", "no-remote", "no-coverage"],\n    srcs = [',
                    )
                )
            build = package / "BUILD.bazel"
            build.write_text(
                build.read_text().replace('"undeclared.py"', '":sources"')
                + '\nfilegroup(name = "sources", srcs = ["undeclared.py"])\n'
            )
            result = subprocess.run(
                ["bazel", "run", "//tools/coverage", "--", "//python_sample/..."],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            print(result.stdout + result.stderr)
            assert result.returncode == 0, result.stdout + result.stderr
            inventory = json.loads((root / "coverage-inventory.json").read_text())
            sources = {item["path"]: item for item in inventory["eligible"]}
            assert not any(
                name.startswith("3rdparty/gazelle_rust/upstream/") for name in sources
            )
            assert "angular/projects/nicknamer2-web/src/generated/graphql.ts" in {
                item["path"] for item in inventory["excluded"]
            }
            for path in (source, hidden / "untested.py"):
                name = str(path.relative_to(root))
                assert sources[name]["owners"], sources[name]
                assert any(
                    "no-coverage" in owner["tags"] for owner in sources[name]["owners"]
                )
                report = (root / "coverage-report.lcov").read_text()
                record = next(
                    record
                    for record in report.split("end_of_record")
                    if f"SF:{name}\n" in record
                )
                assert "DA:1,0" in record, record


if __name__ == "__main__":
    main()
