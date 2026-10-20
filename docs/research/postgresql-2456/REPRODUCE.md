# Reproduce the PostgreSQL experiment

Use the research branch and the repository's Nix environment.
The database test requires the configured remote Docker worker.
It does not require a local database.
The profiler files come from Bazel-managed, hash-pinned dependencies.

## Commands

Use this startup flag for every Bazel invocation:

```sh
--host_jvm_args=-Xmx1536m
```

Use these command flags for every build, test, and tool run:

```sh
--config=remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
```

Run the focused test:

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m test \
  //predix/internal/research_sql:research_sql_test \
  --config=remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  --test_output=all --nocache_test_results \
  --build_event_json_file=/private/output/test.bep.json \
  --execution_log_compact_file=/private/output/test.execution_log.binpb.zst
```

Copy the individual files from the target's `test.outputs` directory.
The executed run produced individual files, not `outputs.zip`.
Keep the original output private.
Use an empty export directory for each collector run.

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  //predix/internal/research_sql/export_tool:export_profile \
  --config=remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  -- --evidence /private/output/native --output /private/output/export
```

The valid export exits 0.
It writes `baseline.dat`, two application delta files, and `summary.json`.
It also writes `anonymous-unmeasured-baseline.dat` as a separate discovery artifact.
Exclude that unmeasured artifact from measured merges.

For each fault control, copy the valid native output to a separate directory.
Apply exactly one change to `after-receipts.json`:

1. Remove the file.
2. Replace its contents with `[]`.
3. Replace its contents with `{`.
4. Remove the final row from its array.

Preserve `functional.json` unchanged.
Run the exporter against each modified directory.
Each control must exit 1 without a success summary.
The historical metadata failure provides a fifth independent failure after functional success.

Create two merge directories with these `.dat` inputs:

| File     | First order         | Second order        |
| -------- | ------------------- | ------------------- |
| `01.dat` | baseline            | application B delta |
| `02.dat` | application A delta | application A delta |
| `03.dat` | application B delta | baseline            |

Run this command once for each directory:

```sh
nix develop --command bazel --host_jvm_args=-Xmx1536m run \
  @bazel_tools//tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator:Main \
  --config=remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  -- --coverage_dir=/private/output/merge-ab --output_file=/private/output/merged-ab.dat
```

Require nonempty SF, DA, and BRDA records.
Compare every record against the sum of the baseline and application deltas.
The expected result contains five source identities, 35 lines, and two branches.
Nine lines have count two, and 26 lines have zero.
The branch counts are two and zero.
Both orders must match these records and each other.
The preserved output has 720 bytes.
Its SHA-256 is `2e02fa4933917005e35dfbc70ea9ce1bc2d9be860f92dc791b3de04d8d504b91`.

The repository's Bazel 9.3.0 alias resolves to coverage generator release `v2.9`.
The distribution archive SHA-256 is `aab349130118497d86bc79e3f735f026d3c36e7d38529063e91da1c29cc2ea47`.
The actual `all_lcov_merger_tools_deploy.jar` SHA-256 is `f66fef7cbc12492f4663eab4b3d54e86687a1b8779c02e0447b2881b120988fa`.
The archive preserves its README and BUILD metadata.
The generated launcher JAR is a separate artifact.

## Evidence layout and provenance

`evidence.tar.gz` preserves the following records:

- Native database output from both successful remote runs.
- Exact fixture snapshots for those runs, including the earlier receipt-wrapper version.
- The final exporter source and selected maintained query sources.
- Parser metadata patch, dependency pins, package bytes, and exact profiler source archive.
- Original-source inventories, explicit mappings, native profiles, application receipts, and database logs.
- Fault inputs, failed exports, successful exports, both combined LCOV files, and merge validation.
- Setup failures, focused check results, and BEP execution excerpts.

`source-hashes.json` records the final research sources and selected maintained sources.
`artifact-hashes.json` records every uncompressed archive member.
The report uses the final run's output for capability claims.
The earlier successful run retains its metadata-query failure as historical evidence.
After the final database run, Python edits added explicit types and resolved lint findings.
The extractor edit combined two context managers without changing the selected files.
The final package manifest must match the executed provisioning manifest.
The valid outputs remained byte-identical after that change.

Published logs replace private prefixes with `<CHECKOUT>`, `<RECEIPTS>`, `<BAZEL_CACHE>`, `<HOME>`, and `<TMP>`.
They replace test-session, invocation, and container identities with named placeholders.
Source hashes, image digests, parser positions, counters, and database backend PIDs remain intact.
The backend PIDs connect acknowledgements within each bounded test.
The harness's random database password does not appear in the archive.
Exact maintained-source snapshots retain their existing local-test URL template.
Private pool receipts, full BEP files, and compact execution logs remain outside the branch.
The archive contains selected BEP execution records without remote artifact URLs.
These excerpts establish test strategy separately from command-level cache counts.

The source archive contains upstream code under its original license.
The evidence does not establish a hosted upload or a production collector.
