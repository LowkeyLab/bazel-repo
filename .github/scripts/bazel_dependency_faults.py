"""Isolated hosted restore faults; no real cache entries or credentials are used."""

import hashlib
import http.server
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

PAYLOAD = b"public dependency fixture\n"


def evidence_path():
    return Path(os.environ["RUNNER_TEMP"]) / "bazel-dependency-faults"


def serve(mode):
    evidence = evidence_path()
    lock = threading.Lock()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def record(self):
            with lock, (evidence / "requests.jsonl").open("a") as output:
                output.write(
                    json.dumps(
                        {"method": self.command, "path": self.path, "time": time.time()}
                    )
                    + "\n"
                )

        def do_GET(self):
            self.record()
            body = b"corrupt material" if self.path == "/corrupt" else PAYLOAD
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            self.record()
            if mode == "stall":
                time.sleep(600)
                return
            self.send_response(503 if mode == "error" else 200)
            body = b'{"ok":false}'
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    (evidence / "url").write_text(f"http://127.0.0.1:{server.server_port}/")
    server.serve_forever()


def start(mode):
    evidence = evidence_path()
    evidence.mkdir()
    with (evidence / "server.log").open("w") as output:
        process = subprocess.Popen(
            [sys.executable, __file__, "serve", mode], stdout=output, stderr=output
        )
    (evidence / "server.pid").write_text(str(process.pid))
    deadline = time.monotonic() + 10
    while not (evidence / "url").exists():
        if process.poll() is not None or time.monotonic() >= deadline:
            raise RuntimeError("Fault endpoint failed to start")
        time.sleep(0.1)
    url = (evidence / "url").read_text()
    workspace = evidence / "workspace"
    workspace.mkdir()
    (workspace / "MODULE.bazel").write_text(
        'deps = use_extension("//:deps.bzl", "deps")\nuse_repo(deps, "fixture")\n'
    )
    (workspace / "BUILD.bazel").write_text(
        'filegroup(name = "fixture", srcs = ["@fixture//:material"])\n'
    )
    (workspace / "deps.bzl").write_text(
        "def _repo(ctx):\n"
        f'    ctx.download(url = "{url}dependency", output = "material", sha256 = "{hashlib.sha256(PAYLOAD).hexdigest()}")\n'
        '    ctx.file("BUILD.bazel", "exports_files([\\"material\\"])\\n")\n'
        "repo = repository_rule(implementation = _repo)\n"
        'def _deps(ctx):\n    repo(name = "fixture")\n'
        "deps = module_extension(implementation = _deps)\n"
    )
    # Use the same configured cache and real required fetch command as the pilot.
    (workspace / ".bazelrc").write_text("common:ci --announce_rc\n")
    (workspace / ".bazelversion").write_text((Path.cwd() / ".bazelversion").read_text())
    cache = Path.home() / ".cache" / "bazel-repo"
    cache.mkdir(parents=True, exist_ok=True)
    (cache / "incomplete-restore").write_text("must be discarded before fetch")
    with Path(os.environ["GITHUB_ENV"]).open("a") as output:
        output.write(
            f"FAULT_URL={url}\nFAULT_WORKSPACE={workspace}\nFAULT_EVIDENCE={evidence}\n"
        )
    (evidence / "started").write_text(str(time.time()))


def verify(mode):
    evidence = evidence_path()
    elapsed = float((evidence / "restored").read_text()) - float(
        (evidence / "started").read_text()
    )
    requests = [
        json.loads(line)
        for line in (evidence / "requests.jsonl").read_text().splitlines()
    ]
    if not any(request["method"] == "POST" for request in requests):
        raise RuntimeError("Restore never contacted the injected endpoint")
    if mode == "stall" and (
        elapsed < 120 or os.environ["RESTORE_OUTCOME"] != "failure"
    ):
        raise RuntimeError(f"Restore did not reach its two-minute deadline: {elapsed}s")
    cache = Path.home() / ".cache" / "bazel-repo"
    if (cache / "incomplete-restore").exists():
        raise RuntimeError("Incomplete restore survived preparation")
    if not any(request["path"] == "/dependency" for request in requests):
        raise RuntimeError("Fallback did not fetch the public dependency")
    workspace = evidence / "workspace"
    source = workspace / "deps.bzl"
    source.write_text(
        source.read_text()
        .replace("/dependency", "/corrupt")
        .replace(hashlib.sha256(PAYLOAD).hexdigest(), "0" * 64)
    )
    with (evidence / "integrity.log").open("w") as output:
        result = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("bazel_dependencies.py")),
                "prepare",
            ],
            cwd=workspace,
            stdout=output,
            stderr=subprocess.STDOUT,
            check=False,
        )
    log = (evidence / "integrity.log").read_text()
    if result.returncode == 0 or "Checksum" not in log:
        raise RuntimeError(
            "Malformed upstream material was not rejected by native checksum validation"
        )
    (evidence / "result.json").write_text(
        json.dumps(
            {
                "mode": mode,
                "restore_elapsed_seconds": elapsed,
                "restore_outcome": os.environ["RESTORE_OUTCOME"],
                "native_integrity_exit_status": result.returncode,
            },
            indent=2,
        )
    )


def restored(_mode):
    (evidence_path() / "restored").write_text(str(time.time()))


if __name__ == "__main__":
    command, mode = sys.argv[1:]
    {"start": start, "serve": serve, "verify": verify, "restored": restored}[command](
        mode
    )
