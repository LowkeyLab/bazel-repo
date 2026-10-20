"""Validate a bounded statement-event profile and write diagnostic LCOV."""

import argparse
import collections
import json
from pathlib import Path


def convert(inventory_path: Path, events_path: Path, root: Path, output: Path) -> None:
    inventory = json.loads(inventory_path.read_text())
    events = json.loads(events_path.read_text())
    if not isinstance(inventory, dict) or not inventory:
        raise ValueError("The statement inventory is missing")
    if not isinstance(events, list) or not events:
        raise ValueError("The measured profile has no events")
    counts: dict[str, collections.Counter[int]] = {}
    for source, lines in inventory.items():
        if not isinstance(source, str) or not isinstance(lines, list):
            raise TypeError("The statement inventory is invalid")
        if any(
            not isinstance(line, int) or isinstance(line, bool) or line < 1
            for line in lines
        ):
            raise ValueError("A statement location is invalid")
        counts[source] = collections.Counter(dict.fromkeys(lines, 0))
    for event in events:
        if not isinstance(event, dict):
            raise TypeError("An event is invalid")
        path = event.get("path")
        line = event.get("line")
        if (
            not isinstance(path, str)
            or not isinstance(line, int)
            or isinstance(line, bool)
        ):
            raise TypeError("An event location is invalid")
        source = Path(path).relative_to(root).as_posix()
        if source not in counts or line not in counts[source]:
            raise ValueError("An event has no matching original statement")
        counts[source][line] += 1
    required = {
        "tools/python/defs.bzl": [12, 15, 22, 35],
        "tools/research_starlark/phase.bzl": [3, 6, 8, 14],
    }
    for source, lines in required.items():
        if any(counts.get(source, {}).get(line, 0) <= 0 for line in lines):
            raise ValueError("An expected statement-entry event is missing")
    records = []
    for source, hits in sorted(counts.items()):
        records.extend(["TN:", f"SF:{source}"])
        records.extend(f"DA:{line},{count}" for line, count in sorted(hits.items()))
        records.extend(
            [
                f"LF:{len(hits)}",
                f"LH:{sum(count > 0 for count in hits.values())}",
                "end_of_record",
            ]
        )
    output.write_text("\n".join(records) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    convert(args.inventory, args.events, args.root.resolve(), args.output)


if __name__ == "__main__":
    main()
