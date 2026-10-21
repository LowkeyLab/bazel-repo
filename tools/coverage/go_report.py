"""Apply Go scope exclusions and validate Go LCOV records."""

import argparse
import json
import os
from pathlib import Path


def go_lines(lines: list[str], name: str) -> set[int]:
    counts: dict[int, int] = {}
    for line in lines:
        if line.startswith("DA:"):
            fields = line[3:].split(",")
            if len(fields) not in (2, 3):
                raise ValueError(f"Invalid Go line record: {name}")
            number, count = int(fields[0]), int(fields[1])
            if number <= 0 or count < 0 or number in counts:
                raise ValueError(f"Invalid Go line record: {name}")
            counts[number] = count
    found = [int(line[3:]) for line in lines if line.startswith("LF:")]
    hit = [int(line[3:]) for line in lines if line.startswith("LH:")]
    if found != [len(counts)] or hit != [sum(count > 0 for count in counts.values())]:
        raise ValueError(f"Incomplete Go coverage record: {name}")
    return set(counts)


def baseline_lines(
    events_path: Path, roots: set[str], root: Path, excluded: set[str]
) -> dict[str, set[int]]:
    events = [json.loads(line) for line in events_path.read_text().splitlines()]
    named_sets = {
        event["id"]["namedSet"]["id"]: event["namedSetOfFiles"]
        for event in events
        if "namedSetOfFiles" in event
    }

    def files(identifier: str, active: set[str]) -> list[Path]:
        if identifier in active:
            raise ValueError("A Go baseline artifact set contains a cycle.")
        entry = named_sets[identifier]
        paths = [
            root.joinpath(*file.get("pathPrefix", []), file["name"])
            for file in entry.get("files", [])
        ]
        for child in entry.get("fileSets", []):
            paths.extend(files(child["id"], active | {identifier}))
        return paths

    seen = set()
    expected: dict[str, set[int]] = {}
    for event in events:
        label = event.get("id", {}).get("targetCompleted", {}).get("label")
        if label not in roots:
            continue
        completed = event["completed"]
        if not completed.get("success"):
            raise ValueError(f"Go baseline root failed: {label}")
        paths = []
        for group in completed.get("outputGroup", []):
            if group["name"] == "go_coverage_baselines":
                for entry in group.get("fileSets", []):
                    paths.extend(files(entry["id"], set()))
        if not paths:
            raise ValueError(f"Missing Go baseline artifact: {label}")
        seen.add(label)
        for path in paths:
            text = path.read_text()
            if not text.strip():
                continue
            records = text.split("end_of_record")
            if len(records) < 2 or records[-1].strip():
                raise ValueError(f"Incomplete Go baseline artifact: {path}")
            for record in records[:-1]:
                lines = record.splitlines()
                sources = [line[3:] for line in lines if line.startswith("SF:")]
                if len(sources) != 1 or not sources[0].endswith(".go"):
                    raise ValueError(f"Invalid Go baseline source: {path}")
                name = sources[0]
                numbers = go_lines(lines, name)
                if name not in excluded:
                    expected.setdefault(name, set()).update(numbers)
    if roots - seen:
        raise ValueError(f"Missing Go baseline roots: {sorted(roots - seen)}")
    return expected


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--events", required=True, type=Path)
    parser.add_argument("--expected-output", type=Path)
    parser.add_argument("--inventory", required=True, type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd()))
    args.inventory = root / args.inventory
    args.report = root / args.report
    try:
        inventory = json.loads(args.inventory.read_text())
        excluded = {
            source["path"]
            for source in inventory["excluded"]
            if source["path"].endswith(".go")
        }
        expected = baseline_lines(
            root / args.events,
            {
                label
                for label in inventory["coverage_roots"]
                if label.endswith(".go_coverage")
            },
            root,
            excluded,
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
            if name.endswith(".go"):
                if name in observed:
                    raise ValueError(f"Duplicate Go coverage source: {name}")
                observed[name] = go_lines(lines, name)
            retained.append(record + "end_of_record")
        for name, numbers in expected.items():
            if name not in observed or not numbers.issubset(observed[name]):
                raise ValueError(f"Missing Go baseline source or lines: {name}")
        if args.expected_output is not None:
            (root / args.expected_output).write_text(
                json.dumps(
                    {
                        "sources": {
                            name: sorted(numbers)
                            for name, numbers in sorted(expected.items())
                        },
                        "roots": sorted(
                            label
                            for label in inventory["coverage_roots"]
                            if label.endswith(".go_coverage")
                        ),
                    },
                    indent=2,
                )
                + "\n"
            )
        args.report.write_text("".join(retained) + records[-1])
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Go coverage report failed: {error}\n")


if __name__ == "__main__":
    main()
