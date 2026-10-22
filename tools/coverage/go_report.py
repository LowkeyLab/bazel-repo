"""Apply native scope exclusions and validate required LCOV records."""

import argparse
import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class RustObjectMetadata:
    path: Path
    object_path: str
    no_mapping: bool
    diagnostic: str
    generated_sources: frozenset[str]
    children: tuple[tuple[str, str, str], ...]


def read_rust_object(path: Path) -> RustObjectMetadata:
    data = json.loads(path.read_text())
    if not isinstance(data, dict):
        raise TypeError(f"Invalid Rust object evidence: {path}")
    object_path = data.get("object")
    no_mapping = data.get("no_mapping")
    diagnostic = data.get("diagnostic")
    generated = data.get("generated_sources")
    children = data.get("children")
    if (
        not isinstance(object_path, str)
        or not object_path
        or not isinstance(no_mapping, bool)
        or not isinstance(diagnostic, str)
        or not isinstance(generated, list)
        or not all(isinstance(name, str) for name in generated)
        or any(
            not isinstance(data.get(name), str) or not data[name]
            for name in ("llvm_cov", "llvm_profdata")
        )
        or not isinstance(children, list)
        or any(
            not isinstance(child, list)
            or len(child) != 3
            or not all(isinstance(value, str) and value for value in child)
            for child in children
        )
        or (no_mapping and "no coverage data found" not in diagnostic)
    ):
        raise ValueError(f"Invalid Rust object evidence: {path}")
    return RustObjectMetadata(
        path,
        object_path,
        no_mapping,
        diagnostic,
        frozenset(generated),
        tuple((child[0], child[1], child[2]) for child in children),
    )


def lcov_lines(
    lines: list[str], name: str, family: str = "Go", check_summary: bool = True
) -> set[int]:
    counts: dict[int, int] = {}
    for line in lines:
        if line.startswith("DA:"):
            fields = line[3:].split(",")
            if len(fields) not in (2, 3):
                raise ValueError(f"Invalid {family} line record: {name}")
            number, count = int(fields[0]), int(fields[1])
            if number <= 0 or count < 0 or number in counts:
                raise ValueError(f"Invalid {family} line record: {name}")
            counts[number] = count
    found = [int(line[3:]) for line in lines if line.startswith("LF:")]
    hit = [int(line[3:]) for line in lines if line.startswith("LH:")]
    if (
        len(found) != 1
        or len(hit) != 1
        or found[0] < len(counts)
        or not 0 <= hit[0] <= found[0]
    ):
        raise ValueError(f"Incomplete {family} coverage record: {name}")
    if check_summary and (
        found != [len(counts)] or hit != [sum(count > 0 for count in counts.values())]
    ):
        raise ValueError(f"Incomplete {family} coverage record: {name}")
    return set(counts)


def baseline_lines(
    events_path: Path,
    roots: set[str],
    root: Path,
    excluded: set[str],
    language: str,
    limits: list[dict[str, object]],
    declarations: dict[str, set[str]],
) -> dict[str, set[int]]:
    family = language.title()
    extension = ".rs" if language == "rust" else ".go"
    events = [json.loads(line) for line in events_path.read_text().splitlines()]
    named_sets = {
        event["id"]["namedSet"]["id"]: event["namedSetOfFiles"]
        for event in events
        if "namedSetOfFiles" in event
    }

    def files(identifier: str, active: set[str]) -> list[Path]:
        if identifier in active:
            raise ValueError(f"A {family} baseline artifact set contains a cycle.")
        entry = named_sets[identifier]
        paths = []
        for file in entry.get("files", []):
            path = root.joinpath(*file.get("pathPrefix", []), file["name"])
            contents = path.read_bytes()
            if file.get("digest") != hashlib.sha256(contents).hexdigest() or int(
                file.get("length", 0)
            ) != len(contents):
                raise ValueError(
                    f"Invalid {family} baseline artifact integrity: {path}"
                )
            paths.append(path)
        for child in entry.get("fileSets", []):
            paths.extend(files(child["id"], active | {identifier}))
        return paths

    seen = set()
    expected: dict[str, set[int]] = {}
    for event in events:
        label = event.get("id", {}).get("targetCompleted", {}).get("label")
        if label not in roots or event["id"]["targetCompleted"].get("aspect"):
            continue
        completed = event["completed"]
        if not completed.get("success"):
            raise ValueError(f"{family} baseline root failed: {label}")
        paths = []
        for group in completed.get("outputGroup", []):
            if group["name"] == f"{language}_coverage_baselines":
                if group.get("incomplete"):
                    raise ValueError(
                        f"Incomplete {family} baseline output group: {label}"
                    )
                for entry in group.get("fileSets", []):
                    paths.extend(files(entry["id"], set()))
        if not paths:
            raise ValueError(f"Missing {family} baseline artifact: {label}")
        seen.add(label)
        metadata = None
        if language == "rust":
            metadata_paths = [path for path in paths if path.suffix == ".json"]
            if len(metadata_paths) != 1:
                raise ValueError(f"Missing Rust object evidence: {label}")
            metadata = read_rust_object(metadata_paths[0])
            excluded.update(metadata.generated_sources)
            for source, child_rule, child_object in metadata.children:
                limits.append(
                    {
                        "source": source,
                        "rule": child_rule,
                        "object": child_object,
                        "collection_rule": label,
                        "missing_guarantee": "LLVM profiles cannot distinguish an unexecuted child from a child whose complete profile disappeared.",
                        "evidence": str(metadata.path),
                        "diagnostic": "The declared child object does not prove execution. Available profile IDs establish only observed executions.",
                    }
                )
            paths = [path for path in paths if path.suffix == ".lcov"]
            if len(paths) != 1:
                raise ValueError(f"Missing Rust baseline artifact: {label}")
        mapped = set()
        for path in paths:
            text = path.read_text()
            if not text.strip():
                if metadata is not None and not metadata.no_mapping:
                    raise ValueError(f"Empty required Rust baseline: {label}")
                continue
            if metadata is not None and metadata.no_mapping:
                raise ValueError(f"Conflicting Rust mapping evidence: {label}")
            records = text.split("end_of_record")
            if len(records) < 2 or records[-1].strip():
                raise ValueError(f"Incomplete {family} baseline artifact: {path}")
            for record in records[:-1]:
                lines = record.splitlines()
                sources = [line[3:] for line in lines if line.startswith("SF:")]
                if len(sources) != 1 or not sources[0].endswith(extension):
                    raise ValueError(f"Invalid {family} baseline source: {path}")
                name = sources[0]
                if metadata is not None and name in excluded:
                    mapped.add(name)
                    continue
                numbers = lcov_lines(
                    lines, name, family, check_summary=language != "rust"
                )
                if metadata is not None and int(
                    next(line[3:] for line in lines if line.startswith("LF:"))
                ) > len(numbers):
                    limits.append(
                        {
                            "source": name,
                            "rule": label,
                            "object": metadata.object_path,
                            "line_summary": True,
                            "missing_guarantee": "LLVM LCOV does not identify every summarized executable line with a DA record.",
                            "evidence": str(path),
                            "diagnostic": "The LLVM LF summary exceeds its exported DA records.",
                        }
                    )
                mapped.add(name)
                if name not in excluded:
                    expected.setdefault(name, set()).update(numbers)
        if language == "go":
            for source in sorted(declarations.get(label, set()) - mapped):
                limits.append(
                    {
                        "source": source,
                        "rule": label,
                        "record_absence": True,
                        "evidence": [str(path) for path in paths],
                        "missing_guarantee": "The configured Go source selection omits this declaration. The native builder emits no baseline record for this configuration.",
                    }
                )
        if metadata is not None:
            for source in sorted(declarations.get(label, set()) - mapped):
                limits.append(
                    {
                        "source": source,
                        "rule": label,
                        "object": metadata.object_path,
                        "record_absence": True,
                        "missing_guarantee": "LLVM emitted no source mapping in this configured object.",
                        "evidence": str(metadata.path),
                        "diagnostic": metadata.diagnostic,
                    }
                )
    if roots - seen:
        raise ValueError(f"Missing {family} baseline roots: {sorted(roots - seen)}")
    if language == "rust":
        expected = {
            name: lines for name, lines in expected.items() if name not in excluded
        }
        limits[:] = [limit for limit in limits if limit["source"] not in excluded]
    return expected


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--language", choices=["go", "rust"], default="go")
    parser.add_argument("--events", required=True, type=Path)
    parser.add_argument("--expected-output", type=Path)
    parser.add_argument("--inventory", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    family = args.language.title()
    extension = ".rs" if args.language == "rust" else ".go"
    limits = []
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd()))
    args.inventory = root / args.inventory
    args.report = root / args.report
    try:
        inventory = json.loads(args.inventory.read_text())
        excluded = {
            source["path"]
            for source in inventory["excluded"]
            if source["path"].endswith(extension)
        }
        expected = baseline_lines(
            root / args.events,
            {
                label
                for label in inventory["coverage_roots"]
                if label.endswith(f".{args.language}_coverage")
            },
            root,
            excluded,
            args.language,
            limits,
            {
                label: {
                    source["path"]
                    for source in inventory.get("eligible", [])
                    if source["language"] == args.language
                    and any(owner["label"] == label for owner in source["owners"])
                }
                for label in inventory["coverage_roots"]
            },
        )
        original = args.report.read_text()
        records = original.split("end_of_record")
        if len(records) < 2 or records[-1].strip():
            raise ValueError("The combined report is empty or incomplete.")
        retained = []
        observed: dict[str, set[int]] = {}
        for record in records[:-1]:
            lines = record.splitlines()
            sources = [line[3:] for line in lines if line.startswith("SF:")]
            if len(sources) != 1:
                raise ValueError("A coverage record must identify one source.")
            name = sources[0]
            if name in excluded:
                continue
            if name.endswith(extension):
                if name in observed:
                    raise ValueError(f"Duplicate {family} coverage source: {name}")
                observed[name] = lcov_lines(lines, name, family)
            retained.append(record + "end_of_record")
        for name, numbers in expected.items():
            if name not in observed or not numbers.issubset(observed[name]):
                raise ValueError(f"Missing {family} baseline source or lines: {name}")
        if args.expected_output is not None:
            (root / args.expected_output).write_text(
                json.dumps(
                    {
                        "limits": limits,
                        "sources": {
                            name: sorted(numbers)
                            for name, numbers in sorted(expected.items())
                        },
                        "roots": sorted(
                            label
                            for label in inventory["coverage_roots"]
                            if label.endswith(f".{args.language}_coverage")
                        ),
                    },
                    indent=2,
                )
                + "\n"
            )
        args.report.write_text("".join(retained) + records[-1])
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"{family} coverage report failed: {error}\n")


if __name__ == "__main__":
    main()
