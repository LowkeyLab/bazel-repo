# Original-action evidence compatibility baseline

Issue [#2384](https://github.com/LowkeyLab/bazel-repo/issues/2384) characterizes the
app before the evidence consolidation in #2385 and #2386. These tests run
against the packaged app and concrete PostgreSQL adapter. Controlled
tests append app payloads with `ApplicationEvent` and `CommandTransaction`.
Ranking envelopes come from real domain commands through `Store`. The tests use the real interpreter. Malformed app-owned data remains inside valid storage envelopes.

The following behavior is compatibility evidence, including surprising outcomes.
Changing it requires a separate behavior decision. An HTTP 500 is an observable
reader-wide validation failure. Recovery's checking response is HTTP 200, saved
recovery redirects with HTTP 303, and a missing correlated intent is HTTP 403.

## Added characterization

All tests are in `action_test.rs`, within `//book_smartz/web:application_test`.

| Test / case                                                                            | Observable contract                                                                                                                                                                                                                                         |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `evidence_controls_preserve_intent_order_duplicates_and_payload_identity`              | Recovery controls follow intent order, retain duplicates, use payload identity, ignore unknown stages, and block new Adds. Reads and denied recovery leave both histories unchanged.                                                                        |
| `saved_before_rejection`                                                               | A complete ranking match wins over a retained rejection during recovery. Reader-wide controls exclude the action.                                                                                                                                           |
| `rejected_before_prepared`                                                             | A retained rejection wins before recovery decodes invalid prepared data. Reader-wide rejection exclusion skips that prepared identity.                                                                                                                      |
| `later_preparation`                                                                    | First matching preparation wins over later malformed preparation, across Add/preference stage families.                                                                                                                                                     |
| `unrelated_intent`                                                                     | Reader-wide reconstruction rejects an unrelated malformed intent. Saved-action recovery ignores it.                                                                                                                                                         |
| `first_intent` / `later_intent`                                                        | Reader-wide reconstruction validates every recognized intent. Recovery uses the first correlated intent, so only the first malformed intent prevents recovery.                                                                                              |
| `first_rejection` / `later_rejection`                                                  | Reader-wide reconstruction validates every rejection reason. Recovery uses the first rejection: a malformed first reason doesn't select a later valid reason, while a valid first reason wins over malformed preparation.                                   |
| `missing_document_id` / `invalid_document_id`                                          | Reader-wide reconstruction fails when an unresolved action's prepared identity is missing or invalid. Recovery remains checking without appending either history.                                                                                           |
| `unrelated_preparation`                                                                | Neither query eagerly validates an invalid document identity on a preparation with no enumerated intent.                                                                                                                                                    |
| `unrelated_rejection`                                                                  | A malformed rejection for another action fails the reader-wide query but doesn't prevent saved-action recovery.                                                                                                                                             |
| `saved_before_bad_rejection`                                                           | Complete saved evidence wins before malformed rejection data in single-action recovery. The reader-wide query still fails.                                                                                                                                  |
| `missing_correlated_intent`                                                            | Matching preparation and a payload naming the action don't substitute for a correlated intent in recovery.                                                                                                                                                  |
| `different_payload_identity`                                                           | Reader-wide controls name the intent payload's action. Recovery selects by retained correlation without requiring equality.                                                                                                                                 |
| `evidence_first_preparation_requires_the_complete_envelope_including_optional_context` | First preparation wins across stage families. Adding subject or correlation context to an otherwise identical prepared envelope leaves recovery checking. A matching first preparation ignores a later conflict. Reads and recovery produce no new effects. |

The middle cases belong to
`evidence_recovery_preserves_validation_scope_first_facts_and_terminal_precedence`.
Each uses a separate reader and independently specified HTTP expectations.

## Existing acceptance retained

| Existing test in `action_test.rs`                                                   | Coverage retained for later consolidation                                                         |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `protected_actions_and_read_only_views_preserve_empty_history`                      | Authorization and ordinary reads without recovery effects.                                        |
| `concurrent_registration_converges_and_rejected_start_retains_metadata`             | Independent registration, concurrency, and retained metadata after rejection.                     |
| `failed_save_blocks_new_actions_and_recovers_frozen_expired_intent_after_restart`   | Frozen event/time/revision, expired selection, restart, new-action gate, and one original effect. |
| `matching_event_identity_with_different_contents_stays_checking`                    | Conflicting same-identity contents can't confirm success.                                         |
| `real_lost_commit_acknowledgement_and_cancelled_process_recover_one_original_event` | Real COMMIT acknowledgement loss and cancellation.                                                |
| `in_flight_absent_ranking_stays_checking_until_original_action_is_recovered`        | Unlocked absence can't authorize retry. Recovery rechecks under the lock.                         |
| `five_concurrent_readers_save_with_the_production_connection_limit`                 | Connection ownership under the production limit.                                                  |
| `preference_security_and_stale_offers_require_fresh_confirmation`                   | Authorization, stale revisions, session expiry, and reader isolation.                             |
| `uncertain_preference_blocks_answers_and_restores_original_after_restart`           | Uncertain preference gate and original-choice recovery.                                           |
| `real_ranking_commit_ack_loss_and_cancellation_preserve_frozen_commands`            | Real lost acknowledgements and cancellation for ranking commands.                                 |
| `pause_resume_retains_the_unanswered_pair_after_preference_and_restart`             | Pause/Resume and retained unanswered placement.                                                   |
| `placement_actions_authorize_each_post_and_reconcile_before_expired_session_resume` | Placement authorization, uncertain recovery, and session-expiry boundaries.                       |
| `private_diagnostics_confirm_new_commits_without_replaying_activity`                | Confirmation after new commits. Replay emits no new ranking activity.                             |
| `broken_diagnostic_delivery_preserves_confirmed_outcomes_and_durable_history`       | Diagnostic delivery can't alter durable outcomes.                                                 |
| `initial_intent_ack_loss_reports_checking_before_original_action_recovery`          | Initial-intent acknowledgement loss remains recoverable.                                          |

The diagnostic replay assertion counts business records separately from authentication, read, and mutation attempts.
Typed attempt facts do not represent new saves.

Structured mutation checks cover acknowledged Add, duplicate attempts, distinct preference/Pause/Resume kinds, and expected rejection.
Known dependency failures retain their bounded cause.
An unresolved original save retains uncertainty and its original-action ID.
The existing PostgreSQL commit-fault cases also assert structured facts after lost acknowledgements.
Process termination can prevent terminal diagnostics.
Producer cancellation checks cover uncertainty when a guard can complete cleanup.

The existing HTTP, browser, package, domain, and storage suites remain part of the
repository acceptance. Controlled identity/catalog providers and local or hosted
test results don't establish live-provider behavior or deployment readiness.

## Reader-wide interpreter

Issue [#2385](https://github.com/LowkeyLab/bazel-repo/issues/2385) routes reader-wide
checking through the private `actions::evidence` module. The preceding packaged
PostgreSQL cases remain the compatibility baseline. Direct tests in
`//book_smartz/web:evidence_test` exercise the real app crate with supplied
app events and ranking documents:

- Intent order, duplicate results, payload identity, and unknown stages.
- First preparation across stage families, missing ranking evidence, and complete
  envelope equality, including retained trace context and extensions.
- Rejection exclusion before prepared-document validation, including saved actions.
- Validation of every recognized intent and rejection, including unrelated and
  later malformed facts.
- Deferred prepared-identity validation and history-validation error precedence.

These tests use no database, provider, lock, or retry adapter. Their observations
can't authorize a retry. Single-action recovery orchestration remains in
`actions.rs` for the separate follow-up #2386.
