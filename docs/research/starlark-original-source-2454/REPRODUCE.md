# Reproduce the Starlark observations

Use the research branch and its pinned Nix environment.
Use one Bazel invocation at a time.
The native debugger uses port 17354 and bounded waits.
The adapter preserves nonzero child exits.
A successful BEP event does not override the recorded process exit.

## Build and inventory

```bash
umask 077
EVIDENCE=$(mktemp -d)
REPO=$(pwd)
nix develop --command bazel --host_jvm_args=-Xmx1536m build \
  //tools/research_starlark:inventory \
  //tools/research_starlark:probe \
  //tools/research_starlark:convert \
  --config=remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  //tools/research_starlark:inventory \
  --config=remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 -- \
  tools/python/defs.bzl tools/research_starlark/phase.bzl \
  tools/research_starlark/unloaded.bzl tools/research_starlark/BUILD.bazel \
  MODULE.bazel tools/aspect/delivery_flags.axl .aspect/config.axl \
  .aspect/gazelle/shell.axl tools/research_starlark/unloaded.axl \
  > "$EVIDENCE/inventory.json"
```

The historical inventory predates the converter target in the research BUILD file.
Its snapshot remains in `sources/history/BUILD-executed-v2.bazel`.
The final BUILD file adds the converter after all previously measured statements.
Its current inventory therefore includes one additional target declaration.
Use archived inputs when comparing the exact historical LCOV bytes.
The maintained `.bzl`, `.axl`, and MODULE statement offsets remain unchanged.

## Native debugger

The original first run follows a shutdown of this worker's prior server.
The warm run reuses that evaluator state.
The final run explicitly invalidates source files with breakpoints.
Its exact command appears in `bazel-direct-2/command.json`.
The original adapter used `bazel`, then the bounded direct reproduction used `bazelisk`.
Both variants retain child exit 2.

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  //tools/research_starlark:probe \
  --config=remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 -- \
  --inventory "$EVIDENCE/inventory.json" --output "$EVIDENCE/native" --reset
```

The probe creates a build command with these flags:

```text
--experimental_skylark_debug
--experimental_skylark_debug_server_port=17354
--experimental_skylark_debug_reset_analysis
--execution_log_json_file=<run directory>/execution.json
--build_event_json_file=<run directory>/bep.json
--config=remote
--jobs=8
--local_resources=cpu=2
--local_resources=memory=2048
```

The original requested execution JSON files are absent.
The archived console, BEP, debugger frames, and hits preserve the available evidence.
The archive records failed launcher setup separately.
Direct execution initially used the wrong transitioned output path.
The next launcher attempt lacked writable venv storage.
A writable runfiles directory plus the generated manifest resolved that setup failure.
These failures do not establish evaluator capability limits.

## Data conversion and merging

The archived `conversion/bazel-inventory.json` contains five source identities.
The archived selected events come from `bazel-direct-2/hits.json`.
Expand `${REPO}` paths before passing sanitized archived events to the converter.
The converter requires positive entries at specified original locations.
It rejects paths or lines outside the candidate inventory.

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  //tools/research_starlark:convert \
  --config=remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 -- \
  --inventory "$EVIDENCE/bazel-inventory.json" \
  --events "$EVIDENCE/native/hits.json" \
  --root "$REPO" --output "$EVIDENCE/diagnostic.lcov"
```

Missing files, `[]`, and malformed JSON each produce exit 1.
The raw stderr and exit receipts preserve each failure.
The functional output remains `RESEARCH_EXECUTION`.
These checks check data separately from the failed debugger orchestration.
No synthetic positive Aspect profile enters the merger.

Both merge directories contain the same zero baseline and diagnostic profile.
The `a.dat` and `b.dat` filenames exchange their input roles.

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  @bazel_tools//tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator:Main \
  --config=remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 -- \
  --coverage_dir="$EVIDENCE/merge-dat-ab" \
  --output_file="$EVIDENCE/combined-dat-ab.lcov"
```

Repeat the command with `merge-dat-ba` and `combined-dat-ba.lcov`.
Compare the resulting files and every original `DA` count.
The archive includes both corrected merger commands' logs and outputs.
It also preserves the initial `.lcov` inputs and empty results.
The tool ignores that suffix and returns exit 0.
Only the corrected `.dat` results pass exact record comparison.

## Aspect setup

The archive preserves the exact isolated workspace in `aspect-workspace`.
Its `.aspect/check.axl` matches maintained `tools/aspect/delivery_flags.axl` byte for byte.
Its `.aspect/repo_config.axl` matches the maintained repository config.
These mappings have line offset zero.
The copied rc files supply the maintained config names.
The first incomplete setup omitted the `remote-linux` definition and failed.

The final config imports `cached.axl` directly and through `proxy.axl`.
The wrapper emits `ASPECT_LOADING_ONCE` once.
This observation establishes shared import evaluation within this invocation.
It does not establish line coverage or a persistent cross-process cache.

Place the poison fixture at `tools/research_starlark/unloaded.axl` in the isolated workspace.
Do not place it directly under `.aspect` when reproducing the unloaded case.
The archived failed placement demonstrates automatic discovery there.
Use the pinned environment to run `aspect delivery-flags` from the isolated workspace.
Apply the recorded 90-second `timeout` bound.
No Bazel build occurs inside this task.
The task checks flags and prints its two functional assertions.

## Archive format and checks

`SHA256SUMS` inside the archive identifies every included file.
`SOURCE_MANIFEST.json` identifies exact maintained, fixture, and parser source bytes.
`statistics.json` records event and baseline totals.
`conversion/summary.json` retains `orchestration_passed: false`.

Sanitization replaces machine prefixes with these placeholders:

- `${REPO}` identifies the assigned checkout.
- `${EVIDENCE}` identifies the private experiment directory.
- `${BAZEL_OUTPUT_BASE}` identifies this checkout's Bazel output base.
- `${BAZEL_CACHE}` identifies the remaining Bazel cache prefix.
- `${HOME}` identifies the remaining user home prefix.
- `${TMPDIR}` identifies temporary paths.

Wire receipts contain base64-encoded protobuf messages.
The archive changes path bytes and re-encodes affected message lengths.
Field numbers, non-path values, ordering, and statement locations remain unchanged.
Private originals retain the exact initial wire bytes.
Pool ownership receipts and unrelated environment state stay private.

The checks distinguish fresh remote processes, remote cache hits, and action cache hits.
No hosted upload or complete native coverage workflow ran.
The final check summary records formatting, lint, archive validation, and server shutdown.
