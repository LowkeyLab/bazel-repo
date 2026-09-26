# Task 2: persistence, bulk, and export observation producers

## Result

`NameService::with_observer` retains the concrete PostgreSQL connection and emits typed mutation and read facts when database work resolves. The existing `new` constructor remains for compatibility. Mutations record committed only after autocommit returns; duplicate and missing-entry outcomes are rejected; database failures are failed. `NameService::with_context` accepts a caller-supplied observation context for later request correlation. Each fact refreshes `occurred_at` at emission, and reads, mutations, and bulk summaries carry elapsed durations.

Bulk import and delete retain their existing per-item autocommit and partial-success behavior. Each batch child mutation has the summary operation as parent. Summary counts reflect parsed items; malformed YAML yields zero attempts and unknown input count, empty batches succeed, duplicate-only import succeeds with skips, and mixed success/failure is partial. The category vector aggregates each category once, including duplicate skips and repeated missing deletes.

Both web and API export handlers use the observed service. `ExportPrepared` records query failure or completed YAML serialization, with entry count and actual prepared byte count on success. This means prepared, not downloaded. `NameState` carries the observer; startup temporarily constructs a local dispatcher with `LoggingListener`, and test state uses an empty dispatcher. Task 4 can replace the startup composition with its shared application observer. Name handlers use `tracing::instrument(skip_all)` and no longer log raw name/ID/error values directly.

## RED and GREEN evidence

- Initial RED was compile-only: `observations_service_tests` could not compile because `NameService::with_observer` did not yet exist. A first run also found the new target absent until it was declared after Gazelle.
- Semantic RED: with service producers but before export handler wiring, the real-PostgreSQL target ran five tests; four passed and `web_export_records_prepared_bytes_after_serialization` failed because no `ExportPrepared` fact was recorded.
- GREEN: `nix develop --command aspect test //nicknamer/server/lib/tests:tests --test_output=errors` passed all six targets after final edits. Test logs show 12 producer, 9 observation boundary, 13 auth, 33 service, 65 name endpoint, and 3 web endpoint bodies: 135 total.
- `nix develop --command bazel run //:gazelle` ran immediately after each Rust source edit. `nix develop --command aspect format --scope=all` finished with no files modified on the final run. `git diff --check` passed.

## Coverage and handoff

Real PostgreSQL tests cover committed create, duplicate rejection with one persisted row, missing delete, failed create after dropping the table, malformed/empty/duplicate-only import, mixed import with one committed row and one database failure, empty/repeated/missing delete, and web/API export success. Web export query failure is exercised by dropping the table. The response-error check forces an accessible error response after a successful service write and verifies that the committed fact remains; the real Askama template failure path was not triggered. YAML serialization failure is classified by production code but is not forceable with the current `BTreeMap<String, BTreeMap<u64, String>>` data shape. Request-scoped context propagation belongs to Task 3; handlers currently construct observed services without calling `with_context`.
