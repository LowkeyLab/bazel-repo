"""Discover maintained sources and their Bazel declarations."""

import argparse
import json
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path
from typing import TypedDict

LANGUAGES = {
    ".py": "python",
    ".go": "go",
    ".rs": "rust",
    ".java": "jvm",
    ".kt": "jvm",
    ".kts": "jvm",
    ".js": "javascript",
    ".mjs": "javascript",
    ".cjs": "javascript",
    ".jsx": "javascript",
    ".ts": "javascript",
    ".mts": "javascript",
    ".cts": "javascript",
    ".tsx": "javascript",
    ".astro": "browser",
}


class SourceOwner(TypedDict):
    label: str
    kind: str
    tags: list[str]
    testonly: bool


class CoverageSource(TypedDict):
    path: str
    language: str
    owners: list[SourceOwner]


class ScopeExclusion(TypedDict):
    path: str
    reason: str


class CoverageInventory(TypedDict):
    schema_version: int
    eligible: list[CoverageSource]
    excluded: list[ScopeExclusion]
    missing_owners: list[str]
    coverage_roots: list[str]
    baseline_sources: list[str]


def run(root: Path, arguments: list[str]) -> str:
    return subprocess.run(
        arguments, cwd=root, text=True, stdout=subprocess.PIPE, check=True
    ).stdout


def source_path(label: str) -> str | None:
    if not label.startswith("//") or ":" not in label:
        return None
    package, name = label[2:].split(":", 1)
    return f"{package}/{name}" if package else name


def discover(root: Path, options: list[str]) -> CoverageInventory:
    ignored_file = root / ".bazelignore"
    ignored = (
        [
            line.split("#", 1)[0].strip().rstrip("/")
            for line in ignored_file.read_text().splitlines()
        ]
        if ignored_file.exists()
        else []
    )
    files = sorted(
        set(
            run(
                root,
                ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            ).split("\0")
        )
        - {""}
    )
    files = [
        name
        for name in files
        if (root / name).is_file()
        and not any(
            name == prefix or name.startswith(prefix + "/")
            for prefix in ignored
            if prefix
        )
    ]
    packages = sorted(
        {
            "" if Path(name).parent == Path(".") else str(Path(name).parent)
            for name in files
            if Path(name).name in ("BUILD", "BUILD.bazel")
        }
    )
    if not packages:
        raise ValueError("No Bazel packages were discovered.")
    query = (
        "set(" + " ".join(json.dumps(f"//{package}:*") for package in packages) + ")"
    )
    graph = ET.fromstring(
        run(root, ["bazel", "query", *options, query, "--output=xml"])
    )
    owners: defaultdict[str, list[str]] = defaultdict(list)
    rules: dict[str, SourceOwner] = {}
    for rule in graph.findall("rule"):
        label = rule.attrib["name"]
        if not label.startswith("//"):
            continue
        tags = [
            item.attrib["value"] for item in rule.findall('./list[@name="tags"]/string')
        ]
        testonly = rule.find('./boolean[@name="testonly"]')
        rules[label] = {
            "label": label,
            "kind": rule.attrib["class"],
            "tags": tags,
            "testonly": testonly is not None and testonly.attrib["value"] == "true",
        }
        for reference in rule.findall("./rule-input"):
            path = source_path(reference.attrib["name"])
            if path is not None:
                owners[path].append(label)
    attributes = (
        subprocess.run(
            ["git", "check-attr", "-z", "--stdin", "coverage-scope"],
            cwd=root,
            input="\0".join(files) + "\0",
            text=True,
            stdout=subprocess.PIPE,
            check=True,
        )
        .stdout.rstrip("\0")
        .split("\0")
    )
    scopes = dict(zip(attributes[::3], attributes[2::3]))
    for name, scope in scopes.items():
        if scope not in ("unspecified", "vendored", "generated", "non-executable"):
            raise ValueError(f"Invalid coverage scope for {name}: {scope}")
    elements = {rule.attrib["name"]: rule for rule in graph.findall("rule")}
    source_files = {item.attrib["name"] for item in graph.findall("source-file")}

    def declared_sources(label: str, active: set[str]) -> set[str]:
        path = source_path(label)
        if path is not None and label in source_files and path in files:
            return {path}
        if label not in elements or label in active:
            return set()
        references = []
        element = elements[label]
        for attribute in ("srcs", "main", "src", "actual"):
            references.extend(
                item.attrib["value"]
                for item in element.findall(f'./list[@name="{attribute}"]/label')
            )
            reference = element.find(f'./label[@name="{attribute}"]')
            if reference is not None and "value" in reference.attrib:
                references.append(reference.attrib["value"])
        sources = set()
        for reference in references:
            sources.update(declared_sources(reference, active | {label}))
        return sources

    roots = []
    baseline_sources = set()
    for label, rule in rules.items():
        resolved = declared_sources(label, set())
        for source in resolved:
            owners[source].append(label)

        if rule["kind"] == "_coverage_sources":
            sources = {
                name
                for name in resolved
                if name.endswith(".py") and scopes.get(name) == "unspecified"
            }
            if sources:
                roots.append(
                    label
                    if rule["testonly"]
                    else label.removesuffix(".python_coverage")
                )
                baseline_sources.update(sources)
    eligible: list[CoverageSource] = []
    excluded: list[ScopeExclusion] = []
    missing = []
    for name in files:
        language = LANGUAGES.get(Path(name).suffix)
        scope = scopes.get(name, "unspecified")
        if language is None or scope != "unspecified":
            reason = (
                scope
                if scope != "unspecified"
                else "excluded language or non-executable content"
            )
            excluded.append({"path": name, "reason": reason})
            continue
        declarations = [rules[label] for label in sorted(set(owners[name]))]
        eligible.append({"path": name, "language": language, "owners": declarations})
        if not declarations:
            missing.append(name)
    return {
        "schema_version": 1,
        "eligible": eligible,
        "excluded": excluded,
        "missing_owners": missing,
        "coverage_roots": sorted(roots),
        "baseline_sources": sorted(baseline_sources),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots-output", type=Path)
    parser.add_argument("--bazel-option", action="append", default=[])
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd()))
    try:
        inventory = discover(root, args.bazel_option)
        args.output = root / args.output
        if args.roots_output:
            (root / args.roots_output).write_text(
                "".join(label + "\n" for label in inventory["coverage_roots"])
            )
        args.output.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n")
        for source in inventory["missing_owners"]:
            print(f"Eligible source has no Bazel owner: {source}", file=sys.stderr)
        if inventory["missing_owners"]:
            sys.exit(1)
    except (OSError, ValueError, ET.ParseError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Coverage discovery failed: {error}\n")


if __name__ == "__main__":
    main()
