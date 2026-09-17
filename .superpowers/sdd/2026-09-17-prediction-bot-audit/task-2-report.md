# Task 2 evidence: store audit outcomes

## Implementation

- Added `Store::new_with_audit` and `Store::connect_with_audit`; existing constructors delegate to the production logging listener.
- `execute` and `execute_at` now emit one `CommandCompleted` after the bounded retry loop. A private transaction result retains the failing `Stage` with the original `StoreError` for outcome classification and unchanged caller errors.
- Transaction boundaries are classified as acquisition, replay, decision, append, commit, or receipt-refresh. Invalid existing command-key shapes retain legacy storage behavior while receiving omitted audit correlation.
- `grant_due` emits `GrantFailed` for corrupt-guild reconstruction and continues. Discovery errors remain returned for the worker boundary; grant command failures are already represented by `CommandCompleted`.
- Added an injected-listener PostgreSQL fixture and recorder. Tests assert typed events plus real committed state; they do not inspect logging JSON.

## Test evidence

### Red

`nix develop --command aspect test //prediction_bot/lib:store_test`

The first red attempt was compile-only because the fixture referenced the then-missing `Store::new_with_audit`. After the seam existed, a runtime-red invocation entered the real PostgreSQL test but this agent did not receive its final runner verdict; runtime-red evidence is therefore **unverified**.

### Observed regression and correction

Parent-supervised command:

```text
nix develop --command aspect test //prediction_bot/lib:store_test //prediction_bot/lib:domain_test //prediction_bot/lib:discord_test
```

Observed result: domain and Discord passed; 16 of 17 store tests passed. The sole failure was the new append-failure assertion expecting a canonical correlation for the legacy key `discord:reject`. The correction uses canonical `discord:99` for that failure case and adds `legacy_command_keys_are_accepted_with_omitted_audit_correlation` to preserve acceptance of legacy-shaped keys while omitting unsafe correlation.

Corrected parent-supervised verification passed:

```text
nix develop --command aspect test //prediction_bot/lib:store_test //prediction_bot/lib:domain_test //prediction_bot/lib:discord_test
```

`store_test` passed fresh in 37.1 seconds; `discord_test` passed fresh; `domain_test` passed from cache. Total command time was 52.6 seconds. The complete result is captured in `task-2-green-fixed.log`.

## Test design

The new store tests are integration tests: each uses a private testcontainer PostgreSQL instance, an out-of-process managed persistence boundary, and a recording audit listener. Assertions are mixed state/output assertions of application behavior: rejected bets preserve balances and emit expected rejection; append/replay/grant failures retain stage and safe outcome; unaffected grants proceed. Measured feedback speed from the observed run was 40.88 seconds for the 17-test store suite. The recorder observes the public listener contract, so the checks remain resistant to internal transaction refactoring.

## Files

- `prediction_bot/lib/src/store.rs`
- `prediction_bot/lib/tests/store_test.rs`

## Concerns

- The `Store::audit` accessor is intentionally produced for later composition and is not yet consumed in this task, so the focused build warns that it is unused.
- `Store::audit` is intentionally unused until Task 3 consumes the composition accessor, yielding a temporary focused-build dead-code warning. It is not suppressed.
