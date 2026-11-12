# Intake acceptance evidence

Scope: issue #3152 and specification #3151, sections 2, 3, and the intake
acceptance boundary. These are controlled fixture results, not activation
approval or deployed integration evidence.

## Configuration and versions

- Base commit: `5886c059728a95fcd85ac0db19e454d0fbcff205`.
- Bazel `9.3.2`; Aspect `2026.46.3`; configured Bazel Rust `1.99.1`, edition `2024`.
- Project `PVT_kwDOCsIaD84BrIqS`; repository `LowkeyLab/bazel-repo`.
- Fixture App ID `100`, Bot actor ID `200`, installation `77`.
  These are test values, not provisioned production authority.
- Adapter read bounds: 100 pages and 100 native issue nodes.
- Successful poll delay: 60 seconds. Controller backoff fixture: two failed
  observations, 180-second cap; deterministic policy also tests three failures.
- Private receipts: real temporary directories with `0700` root and `0600` files.
- Harness `RUST_TEST_THREADS=1`; a separate process tests exclusive ownership.
- Test/build commands clear remote executor, remote cache, and build-event service
  endpoints. No live GitHub, Hermes, pool, or notification effects are sent.

## Results

The acceptance target passed **37 tests in each of five runs** with no failures.
The count includes `process_owner_probe`, the subprocess helper. The suite uses
real filesystem persistence and real independent subprocess locking; other
external interfaces are controlled fixtures.

| Requirement                             | Positive case and expected result                                                                                                                                                                                                                                                                                                                                        | Failure/recovery case and expected result                                                                                                                                                                                                                                                                                                                                                                                                           | Actual                   |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Complete 60-second intake               | `controller_polling_reports_sixty_seconds_and_bounds_failed_observations`: successful observations request a 60-second delay. `paginated_wire_adapter_selects_native_first_eligible_child_through_controller`: read all Project, actor, label, child, and blocker pages, then select the native first eligible child.                                                    | `partial_connections_and_graphql_errors_never_establish_eligibility`, `partial_history_cannot_advance_or_admit`, `partial_or_contradictory_native_reads_cannot_select`: partial data, repeated cursors, inaccessible nodes, and invalid native relationships cannot authorize selection. Failure backoff stops at its configured bound.                                                                                                             | Passed in all five runs. |
| Authorization and rechecks              | `research_authorization_does_not_override_waiting_or_hitl`: research authorization selects research without granting implementation. `admission_rechecks_blockers_without_removing_readiness`: blocking preserves implementation readiness; a fresh unblocked observation permits the same job.                                                                          | `exclusions_are_applied_before_oldest_selection`, `later_blocker_and_assignment_pages_exclude_the_first_sibling`, `recheck_after_selection_retains_slot_on_assignment_conflict`: waiting, HITL, closed, PR, ownership conflict, execution uncertainty, and missing authorization do not admit. Project PR/foreign items are excluded by the wire adapter.                                                                                           | Passed in all five runs. |
| Ordering and one active slot            | `native_order_constrains_only_eligible_siblings`, `no_eligible_work_is_idle_and_numeric_ties_do_not_use_project_order`: earlier eligible native siblings constrain selection, ineligible siblings do not, and numeric repository-qualified ties ignore Project order. `selection_wait_and_restart_retain_one_job`: waits and restart retain the original job identity.   | Blocking, ownership changes, uncertain actions, and failed notifications retain the slot rather than selecting another issue. `notification_failure_cannot_clear_stop_or_release_slot` verifies that notification failure cannot unlock dispatch.                                                                                                                                                                                                   | Passed in all five runs. |
| Exclusive ownership and missing state   | `independent_process_observes_exclusive_lock_and_stopped_owner_handoff` and `lock_and_stopped_owner_handoff_are_required`: an independent process cannot open the held lock; reopening requires a fresh process identity and trusted stopped-owner evidence. Authenticated active-job history records subsequent owner changes.                                          | `missing_or_nonprivate_receipts_are_not_new_jobs`, `receipts_have_private_modes_and_reject_symlinked_root`: absent identity/state, insecure files, and symlinked roots fail closed. Explicit initialization refuses an existing receipt root.                                                                                                                                                                                                       | Passed in all five runs. |
| App-authenticated exact history         | `publisher_authenticates_configured_app_and_scopes_installation_token`: verify the App/installation and request only repository-scoped issue authority. `uncertain_history_recovers_exact_record_without_duplicate_send`: a lost acknowledgement adopts only the original exact authenticated record.                                                                    | `readback_failure_does_not_advance_anchor_or_send_effect`, `absent_after_uncertain_history_send_never_republishes`, `damaged_history_stops_advancement`, `observed_history_damage_is_a_durable_stop_not_automatic_repair_authority`: missing/edited/forged/duplicate records and App mismatches stop; read verification failure does not advance an anchor; remote absence does not permit resend; restoring bytes does not clear the durable stop. | Passed in all five runs. |
| Intent-before-effect and exact recovery | `native_pool_hermes_and_notification_routes_share_intent_and_recovery_contract`: each native route shares authenticated intent and exact-result handling. `action_recovery_preserves_original_identity_and_exact_target`: restart preserves the action payload and identity after a lost acknowledgement, then records the exact confirmed result without sending twice. | `absent_or_wrong_effect_result_never_allows_repeat_or_another_job`, `completed_action_identity_cannot_be_reused_with_changed_payload`: absent or mismatched exact results stop dependent work, and an old action identity cannot authorize another payload. `unavailable_native_inventory_and_publication_prerequisites_fail_closed` verifies the inactive prerequisite boundary.                                                                   | Passed in all five runs. |

## Retained evidence

Private author evidence root: `/opt/data/cache/scratch/issue-3152`.
These paths are local evidence references, not public production receipts.

- `final-acceptance-results.json`: parsed expected/actual results for every test in
  each run, with exact base commit, versions, and fixture configuration.
- `final-acceptance-evidence/run_1_of_5/` through `run_5_of_5/`: copied Bazel `test.log`
  and `test.xml` outputs for each run. The XML wraps the Rust harness, so individual
  Rust test names and results are parsed from `test.log`, not inferred from XML
  wrapper counts.
- `parent-tests-final.log`: successful five-run acceptance command.
- `parent-build-final.log`: successful `aspect build //tools/software_factory/...`.
- `parent-format-final.log`: successful `aspect format --scope=all`.
- `parent-gazelle-final.log`: successful source-generation validation after formatting.
- `red-regressions.log`: two controller regressions fail before their fixes:
  changed completed-action payload reuse and non-durable history stops.
  `green-regressions-2.log` records the passing suite after those fixes.
- `poll-red.log` and `poll-green.log`: the missing controller backoff seam fails
  compilation before implementation and passes after implementation.
- `focused-clippy.log`: the isolated Clippy aspect completed successfully;
  its raw library and acceptance-test exit codes are zero. Pedantic/nursery
  advisories remain, including public API error documentation and fixture style.
- `parent-lint-final.log`: focused `aspect lint //tools/software_factory/...` passed.
  The host now has the required `patch` executable.
- `parent-vale.log`: Markdown lint reports zero errors and zero warnings.
- `review-fixes-*.log`: failing and passing recovery regression results.
  Missing nullable receipt fields, null private authority, lost pending effects,
  and edited or deleted pending records stop advancement. Restoring damaged
  history does not clear its durable stop. Transient read errors can recover.
- `parent-gates.json`, `full-build.log`, `full-lint.log`, and
  `parent-full-test-final.log`: full-repository validation remains blocked by
  Clojure dependency resolution. `baseline-clojure-build.log` reproduces the
  same failure at the unchanged base commit. These are not passing gates.

Commands for the successful targeted results:

```sh
aspect test //tools/software_factory:acceptance_test --runs_per_test=5 \
  --remote_executor= --remote_cache= --bes_backend=
aspect build //tools/software_factory/... \
  --remote_executor= --remote_cache= --bes_backend=
bazel build //tools/software_factory/... \
  --aspects=//tools/lint:linters.bzl%clippy --output_groups=rules_lint_human \
  --remote_executor= --remote_cache= --bes_backend=
```

## Hosted review corrections

The corrected acceptance target passed **43 tests**, including six new transport
and bounded-history tests. The original five-run, 37-test evidence above remains
historical evidence, not a claim that the expanded suite ran five times.

- `app_jwt_transport_sends_bearer_only_through_private_stdin` checks explicit
  Bearer headers for App GET and token-minting POST requests, stdin-only secret
  transfer, exact JSON input, and withheld failed-command diagnostics.
- `gh_read_and_installation_transports_keep_token_auth_without_secret_arguments`
  checks the unchanged read and installation token modes at the command boundary.
- `app_jwt_transport_rejects_missing_credentials_and_unbounded_requests_before_spawn`
  checks missing credentials, relative executables, foreign endpoints, traversal,
  and unsupported automatic pagination.
- `history_rejects_an_over_bound_count_before_fetching_any_pages` checks that a
  201-comment history with a two-page allowance makes no REST history request.
- `history_fetches_only_explicit_bounded_pages_without_transport_autopagination`
  checks empty, single-comment, full-page, and two-page histories, including
  exactly full final pages. Every REST request has an explicit page and disables
  transport pagination.
- `history_page_failures_never_return_partial_or_unproven_history` checks failed,
  malformed, duplicate, oversized, changing, and incomplete page responses.

Evidence under the same private root:

- `codex-cycle-1-auth-red.log`: missing App JWT transport fails compilation before
  implementation. `codex-cycle-1-bound-red-actual.log` and
  `codex-cycle-1-pages-red.log`: the original history behavior fails the new
  request-boundary assertions. Early commands that used `--test_filter` did not
  run the intended Rust tests; the replacement RED commands used `--test_arg`.
- `codex-cycle-1-final-tests.log`: all 43 tests passed, with no filtered tests.
- `codex-cycle-1-final-build.log`, `codex-cycle-1-final-lint.log`, and
  `codex-cycle-1-format-local.log`: focused build, lint, and whole-tree formatting
  completed successfully. Existing Clippy advisories and toolchain warnings
  remain. An initial formatter invocation timed out; the local-only retry passed.
- `codex-cycle-1-curl-protocol.log`: installed curl sent the exact Bearer header
  and JSON body from stdin configuration to a local HTTP fixture. This checks
  curl's configuration parsing, not a live GitHub App or production credentials.

No commit, push, review request, or GitHub mutation was made for these corrections.

## Remaining readiness prerequisites

No production App identity, installation permissions, private Project access,
controller/worker OS isolation, stopped-owner supervisor, native execution
inventory, native worker admission, pool receipt integration, or notification
routing is certified by these fixtures. Missing evidence blocks its affected
operation through the `Unprovisioned` interfaces. Later tickets own those native
integrations, reviews, merge delivery, and final readiness evidence.

No service or scheduled task was installed, no activation was requested, and no live
external fixture was used. Full-repository test/build/lint was attempted but
remains blocked by the independently reproduced base-commit failure. Initial
independent Standards and Spec reviews found one import-order violation and two
recovery defects. The corrected implementation includes six regression tests. A further lost-acknowledgement case retains the
discovered comment identity before retrying an exact read. Its failing-before-fix
result is in `parent-discovered-red.log`.
