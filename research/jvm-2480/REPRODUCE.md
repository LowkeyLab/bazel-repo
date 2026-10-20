# Reproduce the local experiment

The pool assignment supplies a refreshed checkout at the base recorded in the report.
Use the repository Nix shell.
The following wrapper disables configured remote execution, remote cache access, and event uploads.

```bash
jvm_local() {
  local command="$1"
  shift
  nix develop --command bazel --host_jvm_args=-Xmx1536m "$command" \
    --config=ci-remote --remote_executor= --remote_cache= --bes_backend= \
    --remote_upload_local_results=false --jobs=4 \
    --local_resources=cpu=2 --local_resources=memory=2048 "$@"
}

jvm_local run //:gazelle
jvm_local query 'kind("(java|kt_jvm)_.*", //tools/coverage/jvm_fixture:all)' \
  --output=xml
jvm_local cquery 'kind("(java|kt_jvm)_.*", //tools/coverage/jvm_fixture:all)' \
  --output=starlark \
  --starlark:expr='json.encode({"owner":str(target.label),"jars":[o.class_jar.path for o in providers(target)["@@rules_java+//java/private:java_info.bzl%JavaInfo"].java_outputs],"source_jars":[f.path for f in providers(target)["@@rules_java+//java/private:java_info.bzl%JavaInfo"].source_jars]})'

jvm_local coverage --build_tag_filters= \
  --instrumentation_filter='^//tools/coverage/jvm_fixture' \
  --instrument_test_targets --cache_test_results=no \
  --remote_download_outputs=all \
  --build_event_json_file=/tmp/jvm-2480-local-bep.json \
  //tools/coverage/jvm_fixture:fixture_test \
  //tools/coverage/jvm_fixture:disconnected_java \
  //tools/coverage/jvm_fixture:disconnected_kotlin

inputs="$PWD/research/jvm-2480/evidence/inputs"
jvm_local run //research/jvm-2480:analyze -- \
  "$PWD/research/jvm-2480/evidence"
jvm_local run //tools/coverage/jvm_fixture:original -- empty "$inputs"/*.jar
jvm_local run //tools/coverage/jvm_fixture:original -- \
  "$inputs/parent.exec" "$inputs"/*.jar
jvm_local run //tools/coverage/jvm_fixture:original -- \
  "$inputs/child.exec" "$inputs"/*.jar
```

The published input directory contains artifacts from the passed local run.
A new coverage run writes class jars under `bazel-bin/tools/coverage/jvm_fixture`.
It writes parent execution data under the test's `_coverage` directory.
It writes child execution data under the test's `test.outputs` directory.
Copy those inputs before changing coverage configuration.

For each raw analysis, compare class names and line sets with `expected-lines.json`.
Require zero instruction hits in the empty-data baseline.
Require positive parent instructions at `kprobe/ExecutedKotlinKt:3`.
Require positive child instructions at `probe/Child:7`.
Require zero parent instructions at that child-only line.
Merge line hit states with logical OR in both input orders.
Require identical outputs.
Use only unique ownership records for partial canonical LCOV.
Retain ambiguous classes in the diagnostic output.
Do not fabricate branch identities from aggregate counts.

After functional success, run each negative control separately:

```bash
jvm_local run //tools/coverage/jvm_fixture:original -- \
  /tmp/absent-child.exec "$inputs/child.jar"
printf 'invalid-jacoco-data' > /tmp/jvm-2480-corrupt.exec
jvm_local run //tools/coverage/jvm_fixture:original -- \
  /tmp/jvm-2480-corrupt.exec "$inputs/child.jar"
jvm_local run //tools/coverage/jvm_fixture:original -- \
  "$inputs/child.exec" /tmp/absent-metadata.jar
```

Each command must exit with an error.
The published evidence retains each separate outcome.
