"""Run the bounded Nix coverage feasibility experiment."""

import collections
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import TypedDict

NIX_ROOT = Path("/nix/store/9jw399l4lllgai8p7fd4y61lqhi8qzja-nix-2.34.9")
NIX_HASH = "cac9e1cdced503201d9d175b48732cd29b027b066398a358ac98159c0c9128b1"
TOKEN = re.compile(
    r"\s+|\#[^\n]*|''[\s\S]*?''|\"(?:[^\"\\]|\\.)*\"|[0-9]+|[A-Za-z_][A-Za-z_0-9]*|[{}();=+]"
)


class Node(TypedDict):
    id: int
    kind: str
    start: int
    end: int
    line: int
    column: int
    children: list[int]


type Inventory = dict[str, list[Node]]
type Counts = collections.Counter[tuple[str, int]]


class Parser:
    """Parse only the declared experiment grammar. Reject all other syntax."""

    def __init__(self, source: str, identity: str) -> None:
        self.source = source
        self.identity = identity
        self.tokens: list[tuple[str, int, int]] = []
        self.nodes: list[Node] = []
        offset = 0
        while offset < len(source):
            match = TOKEN.match(source, offset)
            if match is None:
                raise ValueError(f"unsupported syntax at byte {offset}")
            token = match.group()
            if not token.isspace() and not token.startswith("#"):
                if "${" in token or "''$" in token or "''\\" in token:
                    raise ValueError(
                        "interpolation and indented-string escapes are unsupported"
                    )
                self.tokens.append((token, offset, match.end()))
            offset = match.end()
        self.index = 0

    def peek(self) -> str:
        return self.tokens[self.index][0] if self.index < len(self.tokens) else ""

    def take(self, expected: str | None = None) -> tuple[str, int, int]:
        token = self.tokens[self.index]
        if expected is not None and token[0] != expected:
            raise ValueError(f"expected {expected}, got {token[0]}")
        self.index += 1
        return token

    def node(
        self, kind: str, start: int, end: int, children: Sequence[Node] = ()
    ) -> Node:
        item: Node = {
            "id": len(self.nodes),
            "kind": kind,
            "start": start,
            "end": end,
            "line": self.source.count("\n", 0, start) + 1,
            "column": start - self.source.rfind("\n", 0, start),
            "children": [child["id"] for child in children],
        }
        self.nodes.append(item)
        return item

    def expression(self) -> Node:
        start = self.tokens[self.index][1]
        if self.peek() == "let":
            self.take("let")
            children = []
            while self.peek() != "in":
                self.take()
                self.take("=")
                children.append(self.expression())
                self.take(";")
            self.take("in")
            children.append(self.expression())
            return self.node("let", start, children[-1]["end"], children)
        if self.peek() == "if":
            self.take("if")
            condition = self.expression()
            self.take("then")
            yes = self.expression()
            self.take("else")
            no = self.expression()
            return self.node("if", start, no["end"], (condition, yes, no))
        left = self.atom()
        if (
            left["kind"] == "identifier"
            and self.source[left["start"] : left["end"]] == "throw"
        ):
            right = self.atom()
            left = self.node("application", start, right["end"], (left, right))
        while self.peek() == "+":
            self.take("+")
            right = self.atom()
            left = self.node("addition", start, right["end"], (left, right))
        return left

    def atom(self) -> Node:
        token, start, end = self.take()
        if token == "(":
            child = self.expression()
            end = self.take(")")[2]
            return self.node("parentheses", start, end, (child,))
        if token == "{":
            children = []
            while self.peek() != "}":
                self.take()
                self.take("=")
                children.append(self.expression())
                self.take(";")
            end = self.take("}")[2]
            return self.node("attributes", start, end, children)
        if token in ("true", "false"):
            kind = "boolean"
        elif token.isdigit():
            kind = "integer"
        elif token.startswith(('"', "''")):
            kind = "string"
        elif re.fullmatch("[A-Za-z_][A-Za-z_0-9]*", token) and token not in (
            "then",
            "else",
            "in",
        ):
            kind = "identifier"
        else:
            raise ValueError(f"unsupported atom {token}")
        return self.node(kind, start, end)

    def parse(self) -> list[Node]:
        self.expression()
        if self.index != len(self.tokens):
            raise ValueError("unconsumed syntax")
        return self.nodes

    def instrument(self) -> str:
        starts: collections.defaultdict[int, list[Node]] = collections.defaultdict(list)
        ends: collections.defaultdict[int, list[Node]] = collections.defaultdict(list)
        for node in self.nodes:
            starts[node["start"]].append(node)
            ends[node["end"]].append(node)
        output = []
        for offset in range(len(self.source) + 1):
            for _ in sorted(ends[offset], key=lambda node: node["start"], reverse=True):
                output.append("))")
            for node in sorted(
                starts[offset], key=lambda node: (node["end"], node["id"]), reverse=True
            ):
                marker = f"NIXCOV:{self.identity}:{node['id']}"
                output.append("(builtins.trace " + json.dumps(marker) + " (")
            if offset < len(self.source):
                output.append(self.source[offset])
        return "".join(output)


def execute(
    command: list[str],
    evidence: Path,
    name: str,
    environment: dict[str, str] | None = None,
    expected: int = 0,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        env=environment,
        check=False,
        timeout=20,
    )
    (evidence / f"{name}.stdout").write_text(result.stdout)
    (evidence / f"{name}.stderr").write_text(result.stderr)
    (evidence / f"{name}.json").write_text(
        json.dumps({"argv": command, "exit": result.returncode}, indent=2) + "\n"
    )
    capture_manifest(evidence / f"{name}.stderr", result.returncode)
    if result.returncode != expected:
        raise RuntimeError(f"{name}: expected exit {expected}, got {result.returncode}")
    return result


def capture_manifest(trace_path: Path, status: int = 0) -> None:
    data = trace_path.read_bytes()
    trace_path.with_suffix(trace_path.suffix + ".capture.json").write_text(
        json.dumps(
            {
                "sha256": hashlib.sha256(data).hexdigest(),
                "bytes": len(data),
                "evaluator_exit": status,
            },
            indent=2,
        )
        + "\n"
    )


def collect(trace_path: Path, inventory: Inventory, output: Path) -> Counts:
    if not trace_path.is_file():
        raise ValueError("missing evaluator output")
    capture = trace_path.with_suffix(trace_path.suffix + ".capture.json")
    if not capture.is_file():
        raise ValueError("missing evaluator capture manifest")
    metadata = json.loads(capture.read_text())
    data = trace_path.read_bytes()
    if (
        metadata.get("evaluator_exit") != 0
        or metadata.get("bytes") != len(data)
        or metadata.get("sha256") != hashlib.sha256(data).hexdigest()
    ):
        raise ValueError("evaluator capture integrity failure")
    text = data.decode()
    if "NIXCOV:receipt:0" not in text:
        raise ValueError("missing evaluator receipt")
    counts: Counts = collections.Counter()
    for file, node in re.findall(r"trace: NIXCOV:([^:\s]+):([0-9]+)", text):
        if file == "receipt":
            continue
        if file not in inventory or int(node) >= len(inventory[file]):
            raise ValueError("unknown evaluator location")
        counts[file, int(node)] += 1
    root = len(inventory["evaluated.nix"]) - 1
    if not counts["evaluated.nix", root]:
        raise ValueError("missing demanded root expression")
    render(inventory, counts, output)
    return counts


def render(inventory: Inventory, counts: Counts, output: Path) -> None:
    records = []
    for file, nodes in inventory.items():
        lines = collections.defaultdict(int)
        for node in nodes:
            lines[node["line"]] = max(lines[node["line"]], counts[file, node["id"]])
        records.extend(
            [
                f"SF:tools/research_nix/fixtures/{file}",
                *[f"DA:{line},{hit}" for line, hit in sorted(lines.items())],
            ]
        )
        for node in nodes:
            if node["kind"] == "if":
                for arm, child in enumerate(node["children"][1:]):
                    records.append(
                        f"BRDA:{node['line']},{node['id']},{arm},{counts[file, child]}"
                    )
        records.append("end_of_record")
    output.write_text("\n".join(records) + "\n")


def main() -> None:
    output = Path(
        sys.argv[1] if len(sys.argv) > 1 else os.environ["TEST_UNDECLARED_OUTPUTS_DIR"]
    ).resolve()
    output.mkdir(parents=True, exist_ok=True)
    fixtures = Path(__file__).parent / "fixtures"
    nix = str(NIX_ROOT / "bin/nix-instantiate")
    assert hashlib.sha256((NIX_ROOT / "bin/nix").read_bytes()).hexdigest() == NIX_HASH
    version = execute([nix, "--version"], output, "version").stdout.strip()
    assert "2.34.9" in version
    inventory: Inventory = {}
    parsers: dict[str, Parser] = {}
    for fixture in sorted(fixtures.glob("*.nix")):
        parser = Parser(fixture.read_text(), fixture.name)
        inventory[fixture.name] = parser.parse()
        parsers[fixture.name] = parser
        execute([nix, "--parse", str(fixture)], output, fixture.stem + "-parse")
        (output / fixture.name).write_text(parser.instrument())
    (output / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
    original = f"(import {fixtures / 'evaluated.nix'}).result"
    wrapped = f'builtins.trace "NIXCOV:receipt:0" ((import {output / "evaluated.nix"}).result)'
    normal = execute(
        [nix, "--eval", "--strict", "--expr", original], output, "original"
    )
    native = execute(
        [
            nix,
            "--eval",
            "--strict",
            "--expr",
            original,
            "--trace-function-calls",
            "-vvvvv",
        ],
        output,
        "native-trace",
    )
    assert normal.stdout.strip() == native.stdout.strip() == "6"
    environment = os.environ.copy()
    environment.update(
        NIX_SHOW_STATS="1",
        NIX_COUNT_CALLS="1",
        NIX_SHOW_STATS_PATH=str(output / "native-stats.json"),
    )
    execute(
        [nix, "--eval", "--strict", "--expr", original],
        output,
        "native-stats-run",
        environment,
    )
    first = execute(
        [nix, "--eval", "--strict", "--expr", wrapped], output, "instrumented"
    )
    second = execute([nix, "--eval", "--strict", "--expr", wrapped], output, "repeat")
    assert first.stdout == second.stdout == normal.stdout
    counts = collect(
        output / "instrumented.stderr", inventory, output / "measured.lcov"
    )
    render(inventory, collections.Counter(), output / "baseline.lcov")
    repeat = collect(output / "repeat.stderr", inventory, output / "repeat.lcov")
    assert counts == repeat
    assert all(
        not counts[file, node["id"]]
        for file in inventory
        if file == "disconnected.nix"
        for node in inventory[file]
    )
    lazy_line = 8
    assert all(
        not counts["evaluated.nix", node["id"]]
        for node in inventory["evaluated.nix"]
        if node["line"] == lazy_line
    )
    # The shared thunk emits its body once although two identifier expressions demand it.
    additions = [
        node
        for node in inventory["evaluated.nix"]
        if node["kind"] == "addition" and node["line"] == 4
    ]
    assert len(additions) == 1 and counts["evaluated.nix", additions[0]["id"]] == 1
    failures: list[dict[str, str]] = []
    for name, contents in [
        ("missing", None),
        ("empty", ""),
        ("corrupt", "trace: NIXCOV:receipt:0\ntrace: NIXCOV:unknown:999\n"),
        ("receipt-only", "trace: NIXCOV:receipt:0\n"),
        ("truncated", first.stderr[:-20]),
    ]:
        path = output / f"{name}.trace"
        if contents is not None:
            path.write_text(contents)
            capture_manifest(path)
            if name == "truncated":
                shutil.copyfile(
                    output / "instrumented.stderr.capture.json",
                    path.with_suffix(path.suffix + ".capture.json"),
                )
        try:
            collect(path, inventory, output / f"{name}.lcov")
        except ValueError as error:
            failures.append(
                {
                    "case": name,
                    "functional_output": first.stdout.strip(),
                    "collection_error": str(error),
                }
            )
        else:
            raise AssertionError(f"{name} was accepted")
    (output / "negative-cases.json").write_text(json.dumps(failures, indent=2) + "\n")
    # A positive-output corruption must also fail, even when the receipt survives.
    try:
        Parser("builtins.map (x: x) [ 1 ]", "unsupported.nix").parse()
    except ValueError:
        pass
    else:
        raise AssertionError("unsupported syntax was accepted")
    builder = next(
        node
        for node in inventory["evaluated.nix"]
        if node["kind"] == "string"
        and parsers["evaluated.nix"]
        .source[node["start"] : node["end"]]
        .startswith("''")
    )
    builder_expr = f"(import {output / 'evaluated.nix'}).script"
    builder_expr = 'builtins.trace "NIXCOV:receipt:0" (' + builder_expr + ")"
    builder_result = execute(
        [nix, "--eval", "--strict", "--json", "--expr", builder_expr],
        output,
        "builder-string",
    )
    builder_text = json.loads(builder_result.stdout)
    builder_counts = collect(
        output / "builder-string.stderr", inventory, output / "builder-evaluation.lcov"
    )
    merged = counts + builder_counts
    render(inventory, merged, output / "combined.lcov")
    shell = output / "builder.sh"
    shell.write_text(builder_text)
    source_start = builder["line"] + 1
    mapping = {
        index + 1: source_start + index
        for index, _ in enumerate(builder_text.splitlines())
    }
    original_lines = parsers["evaluated.nix"].source.splitlines()
    for extracted, original in mapping.items():
        assert (
            original_lines[original - 1][4:] == builder_text.splitlines()[extracted - 1]
        )
    (output / "builder-map.json").write_text(
        json.dumps(
            {
                "source": "tools/research_nix/fixtures/evaluated.nix",
                "lines": mapping,
                "interpolation": "none",
                "removed_indentation": 4,
            },
            indent=2,
        )
        + "\n"
    )
    bash = "/run/current-system/sw/bin/bash"
    execute([bash, "--version"], output, "bash-version")
    (output / "bash-pin.json").write_text(
        json.dumps(
            {
                "path": str(Path(bash).resolve()),
                "sha256": hashlib.sha256(Path(bash).read_bytes()).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    env = os.environ.copy()
    env.update(out=str(output / "builder-product"), PS4="+NIXSHELL:${LINENO}: ")
    execute(
        [
            bash,
            "--noprofile",
            "--norc",
            "-c",
            "PS4='+NIXSHELL:${LINENO}: '; set -x; source \"$1\"",
            "nix-builder",
            str(shell),
        ],
        output,
        "builder-shell",
        env,
    )
    assert (output / "builder-product").read_text() == "builder-ran\n"
    assert all(merged[key] >= hit for key, hit in counts.items())
    (output / "summary.json").write_text(
        json.dumps(
            {
                "evaluator": str(NIX_ROOT),
                "sha256": NIX_HASH,
                "version": version,
                "native_observation": "outer import call only; no expression-line events",
                "instrumented_scope": "bounded expression-entry model",
                "lazy_poison": "not forced",
                "shared_thunk_body_hits": 1,
                "repeat_same_counts": True,
                "negative_cases": len(failures),
                "capture_integrity": "sha256 plus exit status and demanded-root receipt",
                "remote_execution": False,
                "hosted_upload": False,
            },
            indent=2,
        )
        + "\n"
    )
    print(json.dumps({"output": str(output), "status": "passed"}))


if __name__ == "__main__":
    main()
