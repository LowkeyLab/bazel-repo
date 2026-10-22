"""Check final coverage membership against the independent inventory."""

import argparse
import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import NotRequired, TypedDict

import coverage


class Source(TypedDict):
    path: str
    language: str
    owners: list[dict[str, object]]


class Inventory(TypedDict):
    eligible: list[Source]
    excluded: list[dict[str, str]]
    missing_owners: list[str]
    baseline_sources: list[str]


class Limit(TypedDict, total=False):
    source: str
    rule: str
    rules: list[str]
    owners: list[dict[str, object]]
    missing_guarantee: str
    evidence: str | list[str]
    record_absence: bool
    line_summary: bool


class CollectorReceipt(TypedDict):
    sources: dict[str, list[int]]
    limits: list[Limit]


class SourceResult(TypedDict, total=False):
    source: str
    status: str
    evidence: str
    executable_lines: int
    hit_lines: int
    limits: list[Limit]


class Reconciliation(TypedDict):
    schema_version: int
    sources: list[SourceResult]
    excluded: list[dict[str, str]]
    inputs: NotRequired[dict[str, dict[str, str]]]


@dataclass(frozen=True)
class Record:
    lines: dict[int, int]
    found: int
    hit: int


def line_set(value: object) -> set[int]:
    if not isinstance(value, list):
        raise TypeError("A collector line set must be a list.")
    result: set[int] = set()
    for number in value:
        if type(number) is not int or number <= 0 or number in result:
            raise ValueError(
                "A collector line set contains invalid or duplicate lines."
            )
        result.add(number)
    return result


def validate_branches(lines: list[str], name: str) -> None:
    branches = {}
    for line in lines:
        if not line.startswith("BRDA:"):
            continue
        fields = line[5:].split(",")
        if len(fields) != 4:
            raise ValueError(f"Invalid branch record: {name}")
        number, block, branch, count = fields
        key = (int(number), block, branch)
        value = 0 if count == "-" else int(count)
        if key[0] <= 0 or not block or not branch or value < 0 or key in branches:
            raise ValueError(f"Invalid branch record: {name}")
        branches[key] = value
    found = [int(line[4:]) for line in lines if line.startswith("BRF:")]
    hit = [int(line[4:]) for line in lines if line.startswith("BRH:")]
    if (branches or found or hit) and (
        found != [len(branches)]
        or hit != [sum(count > 0 for count in branches.values())]
    ):
        raise ValueError(f"Incomplete branch record: {name}")


def records(report: str) -> dict[str, Record]:
    parts = report.split("end_of_record")
    if len(parts) < 2 or parts[-1].strip():
        raise ValueError("The combined report is empty or incomplete.")
    result = {}
    for part in parts[:-1]:
        lines = part.splitlines()
        names = [line[3:] for line in lines if line.startswith("SF:")]
        if len(names) != 1 or not names[0] or names[0] in result:
            raise ValueError("A coverage record must identify one unique source.")
        name = names[0]
        counts = {}
        for line in lines:
            if line.startswith("DA:"):
                fields = line[3:].split(",")
                if len(fields) not in (2, 3):
                    raise ValueError(f"Invalid executable line: {name}")
                number, count = int(fields[0]), int(fields[1])
                if number <= 0 or count < 0 or number in counts:
                    raise ValueError(f"Invalid executable line: {name}")
                counts[number] = count
        found = [int(line[3:]) for line in lines if line.startswith("LF:")]
        hit = [int(line[3:]) for line in lines if line.startswith("LH:")]
        if (
            len(found) != 1
            or len(hit) != 1
            or found[0] < len(counts)
            or not 0 <= hit[0] <= found[0]
        ):
            raise ValueError(f"Invalid executable summary: {name}")
        if not name.endswith(".rs") and (
            found[0] != len(counts)
            or hit[0] != sum(value > 0 for value in counts.values())
        ):
            raise ValueError(f"Incomplete executable record: {name}")
        validate_branches(lines, name)
        result[name] = Record(counts, found[0], hit[0])
    return result


def reconcile(
    inventory: Inventory, report: str, receipts: dict[str, CollectorReceipt]
) -> Reconciliation:
    observed = records(report)
    excluded = {source["path"] for source in inventory["excluded"]}
    if excluded.intersection(observed):
        raise ValueError(
            f"Excluded sources remain in coverage: {sorted(excluded.intersection(observed))}"
        )
    collector = coverage.Coverage(config_file=False, branch=True, data_file=None)
    results: list[SourceResult] = []
    for source in inventory["eligible"]:
        name = source["path"]
        if inventory["missing_owners"] or not source["owners"]:
            raise ValueError(f"Eligible source has no Bazel owner: {name}")
        family = "javascript" if source["language"] == "browser" else source["language"]
        receipt = receipts.get(family, {"sources": {}, "limits": []})
        if family != "python" and family not in receipts:
            raise ValueError(f"Missing required {family} receipt: {name}")
        limits = [
            limit for limit in receipt.get("limits", []) if limit["source"] == name
        ]
        record_limits = [
            limit for limit in limits if limit.get("record_absence") is True
        ]
        for limit in limits:
            if (
                not limit.get("missing_guarantee")
                or not limit.get("evidence")
                or not (limit.get("rule") or limit.get("rules") or limit.get("owners"))
            ):
                raise ValueError(f"Incomplete source limit: {name}")
        expected = line_set(receipt.get("sources", {}).get(name, []))
        if family == "python":
            if name not in inventory.get("baseline_sources", []):
                raise ValueError(f"Missing Python instrumentation root: {name}")
            expected = set(collector.analysis2(name)[1])
            if not expected:
                if name in observed and observed[name].lines:
                    raise ValueError(f"Unexpected Python executable lines: {name}")
                results.append(
                    {
                        "source": name,
                        "status": "no_executable_lines",
                        "evidence": "coverage.Coverage.analysis2",
                        "executable_lines": 0,
                        "hit_lines": 0,
                    }
                )
                continue
        required = family == "python" or name in receipt.get("sources", {})
        if not required and not record_limits:
            raise ValueError(f"Missing configured source evidence: {name}")
        if name not in observed:
            if required or not record_limits:
                raise ValueError(f"Unexplained missing source record: {name}")
            results.append(
                {"source": name, "status": "collector_limit", "limits": record_limits}
            )
            continue
        record = observed[name]
        known_hits = sum(count > 0 for count in record.lines.values())
        unidentified_lines = record.found - len(record.lines)
        if not known_hits <= record.hit <= known_hits + unidentified_lines:
            raise ValueError(f"The executable summary contradicts known hits: {name}")
        if (record.found != len(record.lines) or record.hit != known_hits) and not (
            family == "rust"
            and any(limit.get("line_summary") is True for limit in limits)
        ):
            raise ValueError(f"Unexplained executable summary gap: {name}")
        if not expected.issubset(record.lines):
            raise ValueError(f"Missing required executable lines: {name}")
        results.append(
            {
                "source": name,
                "status": (
                    "measured"
                    if known_hits
                    else ("collector_limit" if record.hit else "zero_hit")
                )
                if record.lines
                else ("collector_limit" if limits else "no_executable_lines"),
                "executable_lines": len(record.lines),
                "hit_lines": known_hits,
                "limits": limits,
            }
        )
    return {"schema_version": 1, "sources": results, "excluded": inventory["excluded"]}


def filter_scope(report: str, excluded: set[str]) -> str:
    records(report)
    parts = report.split("end_of_record")
    retained = []
    for part in parts[:-1]:
        name = next(line[3:] for line in part.splitlines() if line.startswith("SF:"))
        if name not in excluded:
            retained.append(part + "end_of_record")
    return "".join(retained) + parts[-1]


def main() -> None:
    os.chdir(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd()))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    for family in ("go", "rust", "jvm", "javascript"):
        parser.add_argument(f"--{family}", type=Path)
    args = parser.parse_args()
    try:
        args.output.unlink(missing_ok=True)
        receipts: dict[str, CollectorReceipt] = {
            family: json.loads(path.read_text())
            for family in ("go", "rust", "jvm", "javascript")
            if (path := getattr(args, family)) is not None
        }
        inventory = json.loads(args.inventory.read_text())
        report = filter_scope(
            args.report.read_text(),
            {source["path"] for source in inventory["excluded"]},
        )
        result = reconcile(inventory, report, receipts)
        args.report.write_text(report)
        result["inputs"] = {
            name: {
                "path": str(path),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            for name in ("inventory", "report", "go", "rust", "jvm", "javascript")
            if (path := getattr(args, name)) is not None
        }
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        AttributeError,
        coverage.exceptions.CoverageException,
    ) as error:
        parser.exit(1, f"Coverage reconciliation failed: {error}\n")


if __name__ == "__main__":
    main()
