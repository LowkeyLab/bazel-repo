# Task 1 report: typed operational audit contract

## Status

Complete. The prediction bot library now exports a typed, sanitized operational audit contract, safe store and Discord classifiers, canonical command-key filtering, and a synchronous structured logging listener.

## Files

- `prediction_bot/lib/src/audit.rs`
  - Defines `AuditListener`, `SharedAudit`, `logging_listener`, all requested event and classification enums, `Failure`, `Outcome`, and `AuditEvent`.
  - Provides `store_outcome`, `discord_failure`, and `canonical_command_key`.
  - Converts existing domain commands to `CommandKind` for the store producer added in Task 2.
- `prediction_bot/lib/src/audit/logging.rs`
  - Maps each event variant to a stable event name and named tracing fields.
  - Uses INFO for success/rejection, WARN for acknowledgement/delivery and timeout failures, and ERROR for other operational failures.
  - Never formats an entire event or raw error.
- `prediction_bot/lib/src/audit_tests.rs`
  - Tests typed classification, stage-sensitive replay handling, data minimization, allowlisted SQLSTATE values, safe Discord categories, and canonical correlation keys.
  - Contains no JSON, rendered severity/layout, writer capture, or log snapshot assertions.
- `prediction_bot/lib/src/lib.rs`
  - Exports `audit` and includes its unit tests.
- `prediction_bot/lib/BUILD.bazel`
  - Gazelle required no changes because the library already uses a recursive source glob and already declares SQLx, Serenity, and tracing.

## TDD and verification evidence

1. Baseline before changes:
   - `nix develop --command aspect test //prediction_bot/lib:discord_test`
   - Passed from cache. Bazel elapsed 1.697s; Aspect elapsed 7.1s.
2. Red phase after adding the tests and an empty contract module:
   - `nix develop --command aspect test //prediction_bot/lib:discord_test`
   - Failed to compile as intended because `Failure`, `FailureCategory`, `Outcome`, `Rejection`, `Stage`, `canonical_command_key`, `discord_failure`, and `store_outcome` did not exist.
   - Bazel elapsed 5.903s; Aspect elapsed 7.2s.
   - The first red compile also showed that Serenity's response structs are non-exhaustive. The test was corrected to avoid manufacturing third-party internals; HTTP status/code extraction remains code-review verified.
3. First green compile attempt:
   - Failed only because the pinned SQLx 0.9 `DatabaseError` test fixture also requires `kind()`.
   - Added `ErrorKind::Other` to the fixture; this was a test compatibility correction, not a production behavior failure.
4. Focused green phase:
   - `nix develop --command aspect test //prediction_bot/lib:discord_test`
   - Passed. Bazel elapsed 12.124s; Aspect elapsed 13.6s.
5. Final regressions after review and formatting:
   - `nix develop --command aspect test //prediction_bot/lib:discord_test //prediction_bot/lib:domain_test`
   - Both passed. `discord_test` executed; `domain_test` was cached. Bazel elapsed 13.879s; Aspect elapsed 15.4s.
   - `nix develop --command aspect build //prediction_bot/lib:lib`
   - Passed. Bazel elapsed 9.941s; Aspect elapsed 11.7s.
   - `nix develop --command aspect lint //prediction_bot/lib:lib`
   - Passed with no findings. Bazel elapsed 11.778s; Aspect elapsed 13.9s.
   - `nix develop --command aspect format --scope=all`
   - Passed and reported no modified files. Aspect elapsed 8.8s.
6. Repository workflow:
   - `nix develop --command bazel run //:gazelle` was run immediately after every Rust source edit. Gazelle made no BUILD changes.

## Classification and safety review

- Only `Stage::Decide` maps `DomainError::Invalid` reasons to expected rejections. The same reason at `Stage::Replay` is an operational `History` failure.
- Unknown decision reasons collapse to `Rejection::InvalidInput`; no raw reason enters an event.
- Database configuration, I/O, pool timeout, SQLSTATE class, history, metadata, and overflow errors collapse to typed categories. SQLSTATE crosses the boundary only when it is exactly five uppercase ASCII letters or digits.
- Discord unsuccessful responses retain only numeric HTTP status and Discord code. Request timeouts, transport errors, and unknown errors retain only their safe categories.
- Correlation keys cross the boundary only after numeric canonical round-trip validation. Public store key acceptance remains unchanged; Task 2 can emit `None` when `canonical_command_key` rejects a legacy key.
- Logging uses explicit variant destructuring and typed fields. No raw SQLx/Serenity object, URL, credential, message, or payload is logged.

## Test quality

- Protection against regressions: demonstrated by the missing-contract red compile and green execution. The tests directly protect stage-sensitive classification, safe-field omission, SQLSTATE allowlisting, and canonical key filtering.
- Resistance to refactoring: inferred from output-based assertions against the public event-classification API; tests do not inspect helper calls or log rendering.
- Feedback speed: demonstrated by the final focused suite completing in 15.4s including Bazel orchestration; individual Rust test binaries report 0.0s execution.
- Maintainability: direct table-driven cases use concrete existing error types. One small SQLx database-error fixture exists solely to supply controlled SQLSTATE input.

## Concerns and fidelity gaps

- Per the approved design and explicit user constraint, logging severity and field layout were code-reviewed and are not asserted by tests.
- Serenity marks HTTP response error structures non-exhaustive, so Task 1 does not fabricate an unsuccessful response to unit-test numeric extraction. The mapping is a direct destructure of the pinned Serenity type; Task 3's controlled transport work is the appropriate higher-fidelity boundary check.
- No PostgreSQL integration behavior changed in this task. The controller separately ran the existing store baseline, which was served from remote cache rather than a fresh PostgreSQL execution.
