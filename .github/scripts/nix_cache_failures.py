"""Token-free hosted Nix outage experiments against isolated fault endpoints."""

import http.server
import json
import os
import shlex
import socket
import sys
import threading
import time
from pathlib import Path

import nix_tool


class Cache(http.server.BaseHTTPRequestHandler):
    status = 404

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        if self.path == "/nix-cache-info":
            self.send_response(200)
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(
                    b"StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 1\n"
                )
            return
        if self.server.mode == "stall":
            time.sleep(70)
            return
        self.send_response(503 if self.server.mode == "unavailable" else 404)
        self.end_headers()

    def log_message(self, *_args):
        pass


def require_publication_timeout(status_file, elapsed_seconds):
    status = int(status_file.read_text())
    if status not in (124, 137) or elapsed_seconds < 10:
        raise ValueError(
            "Publication client must reach its 10-second timeout; "
            f"observed exit {status} after {elapsed_seconds:.3f}s"
        )


def main():
    if os.environ.get("CACHIX_AUTH_TOKEN") or os.environ.get("CACHIX_SIGNING_KEY"):
        raise ValueError("Fault experiments must not receive cache credentials")
    evidence = nix_tool.Evidence("failures")
    results = []

    def command(label, args, succeeds):
        try:
            stdout, stderr = evidence.run(label, *args)
            success = True
        except RuntimeError:
            stdout = (evidence.directory / f"{label}.stdout").read_text()
            stderr = (evidence.directory / f"{label}.stderr").read_text()
            success = False
        if success != succeeds:
            raise ValueError(f"Unexpected outcome for {label}")
        return stdout, stderr

    command("version", ["nix", "--version"], True)
    command("effective-config", ["nix", "config", "show", "--json"], True)
    fixture = evidence.directory / "fixture.nix"
    for mode, diagnostic in (
        ("missing", "cache miss"),
        ("unavailable", "remote unavailable (HTTP 503)"),
        ("refused", "network failure"),
        ("stall", "network failure"),
    ):
        script = "/bin/mkdir -p $out/bin; /bin/printf '#!/bin/sh\\n/bin/echo fixture\\n' > $out/bin/aspect; /bin/chmod +x $out/bin/aspect"
        fixture.write_text(
            'builtins.derivation { name = "cache-fault-'
            + mode
            + "-"
            + str(time.time_ns())
            + '"; system = builtins.currentSystem; '
            'builder = "/bin/sh"; args = [ "-c" ' + json.dumps(script) + " ]; }"
        )
        # The fixture is intentionally input-free; only its host-shell build disables sandboxing.
        drv, _ = command(f"{mode}-instantiate", ["nix-instantiate", str(fixture)], True)
        output, _ = command(
            f"{mode}-output", ["nix-store", "--query", "--outputs", drv.strip()], True
        )
        output = output.strip()
        if os.path.exists(output):
            raise ValueError("Fixture output must initially be absent")
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Cache)
        server.daemon_threads = True
        server.mode = mode
        url = f"http://127.0.0.1:{server.server_port}"
        if mode == "refused":
            server.server_close()
        else:
            threading.Thread(target=server.serve_forever, daemon=True).start()
        options = [
            "--option",
            "substituters",
            url,
            "--option",
            "connect-timeout",
            "5",
            "--option",
            "stalled-download-timeout",
            "30",
            "--option",
            "download-attempts",
            "2",
            "--option",
            "narinfo-cache-negative-ttl",
            "0",
            "--option",
            "require-sigs",
            "true",
            "--option",
            "builders",
            "",
        ]
        started = time.monotonic()
        observed = nix_tool.diagnose(url, output)
        if observed != diagnostic:
            raise ValueError(f"{mode}: expected {diagnostic}, received {observed}")
        _, strict = command(
            f"{mode}-strict",
            [
                "nix-store",
                "--realise",
                output,
                "--log-format",
                "internal-json",
                "-vv",
                "--option",
                "max-jobs",
                "0",
                "--option",
                "fallback",
                "false",
                *options,
            ],
            False,
        )
        if os.path.exists(output):
            raise ValueError("Strict failure must leave the output absent")
        # Strict failure cannot establish successful source fallback: exercise it separately.
        _, ordinary = command(
            f"{mode}-ordinary",
            [
                "nix-build",
                str(fixture),
                "--no-out-link",
                "--fallback",
                "--log-format",
                "internal-json",
                "-vv",
                "--option",
                "sandbox",
                "false",
                *options,
            ],
            True,
        )
        command(f"{mode}-smoke", [f"{output}/bin/aspect", "--version"], True)
        if not any(
            json.loads(line[5:]).get("action") == "start"
            and json.loads(line[5:]).get("type") == 105
            for line in ordinary.splitlines()
            if line.startswith("@nix ")
        ):
            raise ValueError("Source fallback requires an observed Nix build activity")
        if any(
            json.loads(line[5:]).get("action") == "start"
            and json.loads(line[5:]).get("type") == 105
            for line in strict.splitlines()
            if line.startswith("@nix ")
        ):
            raise ValueError("Strict run attempted compilation")
        results.append(
            {
                "mode": mode,
                "diagnostic": observed,
                "output": output,
                "elapsed_seconds": time.monotonic() - started,
                "strict": "failed without compilation",
                "ordinary": "source build and smoke passed",
            }
        )
        if mode != "refused":
            server.shutdown()
            server.server_close()
    # Exercise the exact production publication catch with the real CLI pinned to loopback.
    client = Path(os.environ["CACHIX_PATH"]) / "bin" / "cachix"
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        refused = f"http://127.0.0.1:{reservation.getsockname()[1]}"
    wrapper = evidence.directory / "publication-client" / "bin"
    wrapper.mkdir(parents=True)
    launcher = wrapper / "cachix"
    launcher.write_text(
        "#!/usr/bin/env bash\n"
        + "echo "
        + shlex.quote("Isolated Cachix API " + refused)
        + " >&2\n"
        + "timeout --kill-after=5s 10s "
        + shlex.quote(str(client))
        + " --host "
        + shlex.quote(refused)
        + " --config "
        + shlex.quote(str(evidence.directory / "empty-cachix.dhall"))
        + ' "$@"\nstatus=$?\nprintf \'%s\\n\' "$status" > '
        + shlex.quote(str(evidence.directory / "publication-client.exit-status"))
        + '\nexit "$status"\n'
    )
    launcher.chmod(0o700)
    os.environ.update(
        CACHIX_PATH=str(wrapper.parent),
        CACHIX_AUTH_TOKEN="isolated-nonsecret-test-marker",
        CACHIX_CACHE_NAME="fault-fixture",
        TOOL_PATH=output,
        NIX_TOOL="aspect",
    )
    try:
        publication, errors = command(
            "publication-failure", ["bash", ".github/scripts/publish-nix-tool.sh"], True
        )
    finally:
        os.environ.pop("CACHIX_AUTH_TOKEN", None)
        os.environ.pop("CACHIX_CACHE_NAME", None)
    require_publication_timeout(
        evidence.directory / "publication-client.exit-status",
        evidence.commands[-1]["elapsed_seconds"],
    )
    if "Cachix publication failed" not in publication or "127.0.0.1" not in errors:
        raise ValueError(
            "Publication must fail at the isolated endpoint and emit ordinary fallback warning"
        )
    # A source failure goes through the real reader, not through publication error handling.
    broken = evidence.directory / "broken-tool"
    broken.mkdir()
    (broken / "flake.nix").write_text(
        """{ outputs = { self }: { packages.x86_64-linux.aspect = builtins.derivation { name = "cache-fault-build-failure"; system = "x86_64-linux"; builder = "/bin/sh"; args = [ "-c" "exit 23" ]; }; }; }"""
    )
    helper = Path(nix_tool.__file__).resolve()
    old_directory = Path.cwd()
    os.environ["NIX_CONFIG"] = os.environ.get("NIX_CONFIG", "") + "\nsandbox = false\n"
    try:
        os.chdir(broken)
        _, failure = command(
            "genuine-build-failure",
            [sys.executable, str(helper), "build", "aspect"],
            False,
        )
    finally:
        os.chdir(old_directory)
    if "23" not in failure or "builder" not in failure:
        raise ValueError(
            "Expected genuine fixture builder exit 23 through production helper"
        )
    (evidence.directory / "result.json").write_text(
        json.dumps({"status": "passed", "cases": results}, indent=2) + "\n"
    )
    print(json.dumps(results, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
