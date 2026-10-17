"""Public cache configuration behavior for credential-free CI readers."""

import json
import os
import subprocess
from pathlib import Path

import nix_tool
import pytest


@pytest.mark.parametrize("empty", [None, ""])
def test_fork_reader_uses_verified_public_cache_without_variables(monkeypatch, empty):
    for name in ("CACHIX_CACHE_NAME", "CACHIX_PUBLIC_KEY"):
        if empty is None:
            monkeypatch.delenv(name, raising=False)
        else:
            monkeypatch.setenv(name, empty)
    monkeypatch.setenv("NIX_CONFIG", "")

    assert nix_tool.configure() == "https://lowkey-lab.cachix.org"
    assert (
        "extra-substituters = https://lowkey-lab.cachix.org" in os.environ["NIX_CONFIG"]
    )
    assert (
        "extra-trusted-public-keys = lowkey-lab.cachix.org-1:7YzknrrOt9NRhJp4VIYCqa9Edc/401MuFrtqyPUW3i8="
        in os.environ["NIX_CONFIG"]
    )
    assert "require-sigs = true" in os.environ["NIX_CONFIG"]


def test_explicit_complete_public_cache_override_is_preserved(monkeypatch):
    monkeypatch.setenv("CACHIX_CACHE_NAME", "override-cache")
    monkeypatch.setenv(
        "CACHIX_PUBLIC_KEY",
        "override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    monkeypatch.setenv("NIX_CONFIG", "connect-timeout = 9")

    assert nix_tool.configure() == "https://override-cache.cachix.org"
    assert (
        "extra-substituters = https://override-cache.cachix.org"
        in os.environ["NIX_CONFIG"]
    )
    assert (
        "extra-trusted-public-keys = override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        in os.environ["NIX_CONFIG"]
    )
    assert "https://lowkey-lab.cachix.org" not in os.environ["NIX_CONFIG"]
    assert os.environ["NIX_CONFIG"].startswith("connect-timeout = 9")


@pytest.mark.parametrize(
    ("name", "key"),
    [
        ("override-cache", ""),
        (
            "",
            "override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ),
        ("override-cache", "invalid-key"),
    ],
)
def test_partial_or_invalid_public_override_still_fails(monkeypatch, name, key):
    monkeypatch.setenv("CACHIX_CACHE_NAME", name)
    monkeypatch.setenv("CACHIX_PUBLIC_KEY", key)
    monkeypatch.setenv("NIX_CONFIG", "connect-timeout = 9")

    with pytest.raises(ValueError, match="Set both CACHIX_CACHE_NAME"):
        nix_tool.configure()
    assert os.environ["NIX_CONFIG"] == "connect-timeout = 9"


# Offline regression coverage: command outputs cannot establish real downloads,
# signature verification, remote availability, or executable success. The hosted
# fresh-runner acceptance workflow remains responsible for those claims.
CACHE = "https://lowkey-lab.cachix.org"
UPSTREAM = "https://cache.nixos.org"
ROOT = "/nix/store/" + "a" * 32 + "-test-tool"
DEPENDENCY = "/nix/store/" + "b" * 32 + "-dependency"
UNRELATED = "/nix/store/" + "c" * 32 + "-unrelated"


def activity(path=ROOT, source=CACHE, **changes):
    event = {"action": "start", "type": 108, "fields": [path, source]}
    event.update(changes)
    return "@nix " + json.dumps(event) + "\n"


@pytest.fixture
def acceptance(monkeypatch, tmp_path):
    """Supply process observations while retaining policy and artifact writing."""
    for key in (
        "CACHIX_AUTH_TOKEN",
        "CACHIX_SIGNING_KEY",
        "CACHIX_CACHE_NAME",
        "CACHIX_PUBLIC_KEY",
    ):
        monkeypatch.delenv(key, raising=False)
    monkeypatch.setenv("RUNNER_TEMP", str(tmp_path))
    monkeypatch.setenv("NIX_CONFIG", "")
    # A synthetic path should be absent independent of the host's Nix store.
    monkeypatch.setattr(nix_tool.os.path, "lexists", lambda path: False)

    def execute(
        tool="aspect",
        log=None,
        closure=None,
        initial="",
        output=ROOT,
        signatures=True,
        fail=None,
    ):
        def process(args, *, stdout, stderr, text, check):
            command = list(args)
            if command[:2] == ["nix", "--version"]:
                label, value = "version", "nix (Nix) test\n"
            elif command[:3] == ["nix", "config", "show"]:
                label, value = (
                    "effective-config",
                    json.dumps({"require-sigs": {"value": signatures}}),
                )
            elif command[:2] == ["nix", "eval"]:
                label, value = "evaluate", output
            elif command[:3] == ["nix", "path-info", "--all"]:
                label, value = "initial-store", initial
            elif command[:2] == ["nix-store", "--realise"]:
                label, value = "realise", ROOT + "\n"
                stderr.write(activity() if log is None else log)
            elif command[:2] == ["nix-store", "--check-validity"]:
                label, value = "valid-output", ""
            elif command[:3] == ["nix", "path-info", "--recursive"]:
                label, value = (
                    "closure",
                    json.dumps([ROOT] if closure is None else closure),
                )
            elif len(command) > 2 and command[2] == "verify":
                label, value = "remote-availability", "remote metadata verified\n"
                # This is a captured child-process result, not a replacement for
                # the deterministic provenance policy exercised by accept().
                (tmp_path / f"nix-{tool}-closure.json").write_text(
                    '{"output": "captured remote manifest"}\n'
                )
            elif command[0].startswith(ROOT + "/bin/"):
                label, value = "smoke-" + Path(command[0]).name, "tool version\n"
            else:
                raise AssertionError(f"Unexpected command: {command}")
            stdout.write(value)
            if label == fail:
                stderr.write("injected command failure\n")
            return subprocess.CompletedProcess(command, 1 if label == fail else 0)

        monkeypatch.setattr(nix_tool.subprocess, "run", process)
        status = nix_tool.accept(tool)
        directory = tmp_path / f"nix-acceptance-{tool}"
        result = json.loads((directory / "result.json").read_text())
        journal = directory / "commands.json"
        commands = json.loads(journal.read_text()) if journal.exists() else []
        return status, result, commands, directory

    return execute


def labels(commands):
    return [command["label"] for command in commands]


@pytest.mark.parametrize("tool,source", [("aspect", CACHE), ("lcov", UPSTREAM)])
def test_strict_acceptance_retains_ordered_closure_and_artifacts(
    acceptance, tool, source
):
    log = (
        "ordinary diagnostic\n"
        + activity(action="stop", type=105)
        + activity(type=999)
        + activity(source=source)
        + activity(source=source)
        + activity(DEPENDENCY, UPSTREAM)
        + activity(UNRELATED)
    )
    status, result, commands, directory = acceptance(
        tool=tool, log=log, closure=[DEPENDENCY, ROOT]
    )
    assert status == 0
    assert result == {
        "tool": tool,
        "status": "passed",
        "output": ROOT,
        "paths": [
            {"path": DEPENDENCY, "source": UPSTREAM, "status": "downloaded"},
            {"path": ROOT, "source": source, "status": "downloaded"},
        ],
    }
    smoke = (
        ["smoke-aspect"]
        if tool == "aspect"
        else ["smoke-lcov", "smoke-genhtml", "smoke-geninfo"]
    )
    assert labels(commands) == [
        "version",
        "effective-config",
        "evaluate",
        "initial-store",
        "realise",
        "valid-output",
        "closure",
        "remote-availability",
        *smoke,
    ]
    assert all(command["exit"] == 0 for command in commands)
    for command in commands:
        assert (directory / (command["label"] + ".stdout")).exists()
        assert (directory / (command["label"] + ".stderr")).exists()
    assert (directory / "realise.stderr").read_text() == log
    assert (directory / "evaluate.stdout").read_text() == ROOT
    assert json.loads((directory / "remote-availability.json").read_text()) == {
        "output": "captured remote manifest"
    }
    assert "require-sigs = true" in os.environ["NIX_CONFIG"]
    assert f"substituters = {CACHE} {UPSTREAM}" in os.environ["NIX_CONFIG"]
    assert "fallback = false" in os.environ["NIX_CONFIG"]
    assert "max-jobs = 0" in os.environ["NIX_CONFIG"]


def test_preexisting_dependency_keeps_classification_despite_ambiguous_sources(
    acceptance,
):
    status, result, _, _ = acceptance(
        initial=DEPENDENCY + "\n",
        log=activity() + activity(DEPENDENCY) + activity(DEPENDENCY, UPSTREAM),
        closure=[DEPENDENCY, ROOT],
    )
    assert status == 0
    assert result["paths"] == [
        {"path": DEPENDENCY, "status": "present-before-realisation"},
        {"path": ROOT, "source": CACHE, "status": "downloaded"},
    ]


@pytest.mark.parametrize(
    "log,diagnostic",
    [
        (activity(type=105), "build activity"),
        ("@nix {broken\n", "Expecting property name"),
        (activity(fields=None), "substitution log schema"),
        (activity(fields=[ROOT]), "substitution log schema"),
        (activity(fields=[ROOT, CACHE, "extra"]), "substitution log schema"),
        (activity(fields=[17, CACHE]), "substitution path or source"),
        (activity(fields=[ROOT, 17]), "substitution path or source"),
        (activity(source="https://untrusted.invalid"), "substitution path or source"),
        (
            activity() + activity(UNRELATED, "https://untrusted.invalid"),
            "substitution path or source",
        ),
        (activity() + activity(UNRELATED, fields=[]), "substitution log schema"),
        ("ordinary diagnostic\n", "absent or ambiguous"),
        (activity() + activity(source=UPSTREAM), "absent or ambiguous"),
        (activity(source=UPSTREAM), "exclusively from intended Cachix"),
    ],
)
def test_invalid_log_or_root_stops_before_closure(acceptance, log, diagnostic):
    status, result, commands, directory = acceptance(log=log)
    assert status == 1
    assert result["status"] == "failed"
    assert diagnostic in result["error"]
    assert "paths" not in result
    assert labels(commands) == [
        "version",
        "effective-config",
        "evaluate",
        "initial-store",
        "realise",
        "valid-output",
    ]
    assert (directory / "realise.stderr").read_text() == log


@pytest.mark.parametrize(
    "dependency_log", ["", activity(DEPENDENCY) + activity(DEPENDENCY, UPSTREAM)]
)
def test_new_closure_path_requires_unambiguous_provenance(acceptance, dependency_log):
    status, result, commands, _ = acceptance(
        log=activity() + dependency_log, closure=[ROOT, DEPENDENCY]
    )
    assert status == 1
    assert "Closure download provenance absent or ambiguous" in result["error"]
    assert "paths" not in result
    assert labels(commands)[-1] == "closure"
    assert "remote-availability" not in labels(commands)


@pytest.mark.parametrize("credential", ["CACHIX_AUTH_TOKEN", "CACHIX_SIGNING_KEY"])
def test_strict_reader_rejects_credentials_before_commands(
    acceptance, monkeypatch, credential
):
    monkeypatch.setenv(credential, "synthetic-credential")
    status, result, commands, _ = acceptance()
    assert status == 1
    assert "must not receive Cachix credentials" in result["error"]
    assert "paths" not in result
    assert commands == []


@pytest.mark.parametrize(
    "options,last,diagnostic",
    [
        ({"signatures": False}, "effective-config", "Signature verification"),
        ({"output": ROOT + "\n"}, "evaluate", "exact raw non-derivation"),
        ({"output": ROOT + ".drv"}, "evaluate", "exact raw non-derivation"),
        ({"output": "/tmp/output"}, "evaluate", "exact raw non-derivation"),
        ({"output": ROOT + " " + DEPENDENCY}, "evaluate", "exact raw non-derivation"),
        ({"initial": ROOT + "\n"}, "initial-store", "Output already present"),
    ],
)
def test_acceptance_preconditions_stop_before_realisation(
    acceptance, options, last, diagnostic
):
    status, result, commands, _ = acceptance(**options)
    assert status == 1
    assert diagnostic in result["error"]
    assert "paths" not in result
    assert labels(commands)[-1] == last
    assert "realise" not in labels(commands)


def test_existing_filesystem_output_prevents_realisation(acceptance, monkeypatch):
    monkeypatch.setattr(nix_tool.os.path, "lexists", lambda path: path == ROOT)
    status, result, commands, _ = acceptance()
    assert status == 1
    assert "Output already present" in result["error"]
    assert labels(commands)[-1] == "initial-store"


@pytest.mark.parametrize(
    "failure", ["realise", "valid-output", "remote-availability", "smoke-aspect"]
)
def test_failed_commands_retain_evidence_without_publishing_paths(acceptance, failure):
    status, result, commands, directory = acceptance(fail=failure)
    assert status == 1
    assert result["status"] == "failed"
    assert failure + " failed (1)" in result["error"]
    assert "paths" not in result
    assert labels(commands)[-1] == failure
    assert commands[-1]["exit"] == 1
    assert "injected command failure" in (directory / (failure + ".stderr")).read_text()
    if failure in ("realise", "valid-output"):
        assert "closure" not in labels(commands)
    if failure == "remote-availability":
        assert "smoke-aspect" not in labels(commands)


# Follow-up #2328 must reject non-object JSON activity records through the
# normal failed-result path. Do not freeze the current uncaught AttributeError
# into this compatibility baseline or require its approved correction here.
