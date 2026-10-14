# Automatic Rust toolchain updates

Renovate maintains the stable Rust compiler and its matching formatter in one
pull request. Stable patch and minor releases are eligible immediately, with no
release-age delay. They merge automatically after the existing required CI checks
pass. Major releases require dependency-dashboard approval and manual merging.
The existing CI workflows and merge requirements remain in place.

## Coupled version references

The stable compiler pin in `MODULE.bazel` is the compiler authority. Two custom
regular expression managers track that pin and the formatter repository name in
`MODULE.bazel` and `tools/format/BUILD.bazel`. Both managers use the same `rust`
dependency identity and the `rust-version` datasource. Numeric stable releases
are grouped as **Rust toolchain**; beta and nightly releases are excluded.
The dated nightly toolchain remains pinned.

The formatter manager converts underscore-separated versions into semantic
versions for lookup and converts the selected release back into a repository
name. Renovate updates all three references before running Bazel. Updating only
the compiler would leave an unavailable formatter export and prevent Bazel from
regenerating the lockfile.

[Renovate custom managers](https://docs.renovatebot.com/modules/manager/regex/)
and [replacement templates](https://docs.renovatebot.com/templates/) support this
coupling. The [Rust datasource](https://docs.renovatebot.com/modules/datasource/rust-version/)
reads official Rust distribution manifests.

## Generated changes

Branch-level post-upgrade commands regenerate `MODULE.bazel.lock`, apply the
selected rustfmt to Rust sources, run Gazelle immediately after source formatting,
and format Starlark files. The PR retains the resulting Rust and BUILD file
changes along with the toolchain references and lockfile. All commands run through
Bazel and must match the existing Renovate service command allowlist.

Rust upgrades can introduce new Clippy findings, fail tests, or change the LLVM
raw coverage-profile format. Such failures block merging for human repair. The
LLVM coverage-tools pin changes only through a reviewed compatibility fix;
this automation does not upgrade LLVM or relax coverage checks.

## Acceptance

Validate the configuration and perform dependency extraction and release lookup
with the deployed Renovate version. Verify that patch and minor candidates update
all three references together, preserve the nightly pin, and exclude prereleases.
Verify that major candidates require approval and have automerge disabled.

Exercise the post-upgrade commands and confirm that generated files and rustfmt
changes are retained. A second generation and formatting pass must produce no
changes. Run repository formatting, build, tests, lint, and coverage through the
existing Bazel workflows. Distinguish a local configuration or extraction check
from a successful run by the deployed service.

The design and implementation scope were approved on 2026-10-14.

## Verification on 2026-10-14

Strict configuration validation and a local release-lookup dry run used Renovate
44.155.9 from the image recorded in the existing service's
[successful workflow run](https://github.com/LowkeyLab/renovatebot/actions/runs/38513492679).
The dry run detected all three 1.96.0 references and proposed 1.99.0 for each.
Runtime replacement checks verified coupled edits, preserved nightly, excluded
prereleases, and resolved the inherited merge policy for patch, minor, and major
candidates. Patch and major candidates were simulated.

A real 1.99.0 validation scenario exercised lockfile regeneration, rustfmt,
Gazelle, buildifier, and strict lockfile checking. Validation files were restored
from a saved snapshot afterward. Repository build, all 145 test targets, lint,
and CI-configured coverage passed for the current 1.96.0 toolchain. The test and
coverage results used cached executions. Coverage HTML generation emitted
existing category warnings.
