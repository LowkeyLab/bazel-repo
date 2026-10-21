"""Export an LLVM baseline and retain the collector's mapping evidence."""

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--llvm-cov", required=True)
    parser.add_argument("--llvm-ar", required=True)
    parser.add_argument("--llvm-profdata", required=True)
    parser.add_argument("--generated-source", action="append", default=[])
    parser.add_argument("--object", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--metadata", required=True, type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory() as temporary:
        objects = [args.object]
        if args.object.endswith(".rlib"):
            members = subprocess.check_output(
                [args.llvm_ar, "t", args.object], text=True
            ).splitlines()
            objects = []
            for index, member in enumerate(members):
                if not member.endswith(".o"):
                    continue
                output = Path(temporary) / f"{index}.o"
                output.write_bytes(
                    subprocess.check_output([args.llvm_ar, "p", args.object, member])
                )
                objects.append(str(output))
        if not objects:
            parser.exit(1, "The Rust archive contains no object files.\n")
        result = subprocess.run(
            [
                args.llvm_cov,
                "export",
                "-empty-profile",
                "-format=lcov",
                "-ignore-filename-regex=(.*external/.+|/rustc/.*)",
                objects[0],
                *["-object=" + name for name in objects[1:]],
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        missing_mapping = result.returncode != 0 and result.stderr.splitlines() == [
            f"error: failed to load coverage: '{objects[0]}': no coverage data found",
            "error: could not load coverage information",
        ]
        if result.returncode != 0 and not missing_mapping:
            parser.exit(1, result.stderr)
    lines = []
    for line in result.stdout.splitlines():
        if line.startswith("SF:"):
            name = line[3:]
            for prefix in ("#/proc/self/cwd/", "/proc/self/cwd/", os.getcwd() + "/"):
                name = name.removeprefix(prefix)
            name = os.path.normpath(name)
            if Path(name).is_absolute() or ".." in Path(name).parts:
                parser.exit(
                    1, f"Rust baseline has an unsupported source path: {name}\n"
                )
            line = "SF:" + name
        lines.append(line)
    args.output.write_text("\n".join(lines) + ("\n" if lines else ""))
    args.metadata.write_text(
        json.dumps(
            {
                "object": args.object,
                "llvm_cov": args.llvm_cov,
                "llvm_profdata": args.llvm_profdata,
                "generated_sources": sorted(set(args.generated_source)),
                "no_mapping": missing_mapping,
                "diagnostic": result.stderr,
            }
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
