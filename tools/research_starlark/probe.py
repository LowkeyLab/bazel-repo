"""Collect native Bazel debugger receipts for a fixed statement inventory."""

import argparse
import base64
import json
import socket
import subprocess
import time
from pathlib import Path
from typing import cast


def varint(value: int) -> bytes:
    result = bytearray()
    while value > 127:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def integer(field: int, value: int) -> bytes:
    return varint(field << 3) + varint(value)


def message(field: int, value: bytes) -> bytes:
    return varint((field << 3) | 2) + varint(len(value)) + value


def decode(data: bytes) -> dict[int, list[int | bytes]]:
    offset = 0
    result: dict[int, list[int | bytes]] = {}

    def read_int() -> int:
        nonlocal offset
        value = 0
        shift = 0
        while True:
            byte = data[offset]
            offset += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value
            shift += 7
            if shift > 63:
                raise ValueError("Invalid protobuf varint")

    while offset < len(data):
        tag = read_int()
        field, wire = tag >> 3, tag & 7
        if wire == 0:
            value = read_int()
        elif wire == 2:
            size = read_int()
            value = data[offset : offset + size]
            offset += size
        else:
            raise ValueError(f"Unexpected protobuf wire type {wire}")
        result.setdefault(field, []).append(value)
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--port", type=int, default=17354)
    parser.add_argument("--reset", action="store_true")
    args = parser.parse_args()
    root = Path.cwd()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=True)
    inventory = json.loads(Path(args.inventory).read_text())
    command = [
        "nix",
        "develop",
        "--command",
        "bazelisk",
        "--host_jvm_args=-Xmx1536m",
        "build",
        "//tools/research_starlark:phase_observation",
        "--config=remote",
        "--jobs=8",
        "--local_resources=cpu=2",
        "--local_resources=memory=2048",
        "--experimental_skylark_debug",
        f"--experimental_skylark_debug_server_port={args.port}",
        f"--execution_log_json_file={output / 'execution.json'}",
        f"--build_event_json_file={output / 'bep.json'}",
    ]
    if args.reset:
        command.append("--experimental_skylark_debug_reset_analysis")
    (output / "command.json").write_text(json.dumps(command, indent=2))
    started = time.monotonic()
    with (
        (output / "stdout.txt").open("wb") as stdout,
        (output / "stderr.txt").open("wb") as stderr,
    ):
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        connection = None
        try:
            while time.monotonic() - started < 120:
                if process.poll() is not None:
                    raise RuntimeError(
                        f"Bazel exited before debugger connection: {process.returncode}"
                    )
                try:
                    connection = socket.create_connection(
                        ("127.0.0.1", args.port), timeout=1
                    )
                    break
                except OSError:
                    time.sleep(0.1)
            if connection is None:
                raise TimeoutError("Debugger did not accept a connection")
            connection.settimeout(90)
            sequence = 0
            frames: list[dict[str, object]] = []
            hits: list[dict[str, object]] = []

            def send(field: int, payload: bytes) -> None:
                nonlocal sequence
                sequence += 1
                data = integer(1, sequence) + message(field, payload)
                frames.append(
                    {"direction": "request", "base64": base64.b64encode(data).decode()}
                )
                connection.sendall(varint(len(data)) + data)

            breakpoints = b""
            for path, lines in inventory.items():
                for line in lines:
                    location = message(1, str(root / path).encode()) + integer(2, line)
                    breakpoints += message(1, message(1, location))
            send(101, breakpoints)
            send(105, b"")
            stream = connection.makefile("rb")
            while True:
                size, shift = 0, 0
                while True:
                    byte = stream.read(1)
                    if not byte:
                        break
                    size |= (byte[0] & 127) << shift
                    shift += 7
                    if byte[0] < 128:
                        break
                if not byte:
                    break
                data = stream.read(size)
                if len(data) != size:
                    raise ValueError("Truncated debugger event")
                frames.append(
                    {"direction": "event", "base64": base64.b64encode(data).decode()}
                )
                event = decode(data)
                if 99 in event:
                    raise ValueError(
                        f"Debugger error: {decode(cast(bytes, event[99][0]))}"
                    )
                if 1001 in event:
                    paused = decode(cast(bytes, event[1001][0]))
                    thread = decode(cast(bytes, paused[1][0]))
                    location = decode(cast(bytes, thread[4][0]))
                    hit: dict[str, object] = {
                        "path": cast(bytes, location[1][0]).decode(),
                        "line": location[2][0],
                        "column": location.get(3, [0])[0],
                        "thread": thread[1][0],
                        "reason": thread.get(3, [0])[0],
                    }
                    hits.append(hit)
                    send(102, integer(1, cast(int, thread[1][0])))
                if time.monotonic() - started > 240:
                    raise TimeoutError("Debugger observation exceeded its bound")
            code = process.wait(timeout=30)
            (output / "exit.txt").write_text(str(code) + "\n")
            (output / "hits.json").write_text(json.dumps(hits, indent=2))
            if code:
                raise RuntimeError(f"Bazel build failed: {code}")
        finally:
            if connection:
                connection.close()
            if "frames" in locals():
                (output / "wire.json").write_text(json.dumps(frames, indent=2))
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            (output / "process-exit.txt").write_text(str(process.returncode) + "\n")
    print(json.dumps({"events": len(hits), "output": str(output)}))


if __name__ == "__main__":
    main()
