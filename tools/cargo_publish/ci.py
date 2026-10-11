#!/usr/bin/env python3
"""Run real publishing and installed acceptance, retaining selected CI evidence."""

import argparse
import json
import os
import re
import subprocess
import sys
import threading
import uuid
from pathlib import Path


def redact(text):
    text = re.sub(r"--remote_header(?:=|\s+)\S+", "--remote_header=<REDACTED>", text)
    return re.sub(r"(https?://)[^/\s\"]+@", r"\1<REDACTED>@", text)


def copy_evidence(source, destination):
    if source.is_symlink() or not source.is_file():
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    if source.suffix == ".crate":
        destination.write_bytes(source.read_bytes())
    else:
        destination.write_text(redact(source.read_text(errors="replace")))


class Runner:
    def __init__(self, args):
        self.args = args
        self.output = args.output.resolve()
        self.output.mkdir(mode=0o700)
        self.summary = {"status": "incomplete", "stages": [], "collection_errors": []}
        self.save()

    def save(self):
        (self.output / "summary.json").write_text(
            json.dumps(self.summary, indent=2) + "\n"
        )

    def command(self, name, argv):
        stage = self.output / name
        stage.mkdir()
        record = {"name": name, "argv": argv, "returncode": None}
        self.summary["stages"].append(record)
        self.save()
        try:
            process = subprocess.Popen(
                argv,
                cwd=self.args.workspace,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
        except OSError as error:
            (stage / "stderr.log").write_text(redact(str(error)))
            record["returncode"] = 127
        else:

            def stream(source, destination):
                with destination.open("w") as output:
                    for line in source:
                        output.write(redact(line))
                source.close()

            threads = [
                threading.Thread(target=stream, args=(source, stage / filename))
                for source, filename in (
                    (process.stdout, "stdout.log"),
                    (process.stderr, "stderr.log"),
                )
            ]
            for thread in threads:
                thread.start()
            code = process.wait()
            for thread in threads:
                thread.join()
            record["returncode"] = code if code >= 0 else 128 - code
        (stage / "status.json").write_text(json.dumps(record, indent=2) + "\n")
        self.save()
        return record["returncode"]

    def collect_generic(self, root, before, name):
        for run in sorted(set(root.iterdir()) - before) if root.exists() else []:
            if run.is_symlink() or not run.is_dir() or run.name == "registry-cache":
                continue
            destination = self.output / name / "runs" / run.name
            for source in (
                run / "report.json",
                run / "metadata.json",
                *run.glob("[0-9]*-*.log"),
                *(run / "target/package").glob("*.crate"),
            ):
                copy_evidence(source, destination / source.relative_to(run))

    def collect_installed(self, root):
        for name in (
            "artifact.json",
            "validation-report.json",
            "runtime.json",
            "toolchain.txt",
            "commands.log",
            "native-closure.log",
            "native-build-tools.log",
            "installed-version.log",
            "installed-cli.log",
        ):
            copy_evidence(root / name, self.output / "installed" / name)
        copy_evidence(
            root / "verified-handoff/verified.crate",
            self.output / "installed/verified.crate",
        )

    def handoff(self, name, root, before):
        data = json.loads((self.output / name / "stdout.log").read_text())
        if data["status"] != "success":
            raise ValueError("publishing command did not report success")
        run = root / data["run_id"]
        if run not in set(root.iterdir()) - before or run.is_symlink():
            raise ValueError(
                "publishing handoff must identify this invocation's new run"
            )
        archive, report = Path(data["archive"]), Path(data["report"])
        if (
            not archive.is_absolute()
            or not report.is_absolute()
            or archive.is_symlink()
            or report.is_symlink()
            or report != run / "report.json"
            or not archive.resolve().is_relative_to((run / "target/package").resolve())
            or archive.suffix != ".crate"
            or not archive.is_file()
            or not report.is_file()
        ):
            raise ValueError(
                "publishing handoff paths must belong to this invocation's run"
            )
        return archive, report

    def run(self):
        present = [
            str(self.args.workspace / "worktree_pool" / name)
            for name in ("Cargo.toml", "Cargo.lock")
            if (self.args.workspace / "worktree_pool" / name).exists()
            or (self.args.workspace / "worktree_pool" / name).is_symlink()
        ]
        if present:
            (self.output / "consumer-inputs.log").write_text(
                "Consumer Cargo inputs must remain absent: " + ", ".join(present) + "\n"
            )
            return self.finish("consumer-inputs", 1)
        self.summary["consumer_cargo_files_absent"] = True
        code = self.command("bazel-info", [self.args.bazel, "info", "bazel-bin"])
        if code:
            return self.finish("bazel-info", code)
        binary_root = Path(
            (self.output / "bazel-info/stdout.log").read_text().strip()
        ).resolve()
        fixtures = (
            (
                "library",
                "//tools/cargo_publish/fixtures:publish",
                "tools/cargo_publish/fixtures/publish.runs",
            ),
            (
                "binary",
                "//tools/cargo_publish/fixtures:binary_publish",
                "tools/cargo_publish/fixtures/binary_publish.runs",
            ),
            (
                "combined",
                "//tools/cargo_publish/fixtures:combined_publish",
                "tools/cargo_publish/fixtures/combined_publish.runs",
            ),
            ("consumer", "//worktree_pool:publish", "worktree_pool/publish.runs"),
        )
        for name, label, path in fixtures:
            root = binary_root / path
            before = set(root.iterdir()) if root.exists() else set()
            argv = [self.args.bazel, "run", label, "--remote_download_outputs=all"]
            if name == "library":
                argv += [
                    "--",
                    "--offline",
                    "--registry-cache",
                    str(
                        binary_root
                        / "tools/cargo_publish/ci-empty-probe-cache"
                        / str(uuid.uuid4())
                    ),
                ]
            code = self.command(name, argv)
            try:
                self.collect_generic(root, before, name)
            except OSError as error:
                self.summary["collection_errors"].append(redact(str(error)))
                code = code or 1
            if code:
                return self.finish(name, code)
            try:
                archive, report = self.handoff(name, root, before)
            except (OSError, ValueError, KeyError, TypeError) as error:
                (self.output / name / "handoff-error.log").write_text(
                    redact(str(error))
                )
                return self.finish(name, 1)
        installation = (
            binary_root / "worktree_pool/install/ci-acceptance-runs" / str(uuid.uuid4())
        )
        installation.parent.mkdir(parents=True, exist_ok=True)
        code = self.command(
            "installed",
            [
                self.args.bazel,
                "run",
                "//worktree_pool/install:acceptance",
                "--remote_download_outputs=all",
                "--",
                "--archive",
                str(archive),
                "--report",
                str(report),
                "--output",
                str(installation),
            ],
        )
        try:
            self.collect_installed(installation)
        except OSError as error:
            self.summary["collection_errors"].append(redact(str(error)))
            code = code or 1
        return self.finish("installed", code)

    def finish(self, name, code):
        self.summary.update(status="failure" if code else "success", returncode=code)
        if code:
            self.summary["failed_stage"] = name
        self.save()
        print(
            json.dumps({"status": self.summary["status"], "evidence": str(self.output)})
        )
        return code


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bazel", default="bazel")
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return Runner(parser.parse_args()).run()


if __name__ == "__main__":
    sys.exit(main())
