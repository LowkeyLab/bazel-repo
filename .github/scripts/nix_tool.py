"""Public Nix tool reader and runtime-closure availability probe for CI."""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path


def run(*args, capture=False):
    return subprocess.run(
        args, check=True, text=True, stdout=subprocess.PIPE if capture else None
    )


def warning(message):
    print(f"::warning::{message}", file=sys.stderr)


def configure():
    name = os.environ.get("CACHIX_CACHE_NAME", "")
    key = os.environ.get("CACHIX_PUBLIC_KEY", "")
    if not name and not key:
        warning(
            "Cachix unconfigured: source fallback enabled; this is not verified warm-cache success"
        )
        return None
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", name) or not re.fullmatch(
        re.escape(name) + r"\.cachix\.org-1:[A-Za-z0-9+/]{43}=", key
    ):
        raise ValueError(
            "Set both CACHIX_CACHE_NAME and its verified CACHIX_PUBLIC_KEY"
        )
    url = f"https://{name}.cachix.org"
    os.environ["NIX_CONFIG"] = os.environ.get("NIX_CONFIG", "") + (
        f"\nextra-substituters = {url}\nextra-trusted-public-keys = {key}\n"
        "require-sigs = true\nconnect-timeout = 5\nstalled-download-timeout = 30\n"
        "download-attempts = 2\nnarinfo-cache-negative-ttl = 0\n"
    )
    return url


def diagnose(url, path):
    store_hash = Path(path).name.split("-", 1)[0]
    result = subprocess.run(
        [
            "curl",
            "--silent",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--connect-timeout",
            "5",
            "--max-time",
            "30",
            "--retry",
            "1",
            f"{url}/{store_hash}.narinfo",
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        return "network failure"
    if result.stdout == "404":
        return "cache miss"
    if result.stdout == "200":
        return "download or signature/metadata failure (narinfo exists)"
    return f"remote unavailable (HTTP {result.stdout})"


def info(path, store=None):
    args = ["nix", "path-info", "--json"]
    if store:
        args += ["--store", store]
    metadata = json.loads(run(*args, path, capture=True).stdout).get(path)
    if metadata is None:
        raise ValueError(f"Remote store does not contain {path}")
    return metadata


def verify(tool, output, url):
    if not url:
        return False
    closure = json.loads(
        run("nix", "path-info", "--recursive", "--json", output, capture=True).stdout
    )
    available = []
    for path, expected in closure.items():
        sources = (
            [url]
            if tool == "aspect" and path == output
            else [url, "https://cache.nixos.org"]
        )
        for source in sources:
            try:
                remote = info(path, source)
            except (subprocess.CalledProcessError, ValueError) as error:
                warning(
                    f"{diagnose(source, path)}: {source} {path} ({type(error).__name__})"
                )
                continue
            if remote["narHash"] != expected["narHash"] or sorted(
                remote["references"]
            ) != sorted(expected["references"]):
                warning(f"Remote metadata mismatch: {source} {path}")
                return False
            available.append(
                {
                    "path": path,
                    "source": source,
                    "narHash": remote["narHash"],
                    "references": remote["references"],
                    "signatures": remote.get("signatures", []),
                }
            )
            break
        else:
            warning(
                f"Runtime closure unavailable: {path}; ordinary consumers may build from source"
            )
            return False
    manifest = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / f"nix-{tool}-closure.json"
    manifest.write_text(
        json.dumps({"output": output, "paths": available}, indent=2) + "\n"
    )
    print(f"Remote runtime closure metadata verified: {manifest}")
    return True


class Evidence:
    def __init__(self, tool):
        self.directory = (
            Path(os.environ.get("RUNNER_TEMP", "/tmp")) / f"nix-acceptance-{tool}"
        )
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=False)
        self.commands = []

    def run(self, label, *args):
        started = time.monotonic()
        record = {"command": list(args), "label": label, "exit": None}
        self.commands.append(record)
        journal = self.directory / "commands.json"
        journal.write_text(json.dumps(self.commands, indent=2) + "\n")
        stdout = self.directory / f"{label}.stdout"
        stderr = self.directory / f"{label}.stderr"
        # Stream directly to artifacts so even cancellation retains partial transfer logs.
        with stdout.open("w") as output, stderr.open("w") as errors:
            process = subprocess.run(
                args, stdout=output, stderr=errors, text=True, check=False
            )
        record.update(
            exit=process.returncode, elapsed_seconds=time.monotonic() - started
        )
        journal.write_text(json.dumps(self.commands, indent=2) + "\n")
        if process.returncode:
            raise RuntimeError(
                f"{label} failed ({process.returncode}); see {self.directory}"
            )
        return stdout.read_text(), stderr.read_text()


def downloaded_sources(stderr, allowed):
    sources = {}
    for line in stderr.splitlines():
        if not line.startswith("@nix "):
            continue
        event = json.loads(line[5:])
        if event.get("action") != "start":
            continue
        if event.get("type") == 105:
            raise ValueError("Nix reported a build activity during strict acceptance")
        if event.get("type") != 108:
            continue
        fields = event.get("fields")
        if not isinstance(fields, list) or len(fields) != 2:
            raise ValueError("Unknown Nix substitution log schema")
        path, source = fields
        if (
            not isinstance(path, str)
            or not isinstance(source, str)
            or source not in allowed
        ):
            raise ValueError("Unknown substitution path or source in Nix log")
        sources.setdefault(path, set()).add(source)
    return sources


def accept(tool):
    evidence = Evidence(tool)
    result = {"tool": tool, "status": "failed"}
    try:
        if any(
            os.environ.get(key) for key in ("CACHIX_AUTH_TOKEN", "CACHIX_SIGNING_KEY")
        ):
            raise ValueError("Strict readers must not receive Cachix credentials")
        url = configure()
        if not url:
            raise ValueError(
                "Strict acceptance requires verified public cache configuration"
            )
        # Do not inherit additional substituters, remote builders or credential providers.
        os.environ["NIX_CONFIG"] += (
            f"\nsubstituters = {url} https://cache.nixos.org\n"
            "builders =\nmax-jobs = 0\nfallback = false\n"
            "access-tokens =\nnetrc-file = /dev/null\nsecret-key-files =\n"
        )
        evidence.run("version", "nix", "--version")
        config, _ = evidence.run("effective-config", "nix", "config", "show", "--json")
        settings = json.loads(config)
        if settings["require-sigs"]["value"] is not True:
            raise ValueError("Signature verification must remain enabled")
        output, _ = evidence.run(
            "evaluate",
            "nix",
            "eval",
            "--raw",
            "--no-update-lock-file",
            f".#packages.x86_64-linux.{tool}.outPath",
        )
        if not re.fullmatch(
            r"/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-[^/!\s]+", output
        ) or output.endswith(".drv"):
            raise ValueError("Expected one exact raw non-derivation output path")
        result["output"] = output
        initial, _ = evidence.run("initial-store", "nix", "path-info", "--all")
        initial = set(initial.splitlines())
        if output in initial or os.path.lexists(output):
            raise ValueError(
                "Output already present: this runner cannot prove a fresh download"
            )
        # For raw (non-.drv) paths Nix retrieves the closure without building a known deriver.
        # max-jobs/builders/fallback also document and enforce the acceptance configuration.
        _, stderr = evidence.run(
            "realise",
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
            "builders",
            "",
            "--option",
            "fallback",
            "false",
            "--option",
            "require-sigs",
            "true",
        )
        evidence.run("valid-output", "nix-store", "--check-validity", output)
        sources = downloaded_sources(stderr, {url, "https://cache.nixos.org"})
        if output not in sources or len(sources[output]) != 1:
            raise ValueError("Output download provenance is absent or ambiguous")
        if tool == "aspect" and sources[output] != {url}:
            raise ValueError(
                "Aspect root was not downloaded exclusively from intended Cachix"
            )
        closure, _ = evidence.run(
            "closure", "nix", "path-info", "--recursive", "--json", output
        )
        records = []
        for path in json.loads(closure):
            if path in initial:
                records.append({"path": path, "status": "present-before-realisation"})
            elif path not in sources or len(sources[path]) != 1:
                raise ValueError(
                    f"Closure download provenance absent or ambiguous: {path}"
                )
            else:
                records.append(
                    {
                        "path": path,
                        "source": next(iter(sources[path])),
                        "status": "downloaded",
                    }
                )
        evidence.run(
            "remote-availability", sys.executable, __file__, "verify", tool, output
        )
        manifest = (
            Path(os.environ.get("RUNNER_TEMP", "/tmp")) / f"nix-{tool}-closure.json"
        )
        (evidence.directory / "remote-availability.json").write_bytes(
            manifest.read_bytes()
        )
        for executable in (
            ["aspect"] if tool == "aspect" else ["lcov", "genhtml", "geninfo"]
        ):
            evidence.run(
                f"smoke-{executable}",
                str(Path(output) / "bin" / executable),
                "--version",
            )
        result.update(status="passed", paths=records)
        print(
            f"Verified fresh signed {tool} download: {output}; evidence {evidence.directory}"
        )
    except (ValueError, RuntimeError, KeyError, OSError) as error:
        result["error"] = str(error)
        print(f"Strict acceptance failed: {error}")
    finally:
        (evidence.directory / "result.json").write_text(
            json.dumps(result, indent=2) + "\n"
        )
    return 0 if result["status"] == "passed" else 1


def main():
    command, tool = sys.argv[1:3]
    if tool not in ("aspect", "lcov"):
        raise ValueError("Only repository Aspect and lcov outputs are supported")
    if command == "strict":
        return accept(tool)
    url = configure()
    if command == "validate":
        return 0
    if command == "verify":
        return 0 if verify(tool, sys.argv[3], url) else 1
    if command != "build":
        raise ValueError("Expected validate, build, verify or strict")
    started = time.monotonic()
    # Evaluation/build failures are not optional cache failures.
    output = run(
        "nix",
        "eval",
        "--raw",
        "--no-update-lock-file",
        f".#packages.x86_64-linux.{tool}.outPath",
        capture=True,
    ).stdout
    print(f"Requested {tool}: {output}", flush=True)
    if url and not Path(output).exists():
        try:
            info(output, url)
        except (subprocess.CalledProcessError, ValueError) as error:
            warning(
                f"{diagnose(url, output)}: {output} ({type(error).__name__}); source fallback enabled"
            )
    # Let Nix resolve the Cachix/upstream union. A recursive copy from Cachix alone
    # incorrectly fails when Cachix intentionally omits upstream closure paths.
    # Keep native transfer diagnostics visible, including download/signature errors.
    run("nix", "build", "--no-link", "--no-update-lock-file", "--fallback", f".#{tool}")
    executables = ["aspect"] if tool == "aspect" else ["lcov", "genhtml", "geninfo"]
    for executable in executables:
        run(str(Path(output) / "bin" / executable), "--version")
    # Keep the closure, rather than copying binaries whose store references could be GC'd.
    run(
        "nix-store",
        "--add-root",
        str(Path(os.environ.get("RUNNER_TEMP", "/tmp")) / f"nix-{tool}"),
        "--indirect",
        "--realise",
        output,
    )
    if os.environ.get("GITHUB_PATH"):
        with open(os.environ["GITHUB_PATH"], "a") as handle:
            handle.write(f"{output}/bin\n")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as handle:
            handle.write(f"path={output}\nconfigured={'true' if url else 'false'}\n")
    print(
        f"{tool} ready in {time.monotonic() - started:.1f}s; source build permitted, substitution not asserted"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
