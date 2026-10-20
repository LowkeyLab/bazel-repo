# Reproduce the PostgreSQL experiment

Use this research branch and its Nix environment.
Use a local Docker daemon.
The database container uses at most one CPU and 384 MiB.
The commands below disable remote execution, cache access, cache uploads, and BES.

Use this startup flag for each Bazel command:

```sh
--host_jvm_args=-Xmx1536m
```

Use these command flags for each Bazel build, test, or tool run:

```sh
--jobs=4 --local_resources=cpu=2 --local_resources=memory=2048 \
--bes_backend= --remote_cache= --remote_executor= --noremote_upload_local_results
```

Run the database experiment:

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m test \
  //predix/internal/research_sql:research_sql_test \
  --jobs=4 --local_resources=cpu=2 --local_resources=memory=2048 \
  --bes_backend= --remote_cache= --remote_executor= --noremote_upload_local_results \
  --nocache_test_results --test_output=all
```

Copy the individual `test.outputs` files into a new private evidence directory.
Preserve earlier runs in separate directories.
Bazel output directories can have read-only permissions.
Do not change the original outputs.

Run the collector:

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  //predix/internal/research_sql/export_tool:export_profile \
  --jobs=4 --local_resources=cpu=2 --local_resources=memory=2048 \
  --bes_backend= --remote_cache= --remote_executor= --noremote_upload_local_results \
  -- --evidence /private/output/native --output /private/output/export
```

The collector writes `baseline.dat`, two application delta files, and `summary.json`.
The archive's `run_controls.py` records the exact ten fault changes and both merge commands.
Each fault must exit 1 while the functional record remains unchanged.
The archive's `verify_records.py` contains independently authored original line and branch expectations.
It compares the baseline, both deltas, and both merged records.

Run the pinned merger once per input order:

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  @bazel_tools//tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator:Main \
  --jobs=4 --local_resources=cpu=2 --local_resources=memory=2048 \
  --bes_backend= --remote_cache= --remote_executor= --noremote_upload_local_results \
  -- --coverage_dir=/private/output/merge-ab --output_file=/private/output/merged-ab.dat
```

The first order is baseline, application A, application B.
The second order is application B, application A, baseline.
Both outputs must match all expected original records and each other.

Run the formatter with Bazel flags in their own options:

```sh
nix develop --command aspect --host_jvm_args=-Xmx1536m format \
  --scope=all --upload-format-diff=false \
  --bazel-flag=--jobs=4 --bazel-flag=--local_resources=cpu=2 \
  --bazel-flag=--local_resources=memory=2048 --bazel-flag=--bes_backend= \
  --bazel-flag=--remote_cache= --bazel-flag=--remote_executor= \
  --bazel-flag=--noremote_upload_local_results
```

Do not place these options after an argument separator.
Do not publish a raw command announcement because inherited options can contain credentials.

## Evidence archive

`evidence.tar.gz` contains the final database output, source snapshots, explicit body plans, source receipts, profiles, and original SQL mappings.
It also contains exact exports, expected records, both merged files, ten fault inputs, and command logs.
The source and package pins remain in `MODULE.bazel`, `go.mod`, and `go.sum`.
The archive includes the pinned upstream profiler source under its original license.
`source-hashes.json` binds the research sources and selected maintained sources to this run.
`artifact-hashes.json` binds each uncompressed archive member.

Raw coverage and mapping records retain their bytes.
Published command logs replace private path prefixes and configured header values.
The corrected raw command announcement remains outside the archive.
The archive check also rejects configured header values and database URL credentials.
The log preserves backend PIDs because those PIDs connect the actual Rust query and its receipt.

The final test log contains harmless `file: command not found` messages from Bazel's output-manifest helper.
The individual evidence files exist and the collector checks them.
No test failure follows from those helper messages.
The final database stop record states that the container stopped.

## Approved remote phase

The approved remote commands tested commit `5b8a02edbe0efa4ea08499e8f3d478004800416a`.
They used the same target and resource limits as the local command.
Replace the remote-disable flags with `--config=remote --remote_download_toplevel`.
Keep `--nocache_test_results` to require a fresh test attempt.

The first command recorded a compact execution log.
The focused repeat requested JSON execution logging:

```sh
--execution_log_compact_file= --execution_log_json_file=/private/output/execution.json
```

Both commands also recorded a private BEP JSON file.
Read the TestRunner record for the exact target.
Check its runner, cache result, exit code, and digest.
Compare that execution record with the BEP strategy and actual database outputs.
A remote build or cache hit does not prove remote test execution.

A private supplemental archive preserves the approved remote phase separately from the initial published archive.
The report links both approved BuildBuddy invocations.
The supplemental archive and full repeated-check records require private runtime-receipt access.
The local exporter, fault controls, and mergers used the original remote-disable flags.
The second test's original-record exports must equal the first test's exports byte for byte.
