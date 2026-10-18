"""Generate zero-hit LCOV from Bazel-declared production sources."""

import sys
from pathlib import Path

import coverage


def main() -> None:
    output, *sources = sys.argv[1:]
    collector = coverage.Coverage(config_file=False, branch=True, data_file=None)
    data = collector.get_data()
    data.add_arcs({})
    data.touch_files(sources)
    temporary = output + ".tmp"
    collector.lcov_report(morfs=sources, outfile=temporary)
    with Path(output).open("w") as report:
        for line in Path(temporary).read_text().splitlines(keepends=True):
            if line.startswith("FN:"):
                fields = line.rstrip().split(",")
                if len(fields) == 3:
                    line = fields[0] + "," + fields[2] + "\n"
            report.write(line)
    Path(temporary).unlink()


if __name__ == "__main__":
    main()
