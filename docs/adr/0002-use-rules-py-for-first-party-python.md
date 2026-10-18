---
status: accepted
---

# Use `rules_py` for first-party Python

First-party Python libraries, executables, and tests use `aspect_rules_py`.
The existing Gazelle binary generates their build targets. Interpreter
provisioning and package installation use hermetic tools from `rules_py`.
Keep `rules_python` and its bootstrap configuration for external Bazel tooling.

## Packages and tests

Shared dependencies live in the root `pyproject.toml` default group, with
`uv.lock` and `gazelle_python.yaml`. Updates run through Bazel-managed `uv`.
The manifest generator consumes the same dependency hub as the Python targets.
Tests use `pytest` through a shared `py_test` wrapper, which replaces the
generated source entry point and supplies the test framework dependency.

The maintained reference project lives under `python_sample/`, with its own
Python import root and package-level Gazelle generation. It includes a library,
executable, tests, and a third-party package. Acceptance covers existing Linux
x86_64 development and CI execution environments.

[The upstream Gazelle integration guide](https://github.com/aspect-build/rules_py/blob/v1.12.1/README.md#gazelle-integration)
documents mappings for `py_library`, `py_binary`, and `py_test`. Libraries and
executables map to shared wrappers that call `rules_py`. These wrappers add
production coverage baselines in coverage mode. Tests use the shared `pytest` wrapper.
Disable generated `pyi_deps` and `pyi_srcs` attributes for the inspected release,
which lacks those attributes. [The Gazelle directive reference](https://rules-python.readthedocs.io/en/latest/gazelle/docs/directives.html)
describes their generation controls.

## Automatic interpreter updates

Start on Python 3.14 with automatic patch and minor updates to stable Python 3
releases. Track the selected minor and a standalone interpreter release date.
The release date determines the downloaded patch and its checksum. A patch
string alone doesn't pin the artifact.

Use Renovate custom regular expression managers for the `python-version` and
`github-releases` release sources, with `astral-sh/python-build-standalone` for
interpreter artifacts. Filter preview releases and Python major upgrades before
reducing release versions to their major and minor components.

Use `semver-coerced` for the selector. The inspected Python versioning
implementation doesn't replace a selector when its proposed version lacks a
patch component. Treat interpreter date changes as patch updates, including
across year boundaries. [The version extraction documentation](https://docs.renovatebot.com/configuration-options/#extractversion)
describes reducing release versions to the required precision.

Group available selector and artifact updates. The post-upgrade commands
refresh `MODULE.bazel.lock`, `uv.lock`, the import manifest, and generated
sample targets through Bazel. Verify extraction and command execution with the
deployed Renovate version. The existing service permits the required commands.

Allow automatic merging only after the maintained example and required CI
checks pass. Grouping doesn't guarantee artifact availability. An unavailable
interpreter or incompatible dependency must fail validation and prevent merging.
Package metadata permits future stable Python minor upgrades.

## Validation

Verify the generated library, executable, and tests, including local and external
imports. The runtime test compares the running interpreter with the configured
minor in `MODULE.bazel` and checks isolated execution. It must continue working
when the selected minor changes.

Compare the checked-in import manifest with generated output. A second Gazelle
run must produce no changes. Verify that interpreter updates change the selector
when applicable, the release date, and the locked artifact metadata together.
Reject preview releases and unsupported selector/artifact combinations.

Run full repository formatting, build, and lint checks before completing the
migration. The user confirmed the design and authorized implementation on
2026-10-13.
