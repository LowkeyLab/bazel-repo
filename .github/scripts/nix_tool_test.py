"""Public cache configuration behavior for credential-free CI readers."""

import json
import os
import subprocess
from collections.abc import Iterable
from pathlib import Path
from typing import NotRequired, Protocol, TextIO, TypedDict, cast

import _nix_provenance
import nix_tool
import pytest


@pytest.mark.parametrize("empty", [None, ""])
def test_fork_reader_uses_verified_public_cache_without_variables(
    monkeypatch: pytest.MonkeyPatch, empty: str | None
) -> None:
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


def test_explicit_complete_public_cache_override_is_preserved(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
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
def test_partial_or_invalid_public_override_still_fails(
    monkeypatch: pytest.MonkeyPatch, name: str, key: str
) -> None:
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


type Journal = list[nix_tool.CommandRecord]


class AcceptanceResult(TypedDict):
    tool: str
    status: str
    output: NotRequired[str]
    error: NotRequired[str]
    paths: NotRequired[list[dict[str, str]]]


class AcceptanceOptions(TypedDict, total=False):
    signatures: bool
    output: str
    initial: str


class Acceptance(Protocol):
    def __call__(
        self,
        tool: str = "aspect",
        log: str | None = None,
        closure: list[str] | None = None,
        initial: str = "",
        output: str = ROOT,
        signatures: bool = True,
        fail: str | None = None,
    ) -> tuple[int, AcceptanceResult, Journal, Path]:
        pass


def activity(path: str = ROOT, source: str = CACHE, **changes: object) -> str:
    event: dict[str, object] = {
        "action": "start",
        "type": 108,
        "fields": [path, source],
    }
    event.update(changes)
    return "@nix " + json.dumps(event) + "\n"


@pytest.fixture
def acceptance(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Acceptance:
    """Supply process observations while retaining policy and artifact writing."""
    for key in (
        "CACHIX_AUTH_TOKEN",
        "CACHIX_SIGNING_KEY",
        "CACHIX_CACHE_NAME",
        "CACHIX_PUBLIC_KEY",
    ):
        monkeypatch.delenv(key, raising=False)
    monkeypatch.setenv("RUNNER_TEMP", str(tmp_path))
    monkeypatch.setenv(
        "NIX_CONFIG",
        "substituters = https://inherited.invalid\n"
        "builders = ssh://inherited.invalid\n"
        "max-jobs = 8\nfallback = true\nrequire-sigs = false\n"
        "access-tokens = inherited.invalid=synthetic-token\n"
        "netrc-file = /synthetic/netrc\nsecret-key-files = /synthetic/signing-key\n",
    )
    # A synthetic path should be absent independent of the host's Nix store.
    monkeypatch.setattr(nix_tool.os.path, "lexists", lambda path: False)

    def execute(
        tool: str = "aspect",
        log: str | None = None,
        closure: list[str] | None = None,
        initial: str = "",
        output: str = ROOT,
        signatures: bool = True,
        fail: str | None = None,
    ) -> tuple[int, AcceptanceResult, Journal, Path]:
        def process(
            args: Iterable[str],
            *,
            stdout: TextIO,
            stderr: TextIO,
            text: bool,
            check: bool,
        ) -> subprocess.CompletedProcess[str]:
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
                    json.dumps(
                        {
                            path: {"narHash": "sha256-synthetic", "references": []}
                            for path in ([ROOT] if closure is None else closure)
                        }
                    ),
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
            # Commands are an observable acceptance contract, not private
            # helper calls. Reject stubs that would hide changed targets/options.
            expected = {
                "version": ["nix", "--version"],
                "effective-config": ["nix", "config", "show", "--json"],
                "evaluate": [
                    "nix",
                    "eval",
                    "--raw",
                    "--no-update-lock-file",
                    f".#packages.x86_64-linux.{tool}.outPath",
                ],
                "initial-store": ["nix", "path-info", "--all"],
                "realise": [
                    "nix-store",
                    "--realise",
                    ROOT,
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
                ],
                "valid-output": ["nix-store", "--check-validity", ROOT],
                "closure": ["nix", "path-info", "--recursive", "--json", ROOT],
                "remote-availability": [
                    nix_tool.sys.executable,
                    nix_tool.__file__,
                    "verify",
                    tool,
                    ROOT,
                ],
            }
            for executable in ("aspect", "lcov", "genhtml", "geninfo"):
                expected["smoke-" + executable] = [
                    ROOT + "/bin/" + executable,
                    "--version",
                ]
            assert command == expected[label]
            stdout.write(value)
            if label == fail:
                stderr.write("injected command failure\n")
            return subprocess.CompletedProcess[str](command, 1 if label == fail else 0)

        monkeypatch.setattr(nix_tool.subprocess, "run", process)
        status = nix_tool.accept(tool)
        directory = tmp_path / f"nix-acceptance-{tool}"
        # The fixture reads artifacts authored by accept(); assertions below verify their schema and values.
        result = cast(
            AcceptanceResult, json.loads((directory / "result.json").read_text())
        )
        journal = directory / "commands.json"
        commands = (
            cast(Journal, json.loads(journal.read_text())) if journal.exists() else []
        )
        return status, result, commands, directory

    return execute


def labels(commands: Journal) -> list[str]:
    return [command["label"] for command in commands]


@pytest.mark.parametrize("tool,source", [("aspect", CACHE), ("lcov", UPSTREAM)])
def test_strict_acceptance_retains_ordered_closure_and_artifacts(
    acceptance: Acceptance, tool: str, source: str
) -> None:
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
    # Nix uses the last assignment of a setting. Check full keys and values,
    # including overrides of inherited builders and credential providers.
    settings = {
        key.strip(): value.strip()
        for line in os.environ["NIX_CONFIG"].splitlines()
        if "=" in line
        for key, value in [line.split("=", 1)]
    }
    assert settings["require-sigs"] == "true"
    assert settings["substituters"] == f"{CACHE} {UPSTREAM}"
    assert settings["fallback"] == "false"
    assert settings["max-jobs"] == "0"
    assert settings["builders"] == ""
    assert settings["access-tokens"] == ""
    assert settings["netrc-file"] == "/dev/null"
    assert settings["secret-key-files"] == ""


def test_preexisting_dependency_keeps_classification_despite_ambiguous_sources(
    acceptance: Acceptance,
) -> None:
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
def test_invalid_log_or_root_stops_before_closure(
    acceptance: Acceptance, log: str, diagnostic: str
) -> None:
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
def test_new_closure_path_requires_unambiguous_provenance(
    acceptance: Acceptance, dependency_log: str
) -> None:
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
    acceptance: Acceptance, monkeypatch: pytest.MonkeyPatch, credential: str
) -> None:
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
    acceptance: Acceptance, options: AcceptanceOptions, last: str, diagnostic: str
) -> None:
    status, result, commands, _ = acceptance(**options)
    assert status == 1
    assert diagnostic in result["error"]
    assert "paths" not in result
    assert labels(commands)[-1] == last
    assert "realise" not in labels(commands)


def test_existing_filesystem_output_prevents_realisation(
    acceptance: Acceptance, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(nix_tool.os.path, "lexists", lambda path: path == ROOT)
    status, result, commands, _ = acceptance()
    assert status == 1
    assert "Output already present" in result["error"]
    assert labels(commands)[-1] == "initial-store"


@pytest.mark.parametrize(
    "failure", ["realise", "valid-output", "remote-availability", "smoke-aspect"]
)
def test_failed_commands_retain_evidence_without_publishing_paths(
    acceptance: Acceptance, failure: str
) -> None:
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


@pytest.mark.parametrize("record", [None, [], "activity", 42, True])
def test_non_object_activity_fails_normally_before_closure(
    acceptance: Acceptance, record: object
) -> None:
    log = "@nix " + json.dumps(record) + "\n"
    status, result, commands, directory = acceptance(log=log)
    assert status == 1
    assert result["status"] == "failed"
    assert "activity record must be an object" in result["error"]
    assert "paths" not in result
    assert labels(commands)[-1] == "valid-output"
    assert (directory / "realise.stderr").read_text() == log


@pytest.mark.parametrize(
    "tool,source", [("aspect", CACHE), ("lcov", UPSTREAM), ("lcov", CACHE)]
)
def test_policy_classifies_ordered_mixed_closure_from_validated_evidence(
    tool: str, source: str
) -> None:
    log = (
        "ordinary diagnostic\n"
        + activity(action="stop", type=105)
        + activity(type=999, fields=None)
        + activity(source=source)
        + activity(source=source)
        + activity(DEPENDENCY, UPSTREAM)
        + activity(UNRELATED)
    )
    downloads = _nix_provenance.ValidatedDownloads(tool, ROOT, CACHE, log)
    assert downloads.classify(set(), {DEPENDENCY: {}, ROOT: {}}) == [
        {"path": DEPENDENCY, "source": UPSTREAM, "status": "downloaded"},
        {"path": ROOT, "source": source, "status": "downloaded"},
    ]


def test_policy_preexisting_path_takes_precedence_over_missing_or_ambiguous_sources() -> (
    None
):
    downloads = _nix_provenance.ValidatedDownloads(
        "aspect",
        ROOT,
        CACHE,
        activity() + activity(DEPENDENCY) + activity(DEPENDENCY, UPSTREAM),
    )
    assert downloads.classify(
        {DEPENDENCY, UNRELATED}, [DEPENDENCY, ROOT, UNRELATED]
    ) == [
        {"path": DEPENDENCY, "status": "present-before-realisation"},
        {"path": ROOT, "source": CACHE, "status": "downloaded"},
        {"path": UNRELATED, "status": "present-before-realisation"},
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
def test_policy_rejects_invalid_activity_or_root(log: str, diagnostic: str) -> None:
    with pytest.raises(ValueError, match=diagnostic):
        _nix_provenance.ValidatedDownloads("aspect", ROOT, CACHE, log)


@pytest.mark.parametrize("record", [None, [], "activity", 42, True])
def test_policy_rejects_non_object_activity(record: object) -> None:
    with pytest.raises(ValueError, match="activity record must be an object"):
        _nix_provenance.ValidatedDownloads(
            "aspect", ROOT, CACHE, "@nix " + json.dumps(record) + "\n"
        )


@pytest.mark.parametrize(
    "log", ["", activity(DEPENDENCY) + activity(DEPENDENCY, UPSTREAM)]
)
def test_policy_requires_one_source_for_each_new_closure_path(log: str) -> None:
    downloads = _nix_provenance.ValidatedDownloads(
        "aspect", ROOT, CACHE, activity() + log
    )
    with pytest.raises(
        ValueError, match="Closure download provenance absent or ambiguous"
    ):
        downloads.classify(set(), [ROOT, DEPENDENCY])


def test_policy_uses_requested_output_and_cache_instead_of_default_constants() -> None:
    cache = "https://other.cachix.org"
    downloads = _nix_provenance.ValidatedDownloads(
        "aspect", UNRELATED, cache, activity(UNRELATED, cache)
    )
    assert downloads.classify(set(), [UNRELATED]) == [
        {"path": UNRELATED, "source": cache, "status": "downloaded"},
    ]


@pytest.mark.parametrize(
    "mode",
    [
        "available",
        "hash-mismatch",
        "references-mismatch",
        "missing",
        "aspect-upstream-only",
    ],
)
def test_remote_closure_verification(tmp_path: Path, mode: str) -> None:
    """A slow remote query must not prevent another query from starting."""
    tools = tmp_path / "bin"
    tools.mkdir()
    closure = {
        ROOT: {"narHash": "sha256-root", "references": [DEPENDENCY]},
        DEPENDENCY: {"narHash": "sha256-dependency", "references": []},
    }
    (tmp_path / "closure.json").write_text(json.dumps(closure))
    (tools / "nix.py").write_text(
        "import json, os, pathlib, sys, time\n"
        "root = pathlib.Path(os.environ['RUNNER_TEMP'])\n"
        "closure = json.loads((root / 'closure.json').read_text())\n"
        "if '--recursive' in sys.argv:\n"
        "    print(json.dumps(closure))\n"
        "    sys.exit(0)\n"
        "path = sys.argv[-1]\n"
        "assert path in closure\n"
        "source = sys.argv[sys.argv.index('--store') + 1]\n"
        "with (root / 'queries.jsonl').open('a') as journal:\n"
        "    journal.write(json.dumps({'path': path, 'source': source}) + '\\n')\n"
        "(root / (pathlib.Path(path).name + '.started')).touch()\n"
        "deadline = time.monotonic() + 5\n"
        "while not all((root / (pathlib.Path(p).name + '.started')).exists() for p in closure):\n"
        "    if time.monotonic() >= deadline:\n"
        "        sys.exit(1)\n"
        "    time.sleep(0.01)\n"
        "mode = os.environ['NIX_TEST_MODE']\n"
        "if path == list(closure)[1]:\n"
        "    if mode == 'missing': sys.exit(1)\n"
        "    if mode == 'hash-mismatch': closure[path]['narHash'] = 'sha256-other'\n"
        "    if mode == 'references-mismatch': closure[path]['references'] = list(closure)\n"
        "if mode == 'aspect-upstream-only' and path == list(closure)[0] and 'cachix' in source:\n"
        "    sys.exit(1)\n"
        "print(json.dumps({path: closure[path]}))\n"
    )
    nix = tools / "nix"
    nix.write_text('#!/bin/sh\nexec "$NIX_TEST_PYTHON" "${0}.py" "$@"\n')
    nix.chmod(0o700)
    curl = tools / "curl"
    curl.write_text("#!/bin/sh\nprintf 404\n")
    curl.chmod(0o700)
    environment = {
        **os.environ,
        "PATH": str(tools) + os.pathsep + os.environ["PATH"],
        "RUNNER_TEMP": str(tmp_path),
        "NIX_TEST_PYTHON": nix_tool.sys.executable,
        "NIX_TEST_MODE": mode,
        "CACHIX_CACHE_NAME": "",
        "CACHIX_PUBLIC_KEY": "",
    }

    tool = "aspect" if mode == "aspect-upstream-only" else "lcov"
    result = subprocess.run(
        [nix_tool.sys.executable, nix_tool.__file__, "verify", tool, ROOT],
        env=environment,
        capture_output=True,
        text=True,
        timeout=20,
        check=False,
    )

    manifest_path = tmp_path / f"nix-{tool}-closure.json"
    assert result.returncode == (0 if mode == "available" else 1), (
        result.stdout + result.stderr
    )
    if mode == "aspect-upstream-only":
        queries = [
            json.loads(line)
            for line in (tmp_path / "queries.jsonl").read_text().splitlines()
        ]
        assert [entry["source"] for entry in queries if entry["path"] == ROOT] == [
            CACHE
        ]
    if mode != "available":
        assert not manifest_path.exists()
        return
    manifest = json.loads(manifest_path.read_text())
    assert [entry["path"] for entry in manifest["paths"]] == [ROOT, DEPENDENCY]
    assert [entry["source"] for entry in manifest["paths"]] == [CACHE, CACHE]
    assert manifest["paths"][0]["references"] == [DEPENDENCY]
