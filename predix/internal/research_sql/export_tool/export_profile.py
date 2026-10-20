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
    path: Path,
    locations: dict[str, dict[int, int]],
    branches: tuple[int, int],
    body_branches: list[dict[str, object]] | None = None,
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
        extra = [row for row in body_branches or [] if row["path"] == source]
        for branch in extra:
            for arm, key in enumerate(("then_line", "else_line")):
                output.append(
                    f"BRDA:{branch['line']},{branch['line']},{arm},{hits[integer(branch, key)]}"
                )
        if extra:
            values = [
                hits[integer(row, key)]
                for row in extra
                for key in ("then_line", "else_line")
            ]
            output.extend(
                [f"BRF:{len(values)}", f"BRH:{sum(value > 0 for value in values)}"]
            )
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
    plans = object_row(read_json(directory, "body-plan.json"), "body plan")
    body_index: dict[tuple[str, int], dict[str, object]] = {}
    body_branches: list[dict[str, object]] = []
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
    for source, raw_plan in plans.items():
        plan = object_row(raw_plan, "body transform")
        original = next(
            item
            for item in statements
            if item["path"] == source and item["kind"] == "DoStmt"
        )
        if (
            plan["original_sql_sha256"]
            != hashlib.sha256(text(original, "sql").encode()).hexdigest()
        ):
            raise ValueError("The body plan has a different original SQL hash")
        if (
            plan["instrumented_sql_sha256"]
            != hashlib.sha256(text(plan, "instrumented_sql").encode()).hexdigest()
        ):
            raise ValueError("The instrumented SQL hash is invalid")
        for location in require_rows(plan["locations"], "body locations"):
            line = integer(location, "line")
            if (
                location["source_sha256"] != original["source_sha256"]
                or location["path"] != source
            ):
                raise ValueError(
                    "A body location has invalid original source provenance"
                )
            body_index[source, line] = location
            locations[source][line] = 0
        body_branches.extend(require_rows(plan["branches"] or [], "body branches"))
    if len(plans) != 5 or len(body_index) != 17:
        raise ValueError("The bounded anonymous body inventory changed")
    output.mkdir(parents=True, exist_ok=True)
    write_lcov(output / "baseline.dat", locations, (0, 0), body_branches)
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
        if source == "prediction_bot/migrations/002_application_login.sql":
            expected.add(key)
        if source.endswith(("test_body.sql", "helper_body.sql")):
            expected.add(key)
        if source == text(host_mapping, "path"):
            expected.add(key)
    if len(expected) != 10:
        raise ValueError("The expected original operation set changed")
    after = require_rows(
        read_json(directory, "after-receipts.json"), "final source receipts"
    )
    if len(after) != 20 or len({integer(row, "id") for row in after}) != 20:
        raise ValueError("The final source receipts are empty, partial, or duplicated")
    previous_body = dict.fromkeys(body_index, 0)

    def body_rows(name: str) -> dict[tuple[str, int], int]:
        rows = require_rows(
            read_json(directory, name + "-body-profile.json"), name + " body profile"
        )
        if len(rows) != len(body_index):
            raise ValueError("The anonymous body profile is partial")
        result: dict[tuple[str, int], int] = {}
        for row in rows:
            key = text(row, "source_path"), integer(row, "original_line")
            if key not in body_index or key in result:
                raise ValueError(
                    "The anonymous body profile has an unknown or duplicate location"
                )
            original = body_index[key]
            if (
                row["source_sha"] != original["source_sha256"]
                or row["kind"] != original["kind"]
            ):
                raise ValueError(
                    "The anonymous body profile has invalid source provenance"
                )
            result[key] = integer(row, "hits")
        return result

    if body_rows("before") != previous_body:
        raise ValueError("The anonymous baseline contains execution")
    previous = {integer(row, "stmtid"): 0 for row in before}
    total_native = dict.fromkeys(control_lines, 0)
    for application in ("application_a", "application_b"):
        rows = require_rows(
            read_json(directory, application + "-receipts.json"), application
        )
        selected = [row for row in rows if row.get("test_name") == application]
        if (
            len(selected) != 10
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
            if item["kind"] == "DoStmt":
                runtime = text(
                    object_row(plans[text(item, "path")], "body plan"),
                    "instrumented_sql",
                )
            if row["runtime_sql_sha"] != hashlib.sha256(runtime.encode()).hexdigest():
                raise ValueError(
                    "A database receipt has an unexpected runtime SQL hash"
                )
            counts[text(item, "path")][integer(item, "line")] += 1
        effects = object_row(
            read_json(directory, application + "-maintained-do-effects.json"),
            "maintained DO effects",
        )
        if effects != {
            "login_role_created": True,
            "runtime_membership_granted": True,
            "source_path": "prediction_bot/migrations/002_application_login.sql",
        }:
            raise ValueError(
                "The maintained migration did not produce its expected effects"
            )
        rust = object_row(
            read_json(directory, application + "-rust-call.json"), "Rust call"
        )
        rust_row = next(
            row for row in selected if row["source_path"] == host_mapping["path"]
        )
        if (
            rust.get("operation") != "discord_server_repo::Repo::count"
            or rust.get("maintained_function_executed") is not True
            or rust.get("count") != 1
            or rust.get("backend_pid") != rust_row["backend_pid"]
            or rust.get("source_sha256") != rust_row["source_sha"]
            or rust.get("runtime_sql_sha256") != rust_row["runtime_sql_sha"]
        ):
            raise ValueError("The maintained Rust function receipt is incomplete")
        witness = f"{rust['backend_pid']} coverage_research_rust LOG:  execute"
        log = (directory / "database.log").read_text()
        if not any(
            witness in line and "SELECT COUNT(*) FROM servers" in line
            for line in log.splitlines()
        ):
            raise ValueError(
                "The maintained Rust query has no database session witness"
            )
        current_body = body_rows(application)
        for key, total in current_body.items():
            source, line = key
            delta = total - previous_body[key]
            untaken = (
                (source.endswith("statements.sql") and line == 18)
                or (source.endswith("test_body.sql") and line == 6)
                or (source.endswith("helper_body.sql") and line == 5)
                or source.endswith("disconnected_body.sql")
            )
            if delta != (0 if untaken else 1):
                raise ValueError(
                    "An anonymous body delta disagrees with the bounded execution"
                )
            counts[source][line] = delta
        previous_body = current_body
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
        write_lcov(
            output / (application + ".dat"),
            counts,
            (deltas[4], deltas[6]),
            body_branches,
        )
    if body_rows("after") != previous_body:
        raise ValueError(
            "The final anonymous profile disagrees with application totals"
        )
    limits = object_row(read_json(directory, "counter-limits.json"), "counter limits")
    if limits != {
        "original_found": False,
        "instrumented_found": True,
        "rollback_before": 0,
        "rollback_after": 0,
        "transactional_counter_survived": False,
    }:
        raise ValueError("The counter semantics limits did not reproduce")
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
        "sql_database_receipts": 20,
        "native_named_statement_counts": total_native,
        "anonymous_block_profile_available": True,
        "anonymous_body_locations": len(body_index),
        "maintained_rust_function_executed": True,
        "source_identities": len(locations),
        "candidate_locations": sum(map(len, locations.values())),
        "branch_limit": "The named native control has two RETURN arms. Each anonymous IF has two nonempty arms in this bounded corpus.",
        "counter_limits": limits,
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
