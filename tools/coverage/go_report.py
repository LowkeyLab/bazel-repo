"""Apply Go scope exclusions and validate Go LCOV records."""

import argparse
import json
import os
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
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
        original = args.report.read_text()
        records = original.split("end_of_record")
        if len(records) < 2 or records[-1].strip():
            raise ValueError("The combined report is empty or incomplete.")
        retained = []
        for record in records[:-1]:
            lines = record.splitlines()
            sources = [line[3:] for line in lines if line.startswith("SF:")]
            if len(sources) != 1:
                raise ValueError("A coverage record must identify one source.")
            name = sources[0]
            if name in excluded:
                continue
            if name.endswith(".go"):
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
                if found != [len(counts)] or hit != [
                    sum(count > 0 for count in counts.values())
                ]:
                    raise ValueError(f"Incomplete Go coverage record: {name}")
            retained.append(record + "end_of_record")
        args.report.write_text("".join(retained) + records[-1])
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Go coverage report failed: {error}\n")


if __name__ == "__main__":
    main()
