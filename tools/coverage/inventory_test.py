"""Verify the inventory command with controlled Git and Bazel responses."""

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
QUERY = """<query>
<source-file name="//:library.py"/>
<source-file name="//:library.go"/>
<rule class="_go_coverage_sources" name="//:go_library.go_coverage">
  <list name="srcs"><label value="//:library.go"/></list>
</rule>
<source-file name="//.hidden:tool.py"/>
<rule class="filegroup" name="//:sources">
  <list name="srcs"><label value="//:library.py"/></list>
  <rule-input name="//:library.py"/>
</rule>
<rule class="alias" name="//:alias">
  <label name="actual" value="//:sources"/>
</rule>
<rule class="py_library" name="//:lib">
  <list name="srcs"><label value="//:alias"/></list>
  <list name="tags"><string value="manual"/><string value="no-coverage"/></list>
  <rule-input name="@@external//:dependency"/>
</rule>
<rule class="_coverage_sources" name="//:lib.python_coverage">
  <boolean name="testonly" value="false"/>
  <list name="srcs"><label value="//:alias"/></list>
  <label name="main"/>
</rule>
<rule class="py_binary" name="//.hidden:tool">
  <label name="main" value="//.hidden:tool.py"/>
  <list name="tags"><string value="no-remote"/></list>
</rule>
<rule class="_coverage_sources" name="//.hidden:tool.python_coverage">
  <label name="main" value="//.hidden:tool.py"/>
</rule>
<rule class="_coverage_sources" name="//:test_support.python_coverage">
  <boolean name="testonly" value="true"/>
  <list name="srcs"><label value="//:library.py"/></list>
</rule>
<rule class="py_library" name="@@external//:library"/>
</query>"""

PROTOCOL_TOOL = """import json
import sys
from pathlib import Path

configuration = json.loads(Path("protocol.json").read_text())
if Path(sys.argv[0]).name == "bazel":
    if configuration.get("query_exit"):
        sys.exit(configuration["query_exit"])
    print(Path("query.xml").read_text())
elif "ls-files" in sys.argv:
    sys.stdout.write("\\0".join(configuration["files"]) + "\\0")
else:
    for name in sys.stdin.read().split("\\0"):
        if name:
            scope = configuration["scopes"].get(name, "unspecified")
            sys.stdout.write(name + "\\0coverage-scope\\0" + scope + "\\0")
"""


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    files = [
        "BUILD.bazel",
        "library.py",
        "library.go",
        ".hidden/BUILD.bazel",
        ".hidden/tool.py",
        "generated.py",
        "schema.sql",
        "ignored/undeclared.py",
        ".bazelignore",
    ]
    for name in files:
        source = tmp_path / name
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text("VALUE = 1\n")
    (tmp_path / ".bazelignore").write_text("# Build output\nignored/\n")
    (tmp_path / "protocol.json").write_text(
        json.dumps({"files": files, "scopes": {"generated.py": "generated"}})
    )
    (tmp_path / "query.xml").write_text(QUERY)
    binaries = tmp_path / "bin"
    binaries.mkdir()
    interpreter = binaries / "python"
    interpreter.symlink_to(sys.executable)
    for name in ("git", "bazel"):
        tool = binaries / name
        tool.write_text(f"#!{interpreter}\n" + PROTOCOL_TOOL)
        tool.chmod(0o755)
    return tmp_path


def run_inventory(workspace: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(ROOT / "tools/coverage/inventory"),
            "--output",
            "inventory.json",
            "--roots-output",
            "roots.txt",
            "--bazel-option=--remote_cache=",
        ],
        cwd=workspace,
        env={
            **os.environ,
            "BUILD_WORKSPACE_DIRECTORY": str(workspace),
            "PATH": str(workspace / "bin") + os.pathsep + os.environ["PATH"],
        },
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )


def test_inventory_resolves_owners_and_preserves_scope(workspace: Path) -> None:
    result = run_inventory(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    inventory = json.loads((workspace / "inventory.json").read_text())
    sources = {source["path"]: source for source in inventory["eligible"]}
    assert set(sources) == {"library.py", "library.go", ".hidden/tool.py"}
    owner = next(
        item for item in sources["library.py"]["owners"] if item["label"] == "//:lib"
    )
    assert set(owner["tags"]) == {"manual", "no-coverage"}
    hidden_owner = next(
        item
        for item in sources[".hidden/tool.py"]["owners"]
        if item["label"] == "//.hidden:tool"
    )
    assert hidden_owner["tags"] == ["no-remote"]
    assert inventory["missing_owners"] == []
    excluded = {source["path"]: source["reason"] for source in inventory["excluded"]}
    assert excluded["generated.py"] == "generated"
    assert "schema.sql" in excluded
    assert (
        "ignored/undeclared.py" not in sources
        and "ignored/undeclared.py" not in excluded
    )
    assert set((workspace / "roots.txt").read_text().splitlines()) == {
        "//:lib",
        "//:go_library.go_coverage",
        "//.hidden:tool",
        "//:test_support.python_coverage",
    }
    assert set(inventory["baseline_sources"]) == {
        "library.py",
        "library.go",
        ".hidden/tool.py",
    }


def test_unowned_source_fails_and_retains_diagnostics(workspace: Path) -> None:
    protocol = workspace / "protocol.json"
    configuration = json.loads(protocol.read_text())
    configuration["files"].append("orphan.py")
    protocol.write_text(json.dumps(configuration))
    (workspace / "orphan.py").write_text("VALUE = 1\n")
    result = run_inventory(workspace)
    assert result.returncode != 0
    assert "Eligible source has no Bazel owner: orphan.py" in result.stderr
    assert json.loads((workspace / "inventory.json").read_text())["missing_owners"] == [
        "orphan.py"
    ]


@pytest.mark.parametrize("failure", ["query", "xml", "scope", "packages"])
def test_failed_discovery_never_reports_an_empty_inventory(
    workspace: Path, failure: str
) -> None:
    protocol = workspace / "protocol.json"
    configuration = json.loads(protocol.read_text())
    if failure == "query":
        configuration["query_exit"] = 3
    elif failure == "xml":
        (workspace / "query.xml").write_text("<query>")
    elif failure == "scope":
        configuration["scopes"]["generated.py"] = "unknown"
    else:
        configuration["files"] = ["library.py"]
    protocol.write_text(json.dumps(configuration))
    result = run_inventory(workspace)
    assert result.returncode != 0
    assert "Coverage discovery failed:" in result.stderr
    assert not (workspace / "inventory.json").exists()
    assert not (workspace / "roots.txt").exists()


def test_jvm_candidates_preserve_independent_source_scope(workspace: Path) -> None:
    protocol = workspace / "protocol.json"
    configuration = json.loads(protocol.read_text())
    configuration["files"].extend(["Library.java", "excluded.clj"])
    protocol.write_text(json.dumps(configuration))
    (workspace / "Library.java").write_text("public class Library {}\n")
    (workspace / "excluded.clj").write_text("(ns excluded)\n")
    extra = """
<source-file name="//:Library.java"/>
<source-file name="//:excluded.clj"/>
<rule class="java_library" name="//:java_lib">
  <list name="srcs"><label value="//:Library.java"/></list>
</rule>
<rule class="alias" name="//:java_alias">
  <label name="actual" value="//:java_lib"/>
</rule>
<rule class="java_test" name="//:java_test">
  <list name="deps"><label value="//:java_alias"/></list>
</rule>
<rule class="clojure_library" name="//:clj_lib">
  <list name="srcs"><label value="//:excluded.clj"/></list>
</rule>
<rule class="java_test" name="//:clojure_test">
  <list name="deps"><label value="//:clj_lib"/></list>
</rule>
"""
    query = workspace / "query.xml"
    query.write_text(query.read_text().replace("</query>", extra + "</query>"))
    result = run_inventory(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    inventory = json.loads((workspace / "inventory.json").read_text())
    assert inventory["jvm_tests"] == ["//:clojure_test", "//:java_test"]
    assert any(source["path"] == "Library.java" for source in inventory["eligible"])
    assert any(source["path"] == "excluded.clj" for source in inventory["excluded"])
