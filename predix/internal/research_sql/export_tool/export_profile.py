"""Check bounded database receipts and export diagnostic original-source LCOV."""

import argparse
import hashlib
import json
from pathlib import Path
from typing import cast


def read_json(directory: Path, name: str) -> object:
    return json.loads((directory / name).read_text())


def require_rows(value: object, name: str) -> list[dict[str, object]]:
    if not isinstance(value, list) or any(not isinstance(row, dict) for row in value):
        raise TypeError(f"Invalid database row set: {name}")
    return cast(list[dict[str, object]], value)


def text(row: dict[str, object], key: str) -> str:
    value = row.get(key)
    if not isinstance(value, str):
        raise TypeError(f"The field must contain text: {key}")
    return value


def integer(row: dict[str, object], key: str) -> int:
    value = row.get(key)
    if not isinstance(value, int) or isinstance(value, bool):
        raise TypeError(f"The field must contain an integer: {key}")
    return value


def object_row(value: object, name: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise TypeError(f"The object is missing: {name}")
    return cast(dict[str, object], value)


def procedural_lines(value: object) -> set[int]:
    lines: set[int] = set()
    if isinstance(value, dict):
        for key, item in value.items():
            if key.startswith("PLpgSQL_stmt_") and isinstance(item, dict):
                line = item.get("lineno")
                if isinstance(line, int) and line > 0:
                    lines.add(line)
            lines.update(procedural_lines(item))
    elif isinstance(value, list):
        for item in value:
            lines.update(procedural_lines(item))
    return lines


def write_lcov(
    path: Path, locations: dict[str, dict[int, int]], branches: tuple[int, int]
) -> None:
    output: list[str] = []
    for source, hits in sorted(locations.items()):
        output.extend(["TN:", f"SF:{source}"])
        output.extend(f"DA:{line},{count}" for line, count in sorted(hits.items()))
        if source.endswith("profile_control.sql"):
            output.extend(
                f"BRDA:6,2,{arm},{count}" for arm, count in enumerate(branches)
            )
            output.extend(["BRF:2", f"BRH:{sum(count > 0 for count in branches)}"])
        output.extend(
            [
                f"LF:{len(hits)}",
                f"LH:{sum(count > 0 for count in hits.values())}",
                "end_of_record",
            ]
        )
    path.write_text("\n".join(output) + "\n")


def export(directory: Path, output: Path) -> None:
    inventory = object_row(read_json(directory, "inventory.json"), "inventory")
    if not isinstance(inventory, dict) or inventory.get("parser_version") != 170007:
        raise ValueError("The parser version does not match the inspected corpus")
    statements = require_rows(inventory.get("statements"), "inventory")
    mappings = object_row(
        read_json(directory, "source-mappings.json"), "source mappings"
    )
    if not isinstance(mappings, dict):
        raise TypeError("The original source mappings are missing")
    host_mapping = object_row(mappings["host_origin"], "host source mapping")
    locations: dict[str, dict[int, int]] = {}
    index: dict[tuple[str, int], dict[str, object]] = {}
    control = None
    anonymous = None
    for item in statements:
        source, line, start = (
            text(item, "path"),
            integer(item, "line"),
            integer(item, "start"),
        )
        if item["kind"] == "CreateFunctionStmt":
            control = item
            continue
        locations.setdefault(source, {})[line] = 0
        index[source, start] = item
        if item["kind"] == "DoStmt":
            anonymous = item
    if control is None or anonymous is None:
        raise ValueError("The procedural source inventory is incomplete")
    control_path = text(control, "path")
    control_lines = procedural_lines(control.get("procedural_parse"))
    if control_lines != {2, 3, 4, 6}:
        raise ValueError("The named procedural statement locations changed")
    # The first dollar delimiter is on original line 4. Body line 2 maps to line 5.
    locations[control_path] = dict.fromkeys((line + 3 for line in control_lines), 0)
    output.mkdir(parents=True, exist_ok=True)
    write_lcov(output / "baseline.dat", locations, (0, 0))
    anonymous_lines = procedural_lines(anonymous.get("procedural_parse"))
    anonymous_locations = {
        text(anonymous, "path"): dict.fromkeys(
            (line + 14 for line in anonymous_lines), 0
        )
    }
    write_lcov(
        output / "anonymous-unmeasured-baseline.dat", anonymous_locations, (0, 0)
    )
    functional = read_json(directory, "functional.json")
    server = require_rows(read_json(directory, "server.json"), "server")
    if (
        not isinstance(functional, dict)
        or functional.get("exit") != 0
        or functional.get("applications") != 2
    ):
        raise ValueError("The two functional applications did not succeed")
    if len(server) != 1 or server[0].get("server_version_num") != "180006":
        raise ValueError("The actual PostgreSQL server version is missing or different")
    if read_json(directory, "after-adapter-errors.json") not in (None, []):
        raise ValueError("The database receipt adapter reported an error")
    before = require_rows(
        read_json(directory, "before-profiler-statements.json"), "profiler baseline"
    )
    if len(before) != 4 or any(row.get("exec_stmts") is not None for row in before):
        raise ValueError("The named-function baseline contains measured execution")
    if read_json(directory, "before-profiler-functions.json") != []:
        raise ValueError("The named function executed before the baseline")
    if {integer(row, "lineno") for row in before} != control_lines:
        raise ValueError("Native procedural metadata disagrees with the parser")
    expected: set[tuple[str, int]] = set()
    for key, item in index.items():
        source, _ = key
        if source.endswith("statements.sql") and item["kind"] != "DeleteStmt":
            expected.add(key)
        if source == mappings[
            "generated_origin"
        ] and "WHERE username = $1 LIMIT 1" in text(item, "sql"):
            expected.add(key)
        if source == text(host_mapping, "path"):
            expected.add(key)
    if len(expected) != 6:
        raise ValueError("The expected original operation set changed")
    after = require_rows(
        read_json(directory, "after-receipts.json"), "final source receipts"
    )
    if len(after) != 12 or len({integer(row, "id") for row in after}) != 12:
        raise ValueError("The final source receipts are empty, partial, or duplicated")
    previous = {integer(row, "stmtid"): 0 for row in before}
    total_native = dict.fromkeys(control_lines, 0)
    for application in ("application_a", "application_b"):
        rows = require_rows(
            read_json(directory, application + "-receipts.json"), application
        )
        selected = [row for row in rows if row.get("test_name") == application]
        if (
            len(selected) != 6
            or {
                (text(row, "source_path"), integer(row, "start_byte"))
                for row in selected
            }
            != expected
        ):
            raise ValueError("An application source receipt is missing or unexpected")
        counts = {
            source: dict.fromkeys(lines, 0) for source, lines in locations.items()
        }
        for row in selected:
            key = text(row, "source_path"), integer(row, "start_byte")
            item = index[key]
            if (
                row not in after
                or row["source_sha"] != item["source_sha256"]
                or row["end_byte"] != item["end"]
                or row["start_line"] != item["line"]
            ):
                raise ValueError(
                    "A database receipt has invalid original source provenance"
                )
            runtime = text(item, "sql")
            if item["path"] == mappings["generated_origin"]:
                runtime = text(mappings, "generated_runtime_sql")
            if row["runtime_sql_sha"] != hashlib.sha256(runtime.encode()).hexdigest():
                raise ValueError(
                    "A database receipt has an unexpected runtime SQL hash"
                )
            counts[text(item, "path")][integer(item, "line")] += 1
        native = require_rows(
            read_json(directory, application + "-profiler-statements.json"),
            application + " native profiler",
        )
        if (
            len(native) != 4
            or {integer(row, "lineno") for row in native} != control_lines
        ):
            raise ValueError("The native procedural profile is partial")
        deltas: dict[int, int] = {}
        for row in native:
            value = row.get("exec_stmts")
            if not isinstance(value, int) or value < previous[integer(row, "stmtid")]:
                raise ValueError("A native procedural counter is absent or decreased")
            delta = value - previous[integer(row, "stmtid")]
            previous[integer(row, "stmtid")] = value
            deltas[integer(row, "lineno")] = delta
            counts[control_path][integer(row, "lineno") + 3] = delta
            total_native[integer(row, "lineno")] += delta
        if deltas != {2: 1, 3: 1, 4: 1, 6: 0}:
            raise ValueError(
                "The native named-function delta does not match the bounded calls"
            )
        write_lcov(output / (application + ".dat"), counts, (deltas[4], deltas[6]))
    native_coverage = require_rows(
        read_json(directory, "after-profiler-coverage.json"), "native coverage"
    )
    if native_coverage != [{"branches": 0.5, "statements": 0.75}]:
        raise ValueError("The native procedural coverage metrics changed")
    functions = require_rows(
        read_json(directory, "after-profiler-functions.json"),
        "stored profiler functions",
    )
    if (
        len(functions) != 1
        or functions[0].get("funcoid") != "research_branch(boolean)"
        or functions[0].get("exec_count") != 2
    ):
        raise ValueError("The stored procedural profile set changed")
    summary = {
        "functional_passed": True,
        "sql_database_receipts": 12,
        "native_named_statement_counts": total_native,
        "anonymous_block_profile_available": False,
        "anonymous_body_locations_unmeasured": sorted(
            anonymous_locations[text(anonymous, "path")]
        ),
        "source_identities": len(locations),
        "candidate_locations": sum(map(len, locations.values())),
        "branch_limit": "One IF with one RETURN in each arm. Native parent_stmtid 2 and source AST identify this control only.",
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    export(args.evidence, args.output)


if __name__ == "__main__":
    main()
