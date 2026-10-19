# Reproduce the bounded JVM experiment

Use the repository Nix shell and Bazel toolchains.
The commands use the repository-configured BuildBuddy services.
They need access to those services.
The `run` command builds through the configured service, then executes the adapter locally.

```bash
jvm_bazel() {
  local command="$1"
  shift
  nix develop --command bazel --host_jvm_args=-Xmx1536m "$command" \
    --config=ci-remote --jobs=8 \
    --local_resources=cpu=2 --local_resources=memory=2048 "$@"
}

jvm_bazel run //:gazelle

jvm_bazel coverage \
  --build_tag_filters= \
  --instrumentation_filter='^//tools/coverage/jvm_fixture' \
  --instrument_test_targets --cache_test_results=no \
  --remote_download_outputs=all \
  //tools/coverage/jvm_fixture:fixture_test \
  //tools/coverage/jvm_fixture:disconnected_java \
  //tools/coverage/jvm_fixture:disconnected_kotlin
```

The completed run retained the actual source and class inputs in `evidence`.
Use those inputs to repeat the adapter without dependence on a later compilation.

```bash
research_data="$PWD/research/jvm-2448/evidence"
research_scratch=$(mktemp -d)
export research_data research_scratch
python3 - <<'PY'
import json
import os
from pathlib import Path
from zipfile import ZipFile

source = Path(os.environ['research_data'])
scratch = Path(os.environ['research_scratch'])
(scratch / 'split').mkdir()
for row in json.loads((source / 'class-owners.json').read_text()):
    entry = row['class'] + '.class.uninstrumented'
    with ZipFile(source / row['input_jar']) as original:
        data = original.read(entry)
    with ZipFile(scratch / 'split' / row['adapter_jar'], 'w') as isolated:
        isolated.writestr(entry, data)
        isolated.writestr(
            'owner-paths-for-coverage.txt',
            row['source'] + '////' + row['source_key'] + '\n',
        )
PY

jvm_bazel run //tools/coverage/jvm_fixture:baseline -- \
  "$research_scratch/baseline.dat" empty "$research_scratch"/split/*.jar
jvm_bazel run //tools/coverage/jvm_fixture:baseline -- \
  "$research_scratch/parent.dat" "$research_data/parent.exec" \
  "$research_scratch"/split/*.jar
jvm_bazel run //tools/coverage/jvm_fixture:baseline -- \
  "$research_scratch/child.dat" "$research_data/child.exec" \
  "$research_scratch/split/probe_Child.jar"
```

The negative controls change the expected child input after the functional test passes.
The first control removes that input.
The second control replaces that input with corrupt bytes.
Each adapter command must fail.
These controls execute locally. They do not prove native collector failure detection.

```bash
cp "$research_data/child.exec" "$research_scratch/expected-child.exec"
rm "$research_scratch/expected-child.exec"
jvm_bazel run //tools/coverage/jvm_fixture:baseline -- \
  "$research_scratch/missing.dat" "$research_scratch/expected-child.exec" \
  "$research_scratch/split/probe_Child.jar"

printf 'not-jacoco-execution-data' > "$research_scratch/expected-child.exec"
jvm_bazel run //tools/coverage/jvm_fixture:baseline -- \
  "$research_scratch/corrupt.dat" "$research_scratch/expected-child.exec" \
  "$research_scratch/split/probe_Child.jar"
```

Merge the reports in both orders with the pinned Bazel merger.
The result must preserve measured line hits and branch identities.

```bash
printf '%s\n' "$research_scratch/baseline.dat" \
  "$research_scratch/parent.dat" "$research_scratch/child.dat" \
  > "$research_scratch/forward.txt"
printf '%s\n' "$research_scratch/child.dat" \
  "$research_scratch/parent.dat" "$research_scratch/baseline.dat" \
  > "$research_scratch/reverse.txt"

jvm_bazel run @bazel_tools//tools/test:coverage_report_generator -- \
  --reports_file="$research_scratch/forward.txt" \
  --output_file="$research_scratch/forward.dat"
jvm_bazel run @bazel_tools//tools/test:coverage_report_generator -- \
  --reports_file="$research_scratch/reverse.txt" \
  --output_file="$research_scratch/reverse.dat"
cmp "$research_scratch/forward.dat" "$research_scratch/reverse.dat"
```

Sanitization replaces execution session names and timestamps.
It preserves class IDs, probe arrays, source names, executable lines, and branch outcomes.
Generated launcher copies replace local absolute paths with `[LOCAL_PATH]`.
Launcher copies also remove trailing whitespace.
Child output replaces the remote execution root with `[REMOTE_EXECROOT]`.
No credential headers or worker identities occur in the published command records.
Private raw receipts preserve the original commands and outputs.
