"""Private adapter regressions; build substitutes never produce benchmark evidence."""

import argparse
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from worktree_pool.benchmark.run import EDIT_FILE, Experiment


class FirstPairComplete(Exception):
    pass


class PrivateExperiment(Experiment):
    """Keep Git/catalog/orchestration real; omit host build/resource effects."""

    def native(self, cwd, base, executable, *args):
        return ""

    def footprint(self, path, base):
        return {}

    def resources(self, label):
        return None

    def cleanup_disposable(self, path, base):
        return None

    def append(self, name, value):
        super().append(name, value)
        if name == "pairs.jsonl":
            raise FirstPairComplete


class ChangedAfterSetup(PrivateExperiment):
    def append(self, name, value):
        super().append(name, value)
        if name == "setup_cycles.jsonl" and value.get("kind") == "unchanged_base":
            identity = value["cycle"]["callers"][0]["selected_worktree_id"]
            path, _ = self.slots[identity]
            source = path / EDIT_FILE
            source.write_text(source.read_text().replace("8080", "8083"))
            self.git(path, "add", EDIT_FILE)
            self.git(path, "commit", "-m", "test: change retained tip after setup")


class AdapterTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = Path("worktree_pool/worktree_pool").resolve()

    def experiment(self, experiment_type=Experiment, fixture_commit="unused"):
        return experiment_type(
            argparse.Namespace(
                root=self.root / "run",
                source=self.root,
                pool_binary=self.binary,
                phase="warmup",
                fixture_commit=fixture_commit,
                tool_commit="private-adapter-regression",
                quiet_window="test fixture only; no measured native builds",
                minimum_free_gib=0,
                minimum_available_ram_gib=0,
            )
        )

    def test_catalog_stays_private_despite_ambient_pool_authority(self):
        outside = self.root / "outside"
        outside.mkdir()
        sentinel = outside / "sentinel"
        sentinel.write_bytes(b"unrelated authority must remain untouched\n")
        with patch.dict(
            os.environ, {"WORKTREE_POOL_CATALOG_DIR": str(outside / "catalog")}
        ):
            experiment = self.experiment()
        experiment.pool("catalog", "init")
        self.assertTrue((experiment.root / "data/worktree-pool/catalog.redb").exists())
        self.assertEqual(list(outside.iterdir()), [sentinel])
        self.assertEqual(
            sentinel.read_bytes(), b"unrelated authority must remain untouched\n"
        )

    def test_both_git_adapters_ignore_global_checkout_policy(self):
        home = self.root / "home"
        home.mkdir()
        (home / ".gitconfig").write_text("[core]\n\tautocrlf = true\n")
        with patch.dict(os.environ, {"HOME": str(home)}):
            experiment = self.experiment()
        argv = ["git", "config", "--default", "false", "--get", "core.autocrlf"]
        self.assertEqual(experiment.command(argv, env=experiment.git_env), "false")
        self.assertEqual(experiment.command(argv, env=experiment.pool_env), "false")

    def repository_experiment(self, experiment_type=PrivateExperiment):
        # Neutral ambient controls isolate this revision regression from the
        # independent environment regressions above.
        env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("GIT_", "WORKTREE_POOL_"))
        }
        env.update(
            GIT_CONFIG_GLOBAL="/dev/null",
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="Test",
            GIT_AUTHOR_EMAIL="test@example.invalid",
            GIT_COMMITTER_NAME="Test",
            GIT_COMMITTER_EMAIL="test@example.invalid",
        )
        source = self.root / "source"
        source.mkdir()

        def git(*args):
            return subprocess.run(
                ["git", *map(str, args)],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()

        git("init", "--initial-branch=main", source)
        (source / ".gitignore").write_text("user.bazelrc\n")
        edit = source / EDIT_FILE
        edit.parent.mkdir(parents=True)
        edit.write_text("fn main() { let port = 8080; }\n")
        git("-C", source, "add", ".")
        git("-C", source, "commit", "-m", "test: seed private adapter fixture")
        base = git("-C", source, "rev-parse", "HEAD")
        with patch.dict(os.environ, env, clear=True):
            experiment = self.experiment(experiment_type, base)
        experiment.command(
            ["git", "clone", "--bare", source, experiment.origin],
            env=experiment.git_env,
        )
        for context in (experiment.pool_context, experiment.disposable_context):
            experiment.command(
                ["git", "clone", experiment.origin, context], env=experiment.git_env
            )
        experiment.pool("catalog", "init")
        registered = experiment.pool("repo", "register", str(experiment.pool_context))
        experiment.repo_id = registered["context"]["repository_id"]
        return experiment

    def test_first_unchanged_cycle_uses_warmed_base_before_acquisition(self):
        experiment = self.repository_experiment()
        base = experiment.base_commit
        experiment.warmup()
        with self.assertRaises(FirstPairComplete):
            experiment.sample()
        records = [
            json.loads(line)
            for line in (experiment.root / "pairs.jsonl").read_text().splitlines()
        ]
        self.assertEqual(len(records), 1)
        pair = records[0]
        self.assertEqual(pair["workload"], "unchanged")
        self.assertEqual(pair["pooled"]["callers"][0]["pre_acquisition_head"], base)
        self.assertEqual(
            pair["pooled"]["callers"][0]["observed_post_acquisition_head"], base
        )
        setups = [
            json.loads(line)
            for line in (experiment.root / "setup_cycles.jsonl")
            .read_text()
            .splitlines()
        ]
        self.assertEqual(len(setups), 1)
        self.assertEqual(setups[0]["kind"], "unchanged_base")
        self.assertEqual(
            setups[0]["cycle"]["callers"][0]["observed_post_acquisition_head"], base
        )

    def test_changed_selected_tip_cannot_be_accepted_as_unchanged(self):
        experiment = self.repository_experiment(ChangedAfterSetup)
        experiment.warmup()
        rejected = False
        try:
            experiment.sample()
        except RuntimeError as error:
            self.assertIn("unchanged", str(error))
            rejected = True
        except FirstPairComplete:
            pass
        self.assertTrue(rejected, "a revision return was accepted as unchanged")
        self.assertFalse((experiment.root / "pairs.jsonl").exists())

    def test_sampling_refuses_incompatible_warmup_proof_without_overwriting_it(self):
        experiment = self.experiment()
        for proof in ("legacy", "different_runner"):
            with self.subTest(proof=proof):
                previous = dict(experiment.metadata)
                if proof == "legacy":
                    previous.pop("protocol_version")
                    previous.pop("runner_sha256")
                else:
                    previous["runner_sha256"] = "0" * 64
                previous.update(
                    warmup_complete=True,
                    revision_commit="unused",
                    repository_id="unused",
                    maximum_owned_bytes=0,
                    maximum_server_rss=0,
                    condition_warnings=[],
                    resource_snapshots=[{"slots": []}],
                )
                experiment.save("metadata.json", previous)
                before = (experiment.root / "metadata.json").read_bytes()
                args = argparse.Namespace(**vars(experiment.args))
                args.phase = "sample"
                with self.assertRaisesRegex(RuntimeError, "provenance"):
                    Experiment(args)
                self.assertEqual(
                    (experiment.root / "metadata.json").read_bytes(), before
                )


if __name__ == "__main__":
    unittest.main()
