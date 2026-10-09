#!/usr/bin/env python3
"""Run real, paired complete cycles; never remove state outside the new run root."""

import argparse
import concurrent.futures
import gzip
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path

from worktree_pool.benchmark.evaluate import WORKLOADS, assess

TARGET = "//nicknamer/server/lib:lib"
EDIT_FILE = "nicknamer/server/lib/src/lib.rs"
CACHE_FLAGS = ["--repo_contents_cache="]


class Experiment:
    def __init__(self, args):
        self.args = args
        self.root = args.root.resolve()
        self.source = args.source.resolve()
        self.binary = args.pool_binary.resolve()
        if args.phase == "warmup":
            self.root.mkdir(parents=True, exist_ok=False)
            (self.root / "logs").mkdir()
        self.lock = threading.Lock()
        self.disposable_lock = threading.Lock()
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("GIT_")
        }
        self.git_env = dict(
            self.env,
            GIT_CONFIG_GLOBAL="/dev/null",
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="Benchmark",
            GIT_AUTHOR_EMAIL="benchmark@example.invalid",
            GIT_COMMITTER_NAME="Benchmark",
            GIT_COMMITTER_EMAIL="benchmark@example.invalid",
            GIT_AUTHOR_DATE="2026-10-09T00:00:00Z",
            GIT_COMMITTER_DATE="2026-10-09T00:00:00Z",
        )
        self.pool_env = dict(
            self.env,
            XDG_DATA_HOME=str(self.root / "data"),
            XDG_STATE_HOME=str(self.root / "state"),
            XDG_CONFIG_HOME=str(self.root / "config"),
        )
        self.pairs = []
        self.failures = []
        self.condition_warnings = []
        self.slots = {}
        self.pre_heads = {}
        self.maximum_owned_bytes = 0
        self.maximum_server_rss = 0
        self.base_commit = args.fixture_commit
        self.origin = self.root / "origin.git"
        self.pool_context = self.root / "pool-context"
        self.disposable_context = self.root / "disposable-context"
        self.repo_id = None
        self.metadata = {
            "schema_version": 1,
            "quiet_window": args.quiet_window,
            "target": TARGET,
            "fixture_commit": self.base_commit,
            "tool_sha256": hashlib.sha256(self.binary.read_bytes()).hexdigest(),
            "tool_commit": args.tool_commit,
            "transport": "local pinned origin",
            "cache_flags": CACHE_FLAGS,
            "python_version": sys.version,
            "rust_toolchain_version": "1.96.0 (pinned MODULE.bazel)",
            "minimum_free_gib": args.minimum_free_gib,
            "minimum_available_ram_gib": args.minimum_available_ram_gib,
            "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "host_before": self.host(),
            "resource_snapshots": [],
        }
        if args.phase == "sample":
            previous = json.loads((self.root / "metadata.json").read_text())
            if not previous.get("warmup_complete") or previous.get(
                "sampling_started_utc"
            ):
                raise RuntimeError("sampling requires a complete unused warmup")
            if any(
                previous[key] != self.metadata[key]
                for key in ("tool_sha256", "tool_commit", "fixture_commit")
            ):
                raise RuntimeError("warmup/sample provenance differs")
            self.metadata = previous
            self.revision_commit = previous["revision_commit"]
            self.repo_id = previous["repository_id"]
            self.maximum_owned_bytes = previous["maximum_owned_bytes"]
            self.maximum_server_rss = previous["maximum_server_rss"]
            self.condition_warnings = previous["condition_warnings"]
            for slot in previous["resource_snapshots"][-1]["slots"]:
                identity = slot["worktree_id"]
                self.slots[identity] = (
                    self.root
                    / "data/worktree-pool/worktrees"
                    / self.repo_id
                    / identity,
                    self.root / "bases" / identity,
                )
            self.metadata["sampling_quiet_window"] = args.quiet_window
            self.metadata["sampling_minimum_free_gib"] = args.minimum_free_gib
            self.metadata["sampling_minimum_available_ram_gib"] = (
                args.minimum_available_ram_gib
            )
        self.save("metadata.json", self.metadata)

    def sanitize(self, value):
        text = str(value)
        for path, replacement in (
            (self.root, "$RUN_ROOT"),
            (self.source, "$SOURCE"),
            (self.binary, "$POOL_BINARY"),
            (Path.home(), "$USER_HOME"),
        ):
            text = text.replace(str(path), replacement)
        text = re.sub(
            r"(?im)^.*(?:remote_header|api[-_]?key|authorization:).*$",
            "[credential-bearing line omitted]",
            text,
        )
        return re.sub(r"\x1b\[[0-9;]*[a-zA-Z]", "", text)

    def save(self, name, value):
        (self.root / name).write_text(json.dumps(value, indent=2) + "\n")

    def append(self, name, value):
        with self.lock, (self.root / name).open("a") as stream:
            stream.write(json.dumps(value, separators=(",", ":")) + "\n")

    def command(self, argv, cwd=None, env=None, pool=False):
        command_id = uuid.uuid4().hex
        started = time.monotonic_ns()
        try:
            result = subprocess.run(
                [str(arg) for arg in argv],
                cwd=cwd or self.root,
                env=env or self.env,
                capture_output=True,
                check=False,
                timeout=1200,
            )
        except subprocess.TimeoutExpired:
            self.append(
                "commands.jsonl",
                {
                    "id": command_id,
                    "argv": [self.sanitize(arg) for arg in argv],
                    "outcome": "timeout",
                    "elapsed_ns": time.monotonic_ns() - started,
                },
            )
            raise RuntimeError(f"command timeout: {argv[0]}") from None
        elapsed = time.monotonic_ns() - started
        stdout = result.stdout.decode(errors="replace")
        stderr = result.stderr.decode(errors="replace")
        # Pool JSON contains lossless absolute-path byte arrays; retain only the
        # selected public facts in cycle records, not that private full response.
        if pool:
            try:
                envelope = json.loads(stdout)
                facts = {
                    key: envelope.get(key)
                    for key in ("command", "outcome", "reason_code")
                }
                log = json.dumps(facts) + "\n" + stderr
            except json.JSONDecodeError:
                log = (
                    "Pool stdout was not valid JSON; private raw response omitted.\n"
                    + stderr
                )
        else:
            log = stdout + stderr
        with gzip.open(self.root / "logs" / f"{command_id}.log.gz", "wt") as stream:
            stream.write(self.sanitize(log))
        self.append(
            "commands.jsonl",
            {
                "id": command_id,
                "argv": [self.sanitize(arg) for arg in argv],
                "cwd": self.sanitize(cwd or self.root),
                "status": result.returncode,
                "elapsed_ns": elapsed,
                "log": f"logs/{command_id}.log.gz",
            },
        )
        if re.search(
            r"(?i)(remote cache.*(?:failed|unavailable|timed out)|UNAVAILABLE:|DEADLINE_EXCEEDED:)",
            stderr,
        ):
            self.condition_warnings.append(command_id)
        if result.returncode:
            raise RuntimeError(
                f"command failed ({result.returncode}); see {command_id}"
            )
        return stdout.strip()

    def git(self, cwd, *args):
        return self.command(["git", "-C", cwd, *args], env=self.git_env)

    def pool(self, *args):
        result = json.loads(
            self.command([self.binary, "--json", *args], env=self.pool_env, pool=True)
        )
        if result["outcome"] != "completed":
            raise RuntimeError(
                f"pool outcome {result['outcome']}: {result['reason_code']}"
            )
        return result

    def native(self, cwd, base, executable, *args):
        return self.command(
            [
                "nix",
                "develop",
                "--command",
                executable,
                f"--output_base={base}",
                args[0],
                *CACHE_FLAGS,
                *args[1:],
            ],
            cwd=cwd,
        )

    def host(self):
        memory = {}
        for line in Path("/proc/meminfo").read_text().splitlines():
            key, value = line.split(":", 1)
            if key in ("MemTotal", "MemAvailable", "SwapTotal"):
                memory[key] = int(value.split()[0]) * 1024
        return {
            "disk_available_bytes": shutil.disk_usage(self.root.parent).free,
            "memory_bytes": memory,
        }

    def headroom(self, additional_slots=0):
        snapshot = self.host()
        snapshot["reserved_new_state_bytes"] = (
            additional_slots * self.maximum_owned_bytes
        )
        snapshot["reserved_server_rss_bytes"] = (
            additional_slots * self.maximum_server_rss
        )
        self.append("headroom.jsonl", snapshot)
        if (
            snapshot["disk_available_bytes"]
            < self.args.minimum_free_gib * 1024**3
            + snapshot["reserved_new_state_bytes"]
        ):
            raise RuntimeError("disk headroom below declared experiment minimum")
        if (
            snapshot["memory_bytes"]["MemAvailable"]
            < self.args.minimum_available_ram_gib * 1024**3
            + snapshot["reserved_server_rss_bytes"]
        ):
            raise RuntimeError("available RAM below declared experiment minimum")

    def configure(self, path):
        config = path / "user.bazelrc"
        expected = "common --repo_contents_cache=\n"
        if config.exists() and config.read_text() != expected:
            raise RuntimeError("unexpected private fixture configuration")
        if not config.exists():
            config.write_text(expected)
        self.git(path, "check-ignore", "--quiet", "user.bazelrc")

    def edit(self, path, base, port):
        source = path / EDIT_FILE
        original = source.read_text()
        if original.count("8080") != 1:
            raise RuntimeError("fixture edit literal mismatch")
        source.write_text(original.replace("8080", str(port)))
        self.native(path, base, "bazel", "run", "//:gazelle")
        self.native(path, base, "aspect", "format", "--scope=all")
        changed = self.git(path, "diff", "--name-only").splitlines()
        if changed != [EDIT_FILE]:
            raise RuntimeError(f"unexpected fixture changes: {changed}")
        return hashlib.sha256(self.git(path, "diff", "--binary").encode()).hexdigest()

    def footprint(self, path, base):
        allocated = sum(
            int(line.split()[0])
            for line in self.command(["du", "-s", "-B1", path, base]).splitlines()
        )
        pid = int(self.native(path, base, "bazel", "info", "server_pid"))
        status = Path(f"/proc/{pid}/status").read_text()
        rss = re.search(r"^VmRSS:\s+(\d+) kB$", status, re.MULTILINE)
        resident = int(rss[1]) * 1024 if rss else None
        with self.lock:
            self.maximum_owned_bytes = max(self.maximum_owned_bytes, allocated)
            if resident is not None:
                self.maximum_server_rss = max(self.maximum_server_rss, resident)
        observation = {
            "checkout": self.sanitize(path),
            "output_base": self.sanitize(base),
            "allocated_bytes": allocated,
            "server_rss_bytes": resident,
        }
        self.append("resource_observations.jsonl", observation)
        return observation

    def cleanup_disposable(self, path, base):
        # The experiment is the human owner of these new disposable fixtures.
        # Product release never cleans or shuts down anything.
        if not path.is_relative_to(self.root) or not base.is_relative_to(
            self.root / "bases"
        ):
            raise RuntimeError("cleanup escaped experiment-owned root")
        actual = self.native(path, base, "bazel", "info", "output_base")
        if Path(actual).resolve() != base.resolve():
            raise RuntimeError("output-base cleanup assertion failed")
        self.native(path, base, "bazel", "clean", "--expunge")

    def setup(self):
        self.headroom()
        self.metadata["versions"] = {
            "git": self.command(["git", "--version"]),
            "kernel": self.command(["uname", "-srmo"]),
            "nix": self.command(["nix", "--version"]),
        }
        self.command(
            ["git", "clone", "--shared", "--bare", self.source, self.origin],
            env=self.git_env,
        )
        self.git(self.origin, "update-ref", "refs/heads/main", self.base_commit)
        self.git(self.origin, "symbolic-ref", "HEAD", "refs/heads/main")
        self.command(
            ["git", "clone", "--shared", self.origin, self.pool_context],
            env=self.git_env,
        )
        self.configure(self.pool_context)
        self.metadata["configuration_digests"] = {
            name: hashlib.sha256((self.pool_context / name).read_bytes()).hexdigest()
            for name in (
                ".bazelrc",
                "tools/preset.bazelrc",
                "flake.lock",
                "MODULE.bazel",
                ".bazelversion",
            )
        }
        self.metadata["versions"].update(
            {
                executable: self.command(
                    ["nix", "develop", "--command", executable, "--version"],
                    cwd=self.pool_context,
                )
                for executable in ("bazel", "aspect")
            }
        )
        prep_base = self.root / "bases" / "fixture-preparation"
        self.git(self.pool_context, "checkout", "--detach", self.base_commit)
        self.edit(self.pool_context, prep_base, 8081)
        self.git(self.pool_context, "add", EDIT_FILE)
        self.git(
            self.pool_context, "commit", "-m", "test: pin benchmark revision fixture"
        )
        self.revision_commit = self.git(self.pool_context, "rev-parse", "HEAD")
        self.git(
            self.origin,
            "fetch",
            str(self.pool_context),
            f"{self.revision_commit}:refs/heads/benchmark-revision",
        )
        self.git(self.pool_context, "checkout", "--detach", self.base_commit)
        self.native(self.pool_context, prep_base, "bazel", "clean", "--expunge")
        self.command(
            ["git", "clone", "--shared", self.origin, self.disposable_context],
            env=self.git_env,
        )
        self.pool("catalog", "init")
        registered = self.pool("repo", "register", str(self.pool_context))
        self.repo_id = registered["context"]["repository_id"]
        self.metadata["repository_id"] = self.repo_id
        self.metadata["revision_commit"] = self.revision_commit
        self.metadata["base_tree"] = self.git(
            self.origin, "rev-parse", f"{self.base_commit}^{{tree}}"
        )
        self.metadata["revision_tree"] = self.git(
            self.origin, "rev-parse", f"{self.revision_commit}^{{tree}}"
        )
        self.metadata["catalog_initial_event_count"] = len(
            self.pool("events", "list")["data"]["events"]
        )
        self.metadata["catalog_initial_bytes"] = (
            (self.root / "data/worktree-pool/catalog.redb").stat().st_size
        )
        self.save("metadata.json", self.metadata)

    def acquire(self, arm, revision):
        if arm == "pooled":
            envelope = self.pool("--repo", self.repo_id, "acquire", revision)
            assignment = envelope["data"]["assignment"]
            if assignment["resolved_commit"] != revision:
                raise RuntimeError("pool resolved a different commit")
            path = Path(os.fsdecode(bytes(assignment["path"]["bytes"])))
            base = self.root / "bases" / assignment["worktree_id"]
            self.slots[assignment["worktree_id"]] = (path, base)
            return (
                path,
                base,
                assignment["assignment_handle"],
                assignment["worktree_id"],
            )
        with self.disposable_lock:
            self.git(
                self.disposable_context,
                "fetch",
                "--no-tags",
                "origin",
                "+refs/heads/main:refs/remotes/origin/main",
            )
            if (
                self.git(
                    self.disposable_context, "rev-parse", "refs/remotes/origin/main"
                )
                != self.base_commit
            ):
                raise RuntimeError("disposable refresh resolved a different main")
        identity = uuid.uuid4().hex
        path = self.root / "disposable" / identity
        base = self.root / "bases" / f"disposable-{identity}"
        self.git(
            self.disposable_context, "worktree", "add", "--detach", str(path), revision
        )
        return path, base, None, None

    def finish(self, arm, path, base, handle, edited):
        start = time.monotonic_ns()
        if edited:
            self.git(path, "add", EDIT_FILE)
            self.git(path, "commit", "-m", "test: finish benchmark caller edit")
        preparation = time.monotonic_ns() - start
        start = time.monotonic_ns()
        cleanup = 0
        if arm == "pooled":
            self.pool("release", handle)
        else:
            self.footprint(path, base)
            self.cleanup_disposable(path, base)
            cleanup = time.monotonic_ns() - start
            start = time.monotonic_ns()
            self.git(self.disposable_context, "worktree", "remove", str(path))
        return {
            "caller_finish_ns": preparation,
            "release_ns": time.monotonic_ns() - start,
            "disposable_cleanup_ns": cleanup,
        }

    def caller(self, arm, revision, edited, start_barrier=None, built_barrier=None):
        if start_barrier:
            start_barrier.wait(timeout=1200)
        start = time.monotonic_ns()
        path, base, handle, worktree_id = self.acquire(arm, revision)
        acquired = time.monotonic_ns()
        self.configure(path)
        observed_head = self.git(path, "rev-parse", "HEAD")
        if observed_head != revision:
            raise RuntimeError("checkout commit mismatch")
        edit_digest = self.edit(path, base, 8082) if edited else None
        prepared = time.monotonic_ns()
        self.native(path, base, "aspect", "build", TARGET)
        built = time.monotonic_ns()
        result = {
            "start_ns": start,
            "built_ns": built,
            "total_ns": built - start,
            "acquisition_ns": acquired - start,
            "preparation_ns": prepared - acquired,
            "build_ns": built - prepared,
            "resolved_commit": revision,
            "edit_digest": edit_digest,
            "selected_worktree_id": worktree_id,
            "pre_acquisition_head": self.pre_heads.get(worktree_id),
            "observed_post_acquisition_head": observed_head,
            "checkout": self.sanitize(path),
            "output_base": self.sanitize(base),
            "assignment_handle": handle,
        }
        if built_barrier:
            built_barrier.wait(timeout=1200)
        result.update(self.finish(arm, path, base, handle, edited))
        return result

    def cycle(self, arm, revision, edited=False, callers=1):
        self.headroom(additional_slots=callers if arm == "disposable" else 0)
        self.pre_heads = {
            identity: self.git(path, "rev-parse", "HEAD")
            for identity, (path, _) in self.slots.items()
        }
        if callers == 1:
            results = [self.caller(arm, revision, edited)]
        else:
            start_barrier = threading.Barrier(callers)
            built_barrier = threading.Barrier(callers)
            with concurrent.futures.ThreadPoolExecutor(max_workers=callers) as executor:
                futures = [
                    executor.submit(
                        self.caller, arm, revision, edited, start_barrier, built_barrier
                    )
                    for _ in range(callers)
                ]
                try:
                    results = [
                        future.result()
                        for future in concurrent.futures.as_completed(futures)
                    ]
                except Exception:
                    start_barrier.abort()
                    built_barrier.abort()
                    raise
        # Caller positions follow completion order, avoiding scheduler-dependent
        # assignment of "caller 0" between arms.
        results.sort(key=lambda item: item["total_ns"])
        return {
            "total_ns": max(item["built_ns"] for item in results)
            - min(item["start_ns"] for item in results),
            "callers": results,
        }

    def resources(self, label):
        slots = []
        for identity, (path, base) in self.slots.items():

            def size(target, apparent=False):
                flags = ["du", "-s", "-B1"] + (["--apparent-size"] if apparent else [])
                return int(self.command([*flags, target]).split()[0])

            pid = int(self.native(path, base, "bazel", "info", "server_pid"))
            status = Path(f"/proc/{pid}/status").read_text()
            rss = re.search(r"^VmRSS:\s+(\d+) kB$", status, re.MULTILINE)
            slots.append(
                {
                    "worktree_id": identity,
                    "checkout_allocated_bytes": size(path),
                    "checkout_apparent_bytes": size(path, True),
                    "output_allocated_bytes": size(base),
                    "output_apparent_bytes": size(base, True),
                    "server_rss_bytes": int(rss[1]) * 1024 if rss else None,
                }
            )
        record = {
            "label": label,
            "slots": slots,
            "host": self.host(),
            "catalog_bytes": (self.root / "data/worktree-pool/catalog.redb")
            .stat()
            .st_size,
            "catalog_event_count": len(self.pool("events", "list")["data"]["events"]),
            "shared_cache_and_nix_store_attribution": "unknown; preexisting shared resources preserved",
        }
        self.metadata["resource_snapshots"].append(record)
        self.save("metadata.json", self.metadata)

    def warmup(self):
        assignments = []
        for _ in range(4):
            self.headroom(additional_slots=1)
            path, base, handle, _ = self.acquire("pooled", self.base_commit)
            self.configure(path)
            start = time.monotonic_ns()
            self.native(path, base, "aspect", "build", TARGET)
            self.append(
                "warmup.jsonl",
                {
                    "kind": "four_slot",
                    "checkout": self.sanitize(path),
                    "build_ns": time.monotonic_ns() - start,
                },
            )
            assignments.append(handle)
            self.footprint(path, base)
        for handle in reversed(assignments):
            self.pool("release", handle)
        self.resources("four_slots_after_base_warmup")
        for arm, count in (("pooled", 4), ("disposable", 1)):
            for index in range(count):
                self.append(
                    "warmup.jsonl",
                    {
                        "kind": "small_edit",
                        "arm": arm,
                        "index": index,
                        "cycle": self.cycle(arm, self.base_commit, edited=True),
                    },
                )
        self.resources("after_documented_warmup")
        self.metadata["maximum_owned_bytes"] = self.maximum_owned_bytes
        self.metadata["maximum_server_rss"] = self.maximum_server_rss
        self.metadata["condition_warnings"] = self.condition_warnings
        self.metadata["warmup_complete"] = True
        self.save("metadata.json", self.metadata)

    def sample(self):
        for workload in WORKLOADS:
            for batch in (1, 2):
                for index in range(10):
                    if workload in ("revision_change", "revision_return"):
                        prior = (
                            self.base_commit
                            if workload == "revision_change"
                            else self.revision_commit
                        )
                        self.append(
                            "setup_cycles.jsonl",
                            {
                                "workload": workload,
                                "batch": batch,
                                "pair": index,
                                "cycle": self.cycle("pooled", prior),
                            },
                        )
                    revision = (
                        self.revision_commit
                        if workload == "revision_change"
                        else self.base_commit
                    )
                    arms = (
                        ("pooled", "disposable")
                        if index % 2 == 0
                        else ("disposable", "pooled")
                    )
                    pair = {
                        "workload": workload,
                        "batch": batch,
                        "pair": index,
                        "order": arms,
                        "revision": revision,
                    }
                    for arm in arms:
                        pair[arm] = self.cycle(
                            arm,
                            revision,
                            edited=workload == "small_edit",
                            callers=2 if workload == "two_callers" else 1,
                        )
                        self.append(
                            "cycles.jsonl",
                            {
                                "workload": workload,
                                "batch": batch,
                                "pair": index,
                                "arm": arm,
                                "cycle": pair[arm],
                            },
                        )
                    if (
                        pair["pooled"]["callers"][0]["edit_digest"]
                        != pair["disposable"]["callers"][0]["edit_digest"]
                    ):
                        raise RuntimeError("paired source edits differ")
                    self.pairs.append(pair)
                    self.append("pairs.jsonl", pair)
                    self.report()
        self.resources("after_all_samples")

    def report(self):
        summaries = {}
        for workload in WORKLOADS:
            values = [pair for pair in self.pairs if pair["workload"] == workload]
            if not values:
                continue
            summary = {}
            for arm in ("pooled", "disposable"):
                durations = [pair[arm]["total_ns"] / 1e9 for pair in values]
                ordered = sorted(durations)
                phases = {}
                for phase in (
                    "total_ns",
                    "acquisition_ns",
                    "preparation_ns",
                    "build_ns",
                    "release_ns",
                    "caller_finish_ns",
                    "disposable_cleanup_ns",
                ):
                    samples = [
                        caller[phase] / 1e9
                        for pair in values
                        for caller in pair[arm]["callers"]
                    ]
                    phases[phase] = {
                        "median_seconds": statistics.median(samples),
                        "p95_seconds": sorted(samples)[
                            math.ceil(len(samples) * 0.95) - 1
                        ],
                        "minimum_seconds": min(samples),
                        "maximum_seconds": max(samples),
                        "sample_stdev_seconds": statistics.stdev(samples)
                        if len(samples) > 1
                        else None,
                    }
                summary[arm] = {
                    "caller_phases": phases,
                    "count": len(durations),
                    "median_seconds": statistics.median(durations),
                    "p95_seconds": ordered[math.ceil(len(ordered) * 0.95) - 1],
                    "minimum_seconds": min(durations),
                    "maximum_seconds": max(durations),
                    "sample_stdev_seconds": statistics.stdev(durations)
                    if len(durations) > 1
                    else None,
                }
            savings = [
                (pair["disposable"]["total_ns"] - pair["pooled"]["total_ns"]) / 1e9
                for pair in values
            ]
            relative = [
                100 * (1 - pair["pooled"]["total_ns"] / pair["disposable"]["total_ns"])
                for pair in values
            ]
            summary["strict_pooled_wins_by_batch"] = [
                sum(
                    pair["pooled"]["total_ns"] < pair["disposable"]["total_ns"]
                    for pair in values
                    if pair["batch"] == batch
                )
                for batch in (1, 2)
            ]
            summary["strict_pooled_slowdowns_by_batch"] = [
                sum(
                    pair["pooled"]["total_ns"] > pair["disposable"]["total_ns"]
                    for pair in values
                    if pair["batch"] == batch
                )
                for batch in (1, 2)
            ]
            summary["median_paired_savings_seconds"] = statistics.median(savings)
            summary["median_paired_savings_percent"] = statistics.median(relative)
            summaries[workload] = summary
        assessment = assess(
            self.pairs, valid=not self.failures and not self.condition_warnings
        )
        self.save(
            "report.json",
            {
                "assessment": assessment,
                "summaries": summaries,
                "failure_count": len(self.failures),
                "condition_warnings": self.condition_warnings,
                "resource_acceptance": "pending explicit maintainer decision",
                "limitations": [
                    "local origin transport excludes GitHub network latency",
                    "shared remote/archive/page caches and host contention",
                    "eight-of-ten consistency is not statistical confidence",
                    "idle server RSS includes shared physical pages",
                ],
            },
        )

    def run(self):
        try:
            if self.args.phase == "warmup":
                self.setup()
                self.warmup()
            else:
                self.headroom(additional_slots=2)
                self.metadata["sampling_started_utc"] = time.strftime(
                    "%Y-%m-%dT%H:%M:%SZ", time.gmtime()
                )
                self.save("metadata.json", self.metadata)
                self.sample()
        except Exception as error:
            failure = {
                "message": self.sanitize(error),
                "completed_pairs": len(self.pairs),
                "host": self.host(),
            }
            self.failures.append(failure)
            self.append("failures.jsonl", failure)
            if self.slots:
                try:
                    self.resources("partial_after_failure")
                except (
                    OSError,
                    RuntimeError,
                    ValueError,
                    subprocess.SubprocessError,
                ) as diagnostic_error:
                    self.append(
                        "failures.jsonl",
                        {"resource_measurement_error": self.sanitize(diagnostic_error)},
                    )
            self.report()
            raise
        finally:
            self.metadata["host_after"] = self.host()
            self.save("metadata.json", self.metadata)
        self.report()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root", type=Path, required=True, help="new private experiment directory"
    )
    parser.add_argument(
        "--source", type=Path, required=True, help="read-only repository checkout"
    )
    parser.add_argument("--phase", choices=("warmup", "sample"), required=True)
    parser.add_argument("--pool-binary", type=Path, required=True)
    parser.add_argument("--fixture-commit", required=True)
    parser.add_argument("--tool-commit", required=True)
    parser.add_argument(
        "--quiet-window",
        required=True,
        help="record coordinator authorization and UTC window",
    )
    parser.add_argument("--minimum-free-gib", type=int, default=8)
    parser.add_argument("--minimum-available-ram-gib", type=int, default=2)
    Experiment(parser.parse_args()).run()


if __name__ == "__main__":
    main()
