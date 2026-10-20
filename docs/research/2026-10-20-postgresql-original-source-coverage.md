# PostgreSQL SQL and procedural original-source coverage

This report answers [Determine PostgreSQL SQL and procedural original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2456).
It extends [Classify remaining maintained executable sources for coverage](https://github.com/LowkeyLab/bazel-repo/issues/2444).
The [predecessor report](https://github.com/LowkeyLab/bazel-repo/blob/0dd2c8293fe0375af1377c93996bf848dcd3ff80/docs/research/2026-10-19-remaining-source-coverage.md) supplies the source inventory.

A bounded adapter preserved original statement positions and database receipts across two application tests.
The native profiler supplied positive counts and a real zero for one named PL/pgSQL function.
The anonymous block executed successfully, but the inspected profiler supplied no body profile.
Its four body locations remain unmeasured.
These results do not establish complete original-source SQL coverage.

## Pins and execution

The research base is `df390e4205234e7ea4d8b09be436739a23de27a2`.
The fixture uses the maintained PostgreSQL image and database helper.
It adds a pinned profiler package to the test container.
The [evidence archive and commands](postgresql-2456/REPRODUCE.md) preserve source snapshots, hashes, native output, intermediate LCOV, and failures.

| Component                       | Exact observation or pin                                                                        |
| ------------------------------- | ----------------------------------------------------------------------------------------------- |
| Repository image                | `postgres:18`, digest `sha256:74935e72241653ca55e0414067e6d8763aceb8a810eb51b452253ec3dcfc4336` |
| Actual server                   | PostgreSQL `18.6`, Debian `18.6-1.pgdg13+2`, `server_version_num=180006`                        |
| Extensions                      | `pg_stat_statements=1.12`, `plpgsql=1.0`, `plpgsql_check=2.10`                                  |
| Profiler package                | `postgresql-18-plpgsql-check_2.10.13-1.pgdg13+1_amd64.deb`                                      |
| Package SHA-256                 | `2565ac865fcc07291b96d85126abc443c2babf0438d66c1fa7ba6a98bc2036eb`                              |
| Installed profiler SHA-256      | `f04739955810154d04aec02cfcc7eddc4e7a05c4b203e3b7d7c94dcf803699e5`                              |
| Profiler source archive SHA-256 | `98f6614058e8feaab044feeb2aa5b0b0ee723d4e851fe3cf3ed14a0a4e7f41da`                              |
| Parser                          | `pg_query_go/v6 v6.2.5`, actual parse version `170007`                                          |
| Testcontainers                  | Go `v0.44.0`, remote Docker `28.1.0`, API `1.49`                                                |
| Bazel                           | `9.3.0`                                                                                         |

The [repository module](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/MODULE.bazel) pins the image.
The [Debian source record](https://apt.postgresql.org/pub/repos/apt/pool/main/p/plpgsql-check/plpgsql-check_2.10.13-1.pgdg13+1.dsc) identifies the profiler source archive.
The [parser source](https://github.com/pganalyze/pg_query_go/tree/v6.2.5) supplies SQL and procedural parse APIs.
Its PostgreSQL 17.7 grammar accepted this corpus against the real PostgreSQL 18.6 server.
This result does not establish support for all PostgreSQL 18 syntax.

Both database runs executed on configured remote workers.
No local database ran.
The final BEP records `strategy=remote` and `executionWallTime=12.532s`.
The final command reports ten remote processes and 2,038 action-cache hits.
These counts describe the whole command, not ten database tests.
The database test itself executed freshly.

The container limits were one CPU and 384 MiB.
The harness used `GOMAXPROCS=1`, bounded contexts, startup waits, and cleanup.
Both applications passed, and `database-stop.json` records termination.
No hosted coverage upload ran.

## Original positions and independent discovery

The parser records canonical repository paths, full source hashes, byte extents, statement kinds, and original lines before database startup.
A statement contributes its first executable-token line to this diagnostic model.
Continuation lines do not receive separate statement counters.
This model is a research candidate, not an approved denominator policy.

The disconnected file contains `SELECT 1 / 0`.
The parser discovered its original line without executing it.
The main fixture contains multiline DDL, ordinary SQL, an anonymous block, and an unused DELETE.
The applications execute the CREATE, INSERT, SELECT, and DO statements.
They leave the DELETE and disconnected file unexecuted.

The named function declares one IF with one RETURN in each arm.
Native metadata identifies four procedural statements before any function-body call.
Their execution counters are NULL before measurement.
The exporter initializes those locations to zero without measured body initialization.
Function declaration and extension setup still execute before this baseline.
This is not proof of discovery without database setup.

Native body lines `2,3,4,6` map to original lines `5,6,7,9`.
The dollar delimiter starts on original line 4.
The anonymous parser identifies original lines `16,17,18,21`.
The archive preserves their separate unmeasured inventory.
Their zero-valued discovery artifact never enters the measured merge.

## Capability matrix

| Case                  | Executed observation                                                         | Boundary                                                           |
| --------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Multiline SQL file    | Each application produces four original-source database receipts             | Counters identify complete statements, not every continuation line |
| Unused SQL            | DELETE line 27 and disconnected line 2 retain zero                           | Parser discovery is separate from execution                        |
| Generated Go query    | Both tests execute maintained `GetUserByUsername` through the tracked driver | Explicit operation mapping connects generated SQL to authored SQL  |
| Embedded Rust SQL     | Both tests execute the exact extracted `SELECT COUNT(*) FROM servers`        | The maintained Rust function itself does not execute               |
| Named PL/pgSQL        | Native statement counts are `2,2,2,0` after both tests                       | One bounded function, not general procedural support               |
| Named IF branches     | Native branch coverage is `0.5`, statement coverage is `0.75`                | The adapter maps this IF's two RETURN arms only                    |
| Anonymous DO          | Both applications observe its inserted row and untaken-arm absence           | No anonymous body profile appears                                  |
| Native SQL statistics | Top-level DO, DDL, ordinary queries, and nested INSERT appear                | Normalized SQL alone does not identify original files              |
| Missing collection    | Missing, empty, corrupt, and partial receipts each make export exit 1        | Functional tests remain successful                                 |
| Aggregation           | Both merge orders preserve exact locations, hits, and branches               | Diagnostic LCOV does not resolve the anonymous gap                 |

### SQL receipts and maintained sources

The adapter attaches explicit source metadata to each selected operation.
After successful execution or row closure, it inserts a receipt through the same database connection.
Each receipt preserves source hash, byte extent, original line, actual SQL hash, test name, and backend PID.
This demonstrates the bounded adapter's mapping.
It does not create an automatic mapping for every database client.

Each application produces six receipts.
The final database contains twelve distinct receipts.
The exporter checks actual SQL hashes against explicit mappings.
It also checks source extents and full source hashes.
Receipt insertion errors remain separate from application assertions.

The [authored query](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/predix/internal/sql/query.sql#L11) uses `SELECT *`.
The maintained generated Go query selects explicit columns.
The adapter preserves both texts and names the `GetUserByUsername` operation.
The [sqlc configuration](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/predix/sqlc.yaml) supplies the source relationship.
This experiment does not rerun sqlc or prove every generated query mapping.

The embedded literal comes from [the maintained Rust source](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/nicknamer2/src/discord_server/repo/lib.rs#L136).
Its original byte extent is `[3653,3681)` on line 136.
Its full host-source hash is `95216aef7fe79d0b128d5b86ace84dfa8e2091105b5fae3e22911b967d314343`.
The harness executes a snapshot's extracted SQL through Go.
This proves that literal's database behavior, not Rust call-path coverage.

### Native profiling and the anonymous gap

The profiler reports two named-function calls and four statement counts.
The untaken RETURN has a native zero.
The two application snapshots contain cumulative counters.
The exporter subtracts the first snapshot from the second before merging.
It filters SQL receipts by application name for the same reason.

The final stored-function list contains only `research_branch(boolean)`.
The anonymous block successfully inserts its ELSE value in both tests.
Its untaken INSERT remains absent.
Neither effect establishes native body-line counts.

The exact profiler source archive supplies a separate explanation.
In `src/profiler.c`, `profiler_func_setup` initializes plugin state only when `OidIsValid(func->fn_oid)` is true.
The archive also preserves statement completion and abort counter hooks.
This source inspection supports the observed anonymous limitation for this package.
It does not prove that all PostgreSQL procedural collectors lack anonymous support.

[PostgreSQL's statistics documentation](https://www.postgresql.org/docs/18/pgstatstatements.html) describes normalized query identities and aggregated statistics.
The observed statistics include the anonymous block and its nested INSERT.
They do not preserve this experiment's original source paths or byte extents.
The explicit adapter receipts supply that provenance separately.

## Failures and merge validation

Initial setup failures remain in the archive.
The parser's Bazel metadata omitted nested headers and combined compiler flags.
A research-only BUILD patch corrected those declarations without changing parser algorithms.
A later compile failed because two Docker API packages supplied different types.
The corrected harness uses the repository's current Moby API type.
Neither failure establishes a database collector limitation.

The first successful database run retained a failed metadata query.
`plpgsql_check_version()` was unavailable, while both application assertions passed.
The final run corrected that query and preserved server and extension metadata separately.
It also corrected the receipt wrapper to hash the actual query received by row closure.
Both successful runs and their source snapshots remain available.

| Export control                    | Functional result | Export exit |
| --------------------------------- | ----------------- | ----------- |
| Final native output               | Pass              | 0           |
| Missing final receipt file        | Pass              | 1           |
| Empty final receipt array         | Pass              | 1           |
| Corrupt final JSON                | Pass              | 1           |
| Partial final receipt set         | Pass              | 1           |
| Historical metadata-query failure | Pass              | 1           |

The baseline contains 35 zero locations across five source identities.
The measured merge contains nine positive locations with count two and 26 zero locations.
It contains two branch records, with counts two and zero.
Both orders use `.dat` inputs accepted by Bazel's pinned coverage merger.
Both outputs contain nonempty SF, DA, and BRDA records.
An independent comparison checks every record against the sum of the baseline and application deltas.
Both outputs are byte-identical and contain 720 bytes.

## Checks and remaining decisions

The final remote application test passed freshly.
Bazel built the exporter and merger with configured remote services.
Their `bazel run` subprocesses executed locally against preserved database output.
Focused Ruff, Ty, and KeepSorted checks passed after the exporter type correction.
The archive records formatter, build, and final evidence checks separately.
Cached build actions do not replace the fresh database observation.

A later decision can consider explicit statement adapters and named-function profiling as separate candidates.
The anonymous body still needs a demonstrated collector with original positions and untaken-arm evidence.
General generated-query mapping, maintained Rust execution, dynamic SQL origins, transaction failures, and concurrent sessions remain unproved.
General procedural branch identities also remain unproved.
This report approves no collector, exception, membership change, threshold change, or upload policy change.
