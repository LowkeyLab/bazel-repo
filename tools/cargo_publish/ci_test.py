#!/usr/bin/env python3
"""Exercise CI exit status and retained evidence through its public command."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class PublishingCICommandTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.workspace = self.root / "workspace"
        self.workspace.mkdir()
        self.bin = self.root / "bazel-bin"
        self.bin.mkdir()
        self.bazel = self.root / "bazel"
        self.bazel.write_text(
            "#!"
            + sys.executable
            + "\n"
            + r"""
import json
import os
import sys
from pathlib import Path
root = Path(os.environ["FAKE_BAZEL_BIN"])
if sys.argv[1] == "info":
    print(root)
    sys.exit(0)
target = sys.argv[2]
mode = os.environ["FAKE_MODE"]
if target == "//worktree_pool/install:acceptance":
    argv = sys.argv
    archive = Path(argv[argv.index("--archive") + 1])
    report = Path(argv[argv.index("--report") + 1])
    assert archive == root / "worktree_pool/publish.runs/new-consumer/target/package/consumer.crate"
    assert report == archive.parents[2] / "report.json"
    output = Path(argv[argv.index("--output") + 1])
    output.mkdir()
    (output / "artifact.json").write_text(json.dumps({"generic_run_id": "new-consumer"}))
    if mode == "installed-failure":
        (output / "installed-cli.log").write_text("public CLI assertion failed\n")
        print("installed behavior failure", file=sys.stderr)
        sys.exit(23)
    (output / "installed-cli.log").write_text("124 passed\n")
    (output / "runtime.json").write_text(json.dumps({"helper_scope": "Bazel test_support children; public CLI commands use Cargo-installed artifact"}))
    (output / "installation-configuration.json").write_text("private config")
    sys.exit(0)
labels = {
    "//tools/cargo_publish/fixtures:publish": ("library", "tools/cargo_publish/fixtures/publish.runs"),
    "//tools/cargo_publish/fixtures:binary_publish": ("binary", "tools/cargo_publish/fixtures/binary_publish.runs"),
    "//tools/cargo_publish/fixtures:combined_publish": ("combined", "tools/cargo_publish/fixtures/combined_publish.runs"),
    "//worktree_pool:publish": ("consumer", "worktree_pool/publish.runs"),
}
name, path = labels[target]
run = root / path / ("new-failed-run" if mode in ("package-failure", "collection-failure") else "new-" + name)
run.mkdir(parents=True)
archive = run / "target/package" / (name + ".crate")
archive.parent.mkdir(parents=True)
archive.write_bytes(b"controlled external archive")
(run / "report.json").write_text(json.dumps({"status": "incomplete" if mode in ("package-failure", "collection-failure") else "success", "run_id": run.name}))
(run / "1-cargo_package.log").write_text("missing crate in offline cache\n" if mode in ("package-failure", "collection-failure") else "package verified\n")
(run / "cargo-home").mkdir()
(run / "cargo-home/credentials.toml").write_text("private credential")
if mode in ("package-failure", "collection-failure"):
    if mode == "collection-failure":
        (Path(os.environ["FAKE_EVIDENCE"]) / "library/runs").write_text("blocked collector destination")
    print("--remote_header=x-buildbuddy-api-key=dummy-header-secret", file=sys.stderr)
    print("real command failure", file=sys.stderr)
    sys.exit(17)
print(json.dumps({"status": "success", "run_id": run.name, "archive": str(archive), "report": str(run / "report.json")}))
"""
        )
        self.bazel.chmod(0o700)

    def invoke(self, mode="package-failure"):
        driver = Path(os.environ["TEST_SRCDIR"]) / "_main" / os.environ["CI_RUNNER"]
        return subprocess.run(
            [
                str(driver),
                "--bazel",
                str(self.bazel),
                "--workspace",
                str(self.workspace),
                "--output",
                str(self.root / "evidence"),
            ],
            env=dict(
                os.environ,
                FAKE_BAZEL_BIN=str(self.bin),
                FAKE_MODE=mode,
                FAKE_EVIDENCE=str(self.root / "evidence"),
            ),
            capture_output=True,
            text=True,
            check=False,
        )

    def test_packaging_failure_retains_incomplete_report_and_original_exit(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 17, result.stderr)
        evidence = self.root / "evidence"
        summary = json.loads((evidence / "summary.json").read_text())
        self.assertEqual(summary["status"], "failure")
        self.assertEqual(summary["failed_stage"], "library")
        self.assertEqual(summary["returncode"], 17)
        report = evidence / "library/runs/new-failed-run/report.json"
        self.assertEqual(json.loads(report.read_text())["status"], "incomplete")
        self.assertIn(
            "missing crate", (report.parent / "1-cargo_package.log").read_text()
        )
        self.assertFalse(list(evidence.rglob("credentials.toml")))
        self.assertFalse((evidence / "installed").exists())

    def test_serial_matrix_hands_exact_consumer_pair_to_installed_acceptance(self):
        old = self.bin / "tools/cargo_publish/fixtures/publish.runs/old-success"
        old.mkdir(parents=True)
        (old / "report.json").write_text("stale report")
        result = self.invoke("success")
        self.assertEqual(result.returncode, 0, result.stderr)
        evidence = self.root / "evidence"
        summary = json.loads((evidence / "summary.json").read_text())
        self.assertEqual(summary["status"], "success")
        stages = {stage["name"]: stage["returncode"] for stage in summary["stages"]}
        for name in ("library", "binary", "combined", "consumer", "installed"):
            self.assertEqual(stages[name], 0)
        self.assertEqual(len(list(evidence.rglob("*.crate"))), 4)
        self.assertFalse(list(evidence.rglob("old-success")))
        self.assertFalse(list(evidence.rglob("credentials.toml")))
        self.assertFalse(list(evidence.rglob("installation-configuration.json")))
        artifact = json.loads((evidence / "installed/artifact.json").read_text())
        self.assertEqual(artifact["generic_run_id"], "new-consumer")
        self.assertIn(
            "124 passed", (evidence / "installed/installed-cli.log").read_text()
        )
        self.assertIn(
            "Bazel test_support children",
            (evidence / "installed/runtime.json").read_text(),
        )

    def test_checked_in_consumer_cargo_input_rejects_certification(self):
        consumer = self.workspace / "worktree_pool"
        consumer.mkdir()
        (consumer / "Cargo.toml").write_text("unexpected handwritten consumer input")
        result = self.invoke("success")
        self.assertEqual(result.returncode, 1, result.stderr)
        summary = json.loads((self.root / "evidence/summary.json").read_text())
        self.assertEqual(summary["failed_stage"], "consumer-inputs")
        self.assertIn(
            "Cargo.toml", (self.root / "evidence/consumer-inputs.log").read_text()
        )
        self.assertFalse((self.root / "evidence/library").exists())

    def test_installed_failure_retains_behavior_log_and_original_exit(self):
        result = self.invoke("installed-failure")
        self.assertEqual(result.returncode, 23, result.stderr)
        evidence = self.root / "evidence"
        summary = json.loads((evidence / "summary.json").read_text())
        self.assertEqual(summary["failed_stage"], "installed")
        self.assertEqual(summary["returncode"], 23)
        self.assertIn(
            "public CLI assertion failed",
            (evidence / "installed/installed-cli.log").read_text(),
        )
        self.assertFalse((evidence / "installed/runtime.json").exists())
        self.assertEqual(len(list(evidence.rglob("*.crate"))), 4)

    def test_collector_error_does_not_replace_failed_command_status(self):
        result = self.invoke("collection-failure")
        self.assertEqual(result.returncode, 17, result.stderr)
        summary = json.loads((self.root / "evidence/summary.json").read_text())
        self.assertEqual(summary["returncode"], 17)
        self.assertTrue(summary["collection_errors"])
        log = (self.root / "evidence/library/stderr.log").read_text()
        self.assertIn("<REDACTED>", log)
        self.assertNotIn("dummy-header-secret", log)


if __name__ == "__main__":
    unittest.main()
