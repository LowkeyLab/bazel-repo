# PostgreSQL anonymous bodies and original SQL provenance

Date: 2026-10-20.
Research ticket: [#2484](https://github.com/LowkeyLab/bazel-repo/issues/2484).
Predecessor: [#2456](https://github.com/LowkeyLab/bazel-repo/issues/2456).
Base revision: `39075c1280f357d58b06a9a471e617cd73949bdf`.

The bounded collector measured anonymous bodies at their original positions.
It also measured the exact maintained login migration, including its dynamic `EXECUTE` statement.
The native profiler still omitted anonymous bodies.
The counter prototype changes `FOUND` and loses counters after transaction rollback.
These limits prevent a recommendation for general collection.
No coverage exception follows from this result.

## Scope and sources

The [approved scope](https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818) includes application code, test code, and executable helpers.
This branch contains a throwaway experiment.
It changes no production collector, threshold, combined-report policy, or Codecov policy.

PostgreSQL treats `DO` as a transient anonymous function with one execution.
This differs from a persistent named function.
See the [PostgreSQL 18 DO specification](https://www.postgresql.org/docs/18/sql-do.html).
The pinned profiler initializes its function profile only when `OidIsValid(func->fn_oid)` succeeds.
See `src/profiler.c:1430` in the preserved upstream archive and the [upstream source](https://github.com/okbob/plpgsql_check/blob/72fa03e/src/profiler.c#L1430).
The final function inventory contains only `research_branch(boolean)` after both applications and all anonymous controls.

The experiment uses PostgreSQL 18.6 from the repository's pinned `postgres:18` image.
The image digest is `sha256:74935e72241653ca55e0414067e6d8763aceb8a810eb51b452253ec3dcfc4336`.
The installed package is `postgresql-18-plpgsql-check` 2.10.13.
The loaded extension reports 2.10.
The parser dependency is `pg_query_go/v6` 6.2.5.
Its runtime parser version is 170007, which corresponds to PostgreSQL 17.7.
This experiment does not establish PostgreSQL 18 grammar coverage.
Bazel is 9.3.0.
The local Docker server is 29.8.1.

## Mechanism and provenance

The parser discovers original statement positions before body execution.
The named profiler supplies zero-execution statement metadata.
The body adapter maps procedural line numbers to original source lines and first-token byte positions.
It inserts `PERFORM public.research_body_hit(...)` before each supported statement.
For a plain `BEGIN`, it inserts the call after `BEGIN`.
The counter table starts with an explicit zero for each original body location.

The adapter accepts the inspected block, IF, SQL, dynamic EXECUTE, PERFORM, and RAISE forms.
It rejects unknown statement kinds, shared statement lines, and bodies with direct status or transaction references.
This restriction does not prove semantic preservation for all accepted text.
The five body plans preserve original source hashes, original SQL hashes, generated SQL hashes, byte positions, and branch-arm identities.
The exporter checks these identities and cumulative counter deltas.

The maintained migration snapshot equals `prediction_bot/migrations/002_application_login.sql` byte for byte at the base revision.
Each application executes its instrumented original body and original GRANT statement in an isolated database container.
The test checks role creation and runtime-role membership separately from body counters.
The dynamic `EXECUTE` maps to original line 4.
The prototype does not attribute generated role-command text to a new maintained source file.
The experiment does not execute the application's migration driver.

The Go application calls the maintained `db.GetUserByUsername` implementation.
Its receipt maps generated explicit columns back to the authored `SELECT *` in `predix/internal/sql/query.sql:11`.
The Rust binary calls the maintained `discord_server_repo::Repo::count` method.
Its query maps to `nicknamer2/src/discord_server/repo/lib.rs:136`.
A separate receipt records the same database backend PID before and after that method call.
The database log records the actual query with that PID and `coverage_research_rust` application name.
See the [logging specification](https://www.postgresql.org/docs/18/runtime-config-logging.html) for these log fields.
Execution of an extracted Rust query is not the basis for this claim.
The research binary does not start the full application server.

## Results

The final fresh local test passed in 3.3 seconds.
Each application produced ten source receipts.
The combined result contains nine original source identities, 57 candidate lines, and eight branch records.
Twenty-five lines have count two.
Thirty-two lines remain zero.
Four branches have count two, and four remain zero.

| Source                     | Original body lines per application | Measured counts |
| -------------------------- | ----------------------------------- | --------------- |
| Maintained login migration | 3, 4                                | 1, 1            |
| Application DO fixture     | 16, 17, 18, 21                      | 1, 1, 0, 1      |
| Test DO fixture            | 3, 4, 5, 6, 8                       | 1, 1, 1, 0, 1   |
| Executable helper DO       | 3, 4, 5, 7                          | 1, 1, 0, 1      |
| Disconnected DO            | 3, 4                                | 0, 0            |
| Named native control       | 5, 6, 7, 9                          | 1, 1, 1, 0      |

Top-level SQL records also preserve executed and disconnected statements.
The migration records include original lines 2 and 10.
All 24 authored statements in the maintained query file remain in the baseline.
Only its selected query receives hits.
The exporter does not treat a successful DO acknowledgement as evidence for every body line.

An independent expected record set matches every SF, DA, and BRDA record in the baseline and both application deltas.
Both Bazel merge orders match the expected sum and each other byte for byte.
The merged SHA-256 is `5314c126859675736105af485139276e093227a2bd32667773a84907938d8823`.
The experiment uses Bazel's pinned coverage generator, not a replacement merger.

Ten artifact controls reject missing, empty, corrupt, partial, or inconsistent coverage evidence after functional success.
The controls include a missing Rust call receipt and a missing database session witness.
Every control preserves the successful functional record and exits 1 without a success summary.
The archive retains exact raw inputs, exported records, independent expected records, and failed control logs.

## Remaining feasibility question

The `FOUND` control returns false for the original body and true after a counter call.
The rollback control executes a counter update inside a transaction, then rolls the transaction back.
Its final count remains zero.
Thus a zero can conceal executed code when the application rolls its transaction back.
PostgreSQL documents the `PERFORM` effect on `FOUND` and statement diagnostics in its [basic-statement specification](https://www.postgresql.org/docs/18/plpgsql-statements.html).
These runtime results establish concrete limits beyond syntax support.

Can an anonymous PL/pgSQL observer record statement and branch execution without changing status variables or depending on application transaction commits?
That observer must also preserve original source identities and support the server's grammar.
A native execution hook with transaction-independent counters is a candidate for further research.
This experiment does not implement or check that hook.
The current prototype supports the successful bounded corpus only.
It does not establish general exception, rollback, trigger, loop, dynamic-source, or concurrent-session coverage.

## Validation and command errors

The initial database test, valid export, ten fault controls, and both merges ran locally with remote services disabled.
Focused Clippy, Ruff, Ty, and KeepSorted reports contained no findings.
The corrected full-tree formatter passed with no file changes.
The archive preserves commands and separate check logs.
The initial record did not establish remote test execution.
The approved check below supplies that evidence.
Neither phase establishes a Codecov upload.
A cached prerequisite is not evidence of test execution.

Three earlier formatter calls contacted the configured remote cache and BES while remote-payload approval remained pending.
One call omitted override flags.
Two calls placed override flags after the argument separator, which sent those flags to the formatter.
Those two calls reported an invalid `find` predicate despite a success summary.
The report does not count those summaries as valid formatting evidence.
The preserved incident record identifies all three BuildBuddy invocations and action summaries.
Those logs establish cache reads and BES contact.
They do not establish an artifact upload or fresh remote execution.

Installed Aspect help established the corrected `--bazel-flag` syntax.
A corrected command announcement exposed a configured header in private diagnostic output.
The raw announcement remains private.
Published logs remove that entire announcement and all configured header values.
The final archive check rejects those values before publication.

See [reproduction and evidence](postgresql-anonymous-provenance/REPRODUCE.md).

## Approved BuildBuddy check

The user later approved the exact GitHub publication and research transfers to `remote.buildbuddy.io`.
The published initial archive remains byte-identical to the approved archive.
Its SHA-256 is `7fd79a8ee80db8d9b484200abfc88a3bc340fcba3e0bddc9b023bea8927e1041`.
Both approved remote commands tested commit `5b8a02edbe0efa4ea08499e8f3d478004800416a`.
The later report commit changes only documentation and evidence.

The [first BuildBuddy invocation](https://app.buildbuddy.io/invocation/a3ae993c-9e65-4286-9248-1d3d376447ac) passed the database test in 24.915 seconds.
Its BEP records the remote strategy, one run, and one attempt.
The command used `--nocache_test_results`.
Its JSON omits cache booleans, so their absence alone does not prove a cache miss.
The action summary records 3,678 remote cache hits, 1,185 internal processes, and nine remote processes.
These action counts do not mean that nine tests ran.
The worker's Docker server reports 28.1.0.

The downloaded database records passed the local exporter and independent original-record check.
All ten malformed-output controls rejected those remote records after functional success.
Both local merge orders produced the same expected SHA-256 as the initial local experiment.
These exporter, fault, and merge tools ran locally with remote services disabled.
They do not constitute additional remote tests.

The [focused repeat](https://app.buildbuddy.io/invocation/0c6d0d08-d193-4673-9252-6ed18c674473) passed in 25.704 seconds.
Its JSON execution record identifies the exact TestRunner target.
That record states `runner=remote`, `cacheHit=false`, `remoteCacheable=false`, and `exitCode=0`.
The BEP independently records the remote strategy.
The repeat command also disables test-result caching.
Together these records establish a fresh remote test execution.
The action summary records 4,817 action-cache hits, one internal process, and one remote process.
The repeat's baseline and both original-source delta files equal the first remote run's exports byte for byte.
The raw remote records and supplemental archive remain private in runtime receipts.
The private archive preserves both commands, BEP excerpts, actual database profiles, effect receipts, and local verification records.
Raw BEP files and full execution logs remain private.
Published excerpts omit command environments and configured headers.

GitHub publication approval covered the original archive only.
Automatic review rejected publication of the supplemental archive.
This report publishes aggregate results and links to the approved BuildBuddy invocations.
It does not publish the rejected raw payload in another form.
The branch preserves the original approved archive unchanged.
Readers need access to the private runtime receipts to inspect the full repeated-check records.
The repeat TestRunner action digest is `9e9f6cd4cce7e42d6ec67fbbc89da979c53a26b3c091b1e9958fac4d556cdd2a/346`.
