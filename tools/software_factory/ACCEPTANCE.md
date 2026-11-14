# Software factory acceptance evidence

## AFK research evidence for issue #3154

These results use controlled controller and native-adapter fixtures. They do not
prove deployed integration or authorize activation or live research.

### Configuration and validation boundary

- Assigned source base: `b1a060025608a529ed8014644c2d688ea23283bc`.
- Bazel `9.3.2`, Aspect `2026.46.3`, Bazel-managed Rust `1.99.1`, edition `2024`.
- The current base cannot resolve its dependency graph. The initial attempt
  failed to apply `tools/lint/python_typing.patch` to the pinned `rules_lint`.
  A scratch override of that dependency exposed a second existing failure:
  `aspect_rules_js` declares module name `rules_js`.
- All current-base Gazelle, formatting, full-test, full-build, and lint attempts
  remain blocked before the changed Rust source is analyzed. No repository
  dependency, patch, lint rule, or validation threshold was changed for this ticket.
- To exercise the implementation, a separate scratch validation tree uses the
  repository archive at `59c7445cbdc9bff4ca6d776708e7340005f0235a` for dependency
  configuration. Its `tools/software_factory` and `tools/pool_receipt` paths are
  symlinks to the actual assigned checkout. This validates the changed source,
  not the current base's broken dependency configuration. It is not a current-base pass.
- Fixture identity: gateway `gateway-1`, profile `worker`, principal `worker-owner`,
  epoch `7`, research admission `admission-1` with generation `4`, and separate
  handoff admission `handoff-1` with generation `5`. Required research skills are
  `research` and `ask-matt`. The trusted fixture manifest is independent of caller
  omissions. Existing App, original pool, private-store, and isolation fixtures
  remain in use. No live assignment is acquired or released by these tests.
- Local validation clears remote execution, cache, and build-event endpoints;
  it uses four jobs, `GOMAXPROCS=2`, and JVM `ActiveProcessorCount=4`.
- Nix and the required `asd-ste100` skill are unavailable. There is no Nix-shell
  or strict-English verification claim.

### Expected and actual results

The alternate dependency configuration passed each focused target in three
uncached runs: **50 worker tests**, **44 intake tests**, and **20 pool-wrapper
command tests** per run. All **342 individual executions passed**, with no
failures or ignored tests. Counts come from parsed Rust harness logs.
The package build passed. All three package Clippy reports are empty, with
explicit report exit code `0`. The Bazel-managed Rust formatter passed.
Current-base whole-repository gates remain blocked as stated above.

| Requirement                   | Expected result                                                                                                                                                         | Actual result                                                                                                                    |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Authorized native research    | Load research and ask-matt explicitly, retain original pool/run/session identities, and submit no implementation workflow.                                              | Passed through controller fixtures and the real native adapter with controlled RPC responses.                                    |
| Findings and completion       | Publish findings on the exact originating issue, verify App/payload/continuity, publish exact comment/digest confirmation, then enter ResearchComplete without a PR.    | Passed; repeat calls create no second findings record or native admission.                                                       |
| Interrupted publication       | Reconcile the original findings or confirmation comment after restart, including waiting labels. Absence after uncertain send never permits republishing or completion. | Passed for lost findings acknowledgement, lost confirmation acknowledgement, and temporarily absent exact findings.              |
| Required skills               | Missing caller-manifest skills, missing resolved research content, and partial actual frozen skills stop before submission.                                             | Passed; restoring a missing prerequisite permits the original selected job, not a substitute workflow.                           |
| Waiting and decision labels   | Waiting labels and decision-type labels suppress admission even with ready-for-agent.                                                                                   | Passed for needs-info, needs-triage, ready-for-human, blocked, waiting, and wayfinder:grilling.                                  |
| Real human state              | Publish the unanswered question, checkpoint HumanInputRequired, invoke preloaded ask-matt with a separate durable submission, and accept no worker-generated answer.    | Passed across restart, lost handoff acknowledgement, unknown exact effect, waiting labels, and changed subsequent worker output. |
| Implementation boundary       | Reject implementation worker substitution, generic PR publication, merge actions, arbitrary replacement prompts, and implementation or fabricated-answer output fields. | Passed. Rejected terminal output remains retained after restart; changed worker output cannot repair authority.                  |
| Native recovery and cessation | Lost create/submit acknowledgements retain the original run and pool receipt. Cancellation of the handoff targets its distinct exact admission and generation.          | Passed without resending an uncertain native mutation.                                                                           |
| Output prerequisites          | Empty findings, absent sources, and empty human questions cannot publish or complete.                                                                                   | Passed; no public findings record is emitted.                                                                                    |

The behavioral red runs are retained in `red-admission.log`, `red-findings.log`,
`red-human-authority.log`, and `red-handoff-identity.log`. They failed before their
corresponding fixes. Subsequent `green-*.log` files retain slice results.

### Evidence files and remaining gates

Private evidence root: `/opt/data/cache/scratch/issue-3154`.

- `final-results.json`: parsed test names/counts, configuration, source SHA-256
  bindings, validation boundary, and actual Clippy report results.
- `testlogs/`: the nine final Rust harness logs and Bazel XML wrappers.
- `lint-reports/`: the three actual Clippy reports and exit-code files.
- `validate-focused.sh` and `collect-evidence.py`: reproducible validation and
  evidence collection commands. `final-mirror-status.log`, `final-mirror-test.log`,
  `final-mirror-build.log`, `final-mirror-lint.log`, and
  `final-mirror-rust-format.log` retain their output.
- `baseline.log`, `baseline-local.log`, and `baseline-override.log`: current-base
  dependency failures before implementation. `final-format.log`, `final-test.log`,
  `final-build.log`, `final-lint.log`, and `gazelle-*.log` retain current-base gate
  attempts. These are failures, not successful acceptance evidence.

The independent Standards and Spec review remains the parent delivery workflow's
responsibility. Deployed host prerequisites, native capability/frozen-skill proof,
App permissions, real human delivery, and the final readiness activation gate
remain unresolved runtime prerequisites. No live worker, research fixture,
GitHub publication, pool recovery, PR, merge, or activation was performed.

## Native worker evidence for issue #3153

Fixture results do not establish production readiness or activation authority.
The controller tests use real private filesystem persistence and authenticated
history fixtures. Native and GitHub adapters use controlled protocol responses.
Command tests execute temporary fixture programs, never live workers or pool commands.

### Configuration

- Base commit: `0a7ca8f51a9353f973f46bb6b1d151837075ce9d`.
- Bazel `9.3.2`, Aspect `2026.46.3`, and Rust edition `2024`.
- Native fixture profile `worker`, principal `worker-owner`, gateway `gateway-1`,
  authority epoch `7`, execution generation `4`, session `session-1`, and admission `admission-1`.
- The fixture reads the real controller user ID from `/proc/self` and uses worker user ID
  `10001` only as trusted-supervisor fixture evidence. It does not provision that OS user.
- Mandatory fixture skills: `implement`, `tdd`, and `code-review`. Production must
  discover the complete repository/workflow manifest, including additional mandatory skills.
- Original pool captures use private temporary files and fixed fixture assignment,
  acquisition, repository, and worktree identities. Native `cwd` uses `/w`.
  The lossless-path negative case contains byte `255` and permits no alternate checkout.
- Publication fixture uses branch `feat/worker-2`, exact head
  `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`, base `main`, and PR number `8`.
  The GitHub adapter fixture has a three-page read bound.
- Tests and builds clear remote execution, remote cache, and build-event endpoints.
  Local build concurrency uses `--jobs=4` and `--action_env=GOMAXPROCS=2`.
- Whole-tree formatting uses Bazel-managed Java 21. Gazelle uses the default
  Bazel-managed Java toolchain. Forcing Java 21 on Gazelle fails its Java 25
  manifest-stamping tool. Java 25 cannot run the configured Java formatter.
  No toolchain installation or source workaround was necessary.

### Expected and actual results

`//tools/software_factory:worker_test` passed **23 tests in each of three runs**.
The existing `acceptance_test` target passed **44 tests in each of three runs**.
The six executions passed **201 individual test executions**, with no failures or
ignored tests. Counts come from parsed Rust harness logs, not the Bazel XML wrapper count.

| Requirement                 | Positive result expected                                                                                                                                                                                         | Failure/recovery result expected                                                                                                                                                                               | Actual                                                             |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Native contract and skills  | `controller_native_adapter_uses_only_flat_supported_rpc_and_explicit_skill_preload` creates and submits with frozen skills and the issue URL as a separate argument.                                             | Missing parameters, omitted host-manifest skills, missing resolved skills, and partial actual frozen content stop admission or submission. Repeated observation creates no second session.                     | Passed in all three runs.                                          |
| Original pool receipt       | `pool_adapter_retains_acquisition_capture_and_only_inspects_exact_assignment` retains the original capture and requests only exact assignment inspection.                                                        | Pending/unknown/rejected captures, wrapper errors, and non-UTF-8 gateway `cwd` stop dependent work without acquisition or fallback.                                                                            | Passed in all three runs.                                          |
| Durable admission recovery  | `lost_create_acknowledgement_restarts_exact_worker_without_resend` retains the same run and create identity.                                                                                                     | `lost_submit_ack_and_unknown_result_never_repeat_or_release_the_slot` retains the exact submission across restart and refuses to repeat an unknown effect.                                                     | Passed in all three runs.                                          |
| Isolation and credentials   | Valid trusted-host fixture evidence permits the deterministic workflow.                                                                                                                                          | `isolation_and_credential_attestations_cannot_be_missing_or_reuse_controller_uid` rejects controller/root worker identities and absent isolation/credential evidence. `UnprovisionedHost` sends no create.     | Passed in all three runs. Actual OS separation remains unverified. |
| Authenticated private state | `one_worker_uses_frozen_skills_authenticated_intents_and_retains_slot` checks verified issue intents before fixture sends.                                                                                       | `private_worker_receipt_cannot_disagree_with_authenticated_history` rejects changed private worker identity. Immediate terminal results remain private and do not enter public issue history.                  | Passed in all three runs.                                          |
| Review and CI slot          | Successful implementation retains the issue slot. Separate publication moves the same worker to `ReviewCiWait`.                                                                                                  | Early publication fails. Lost publication acknowledgement reconciles the exact PR without another send. Existing notification routes remain separate from native worker sends.                                 | Passed in all three runs.                                          |
| Internal review boundary    | Trusted synchronous gate fixture reports clear execution before observation.                                                                                                                                     | `internal_review_failure_is_a_durable_repair_stop_not_an_automatic_loop` retains `RepairBlocked` across restart, even after the fixture clears the failure signal. Missing mandatory gate stops before create. | Passed in all three runs. No stock-gateway gate claim.             |
| Cancellation and cessation  | Exact stopped/effects evidence advances `Cancelling` to `Stopped` while retaining unfinished work and the slot.                                                                                                  | Cancellation acknowledgement alone, or cessation without reconciled effects, does not advance. Queued cancellation needs no invented execution generation or result.                                           | Passed in all three runs.                                          |
| Protocol adapters           | Canonical admission parsing preserves epoch, generation, and terminal result. The command bridge sends one exact packet. GitHub publication reads the exact remote marker, branch, head, base, number, and node. | Malformed native identity and wrong JSON-RPC reply identity fail. An existing publication marker prevents another post.                                                                                        | Passed in all three runs.                                          |

The initial worker API tests failed before implementation. Behavioral red tests
also exposed private-history disagreement, caller omission of mandatory skills,
terminal-result disclosure, and refusal of valid never-started cancellation.
The retained `red-*.log` files record those failures before their fixes.

### Retained native-worker evidence

Private author evidence root: `/opt/data/cache/scratch/issue-3153`.

- `final-results.json` contains parsed per-run status, exact test names, and counts.
- `testlogs/` preserves the six harness logs and six Bazel XML wrappers.
- `final-format.log`, `final-gazelle.log`, `final-test.log`, `final-build.log`,
  and `final-lint.log` retain the actual gate output.
- `red-*.log` and `green-*.log` retain test-first slice evidence.
- The downloaded official gateway document is
  `/opt/data/cache/scratch/gateway-sessions-3153.html`, with `SHA-256` digest
  `1b0fd32b5ca466d089031d6e87f0958a203437cdd6f2d10256ae3715c9a92083`.
  Native source inspection establishes wire shape only, not deployed version compatibility.

The focused package build, whole-tree formatting, Gazelle, and strict package lint
completed successfully. The parent repeated both test targets without cached
results after correcting the independent Standards review's import-order finding.
The independent Spec review reported no findings. Admission status strings and
separate receipt validation remain wire-boundary design choices; the review's
suggestions for typed statuses and shared receipt reads are non-blocking refactoring
opportunities, not waived safety findings. The unavailable `asd-ste100` skill
provides no strict-mode compliance evidence.

Full-repository validation is not passing. The test attempt reported missing
`/var/run/docker.sock` in unrelated database tests, process-fixture failures, and
a timeout, followed by Bazel native-thread exhaustion. Capping the Bazel JVM with
`--host_jvm_args=-XX:ActiveProcessorCount=4` permitted subsequent focused gates.
The full build retry and full lint stop on Clojure dependency resolution:
`clojure-1.12.6.pom.part.lock` is absent. The same dependency failure reproduced
in the untouched planning checkout at `5886c059728a95fcd85ac0db19e454d0fbcff205`.
No unrelated tests, dependencies, or checks were weakened.

Parent evidence in the same private root: `parent-gates.json`,
`parent-full-test.log`, `parent-full-build-retry.log`, `parent-full-lint.log`,
`baseline-clojure.log`, `parent-review-gazelle-retry.log`,
`parent-review-format-retry.log`, `parent-review-tests.log`, and
`parent-focused-lint.log`. The initial formatter retry stopped on Git's
intent-to-add index state; staging the reviewed new files allowed the unchanged
whole-tree formatter to pass.

### Hosted review corrections for issue #3153

Hosted review of `59c7445cbdc9bff4ca6d776708e7340005f0235a` found three defects.
Regression tests failed before correction and now pass:

- The real pool wrapper could not inspect without a receipt root after the
  adapter cleared its environment. The adapter now takes explicit approved
  private receipt and catalog directories, preserves the original acquisition,
  and rejects another catalog identity. The test executes the actual
  `pool_receipt` binary with a controlled pool child, not a substitute wrapper.
- Retained-worker observation incorrectly required absence of its own execution.
  Exact post-admission observation now retains the same job/session/admission
  and all other authorization checks without permitting create or submit.
  New admission and PR publication still need execution clearance.
- Submission readback accepted an execution-generation change. It now preserves
  a known generation and authority epoch; only queued work without a generation
  can acquire its first generation.

The expanded suite passes **35 worker tests**, **44 intake tests**, and
**20 pool-wrapper command tests**. Additional native adapter tests cover exact
cancellation, cessation/effects separation, real publication routing, original
reply recovery, and unprovisioned-host refusal. Local Bazel coverage measures
native adapter line coverage at **94.27%**. This is not hosted patch coverage.
The initial hosted patch result was **90.44%** against a **91.50%** target;
updated hosted results remain a separate gate. No threshold or exclusion changed.

Evidence: `cycle-1-controller-red.log`, `cycle-1-controller-green.log`,
`cycle-1-parent-tests.log`, `cycle-1-parent-lint.log`,
`coverage-additional-tests.log`, and `coverage-additional-tests.lcov` under the
private evidence root. Adapter red logs are
`/opt/data/cache/scratch/adapter-generation-red.log` and
`/opt/data/cache/scratch/adapter-wrapper-red.log`. The downloaded initial hosted
coverage evidence is under `ci-coverage-original/` in the private evidence root.

### Runtime blocks

The native stock contract does not demonstrate synchronous review-before-repair
interception, complete actual frozen-skill proof, or exact lost-reply lookup for
all required interruption cases. `UnprovisionedHost` rejects admission until the
operator supplies reviewed supported interfaces and bound evidence. An explicit
supported authenticated native bridge, approved OS/filesystem identities,
controller/worker credential separation, and original pool compatibility also
remain production prerequisites. No tests in this task activate workers, post
live PRs, change pool state, or authorize normal operation. The final readiness
ticket retains the single activation gate.

## Retained intake evidence for issue #3152

Scope: issue #3152 and specification #3151, sections 2, 3, and the intake
acceptance boundary. Controlled fixtures provide these results, not activation
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
  endpoints. Tests send no live GitHub, Hermes, pool, or notification effects.

## Results

The acceptance target passed **37 tests in each of five runs** with no failures.
The count includes `process_owner_probe`, the subprocess helper. The suite uses
real filesystem persistence and real independent subprocess locking. Controlled
fixtures replace the other external interfaces.

| Requirement                             | Positive case and expected result                                                                                                                                                                                                                                                                                                                                        | Failure/recovery case and expected result                                                                                                                                                                                                                                                                                                                                                                                                           | Actual                   |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Complete 60-second intake               | `controller_polling_reports_sixty_seconds_and_bounds_failed_observations`: successful observations request a 60-second delay. `paginated_wire_adapter_selects_native_first_eligible_child_through_controller`: read all Project, actor, label, child, and blocker pages, then select the native first eligible child.                                                    | `partial_connections_and_graphql_errors_never_establish_eligibility`, `partial_history_cannot_advance_or_admit`, `partial_or_contradictory_native_reads_cannot_select`: partial data, repeated cursors, inaccessible nodes, and invalid native relationships cannot authorize selection. Failure backoff stops at its configured bound.                                                                                                             | Passed in all five runs. |
| Authorization and rechecks              | `research_authorization_does_not_override_waiting_or_hitl`: research authorization selects research without granting implementation. `admission_rechecks_blockers_without_removing_readiness`: blocking preserves implementation readiness; a fresh unblocked observation permits the same job.                                                                          | `exclusions_are_applied_before_oldest_selection`, `later_blocker_and_assignment_pages_exclude_the_first_sibling`, `recheck_after_selection_retains_slot_on_assignment_conflict`: waiting, human-in-the-loop decisions, closed, PR, ownership conflict, execution uncertainty, and missing authorization do not admit. The wire adapter excludes Project PR and foreign items.                                                                       | Passed in all five runs. |
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
  and `test.xml` outputs for each run. The XML wraps the Rust harness, so the result collector parses individual
  Rust test names and results from `test.log`, not XML wrapper counts.
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
and bounded-history tests. The original five-run, 37-test evidence in the preceding section remains
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
  201-comment history with a two-page allowance makes no HTTP history request.
- `history_fetches_only_explicit_bounded_pages_without_transport_autopagination`
  checks empty, single-comment, full-page, and two-page histories, including
  exactly full final pages. Every HTTP request has an explicit page and disables
  transport pagination.
- `history_page_failures_never_return_partial_or_unproven_history` checks failed,
  malformed, duplicate, oversized, changing, and incomplete page responses.

Evidence under the same private root:

- `codex-cycle-1-auth-red.log`: missing App token transport fails compilation before
  implementation. `codex-cycle-1-bound-red-actual.log` and
  `codex-cycle-1-pages-red.log`: the original history behavior fails the new
  request-boundary assertions. Early commands that used `--test_filter` did not
  run the intended Rust tests; the replacement failing-test commands used `--test_arg`.
- `codex-cycle-1-final-tests.log`: all 43 tests passed, with no filtered tests.
- `codex-cycle-1-final-build.log`, `codex-cycle-1-final-lint.log`, and
  `codex-cycle-1-format-local.log`: focused build, lint, and whole-tree formatting
  completed successfully. Existing Clippy advisories and toolchain warnings
  remain. An initial formatter invocation timed out; the local-only retry passed.
- `codex-cycle-1-curl-protocol.log`: installed curl sent the exact Bearer header
  and JSON body from standard input configuration to a local HTTP fixture. This checks
  curl's configuration parsing, not a live GitHub App or production credentials.

These fixture corrections make no live App calls. Subsequent delivery committed
and pushed them for hosted review.

## Lint corrections

The expanded acceptance target passed **44 tests without cached results**, including a known
`SHA-256` digest example. The focused build and formatter also passed.
The latest raw Clippy diagnostics contain no warnings or errors for the library
or acceptance target. Vale reports no errors, warnings, or suggestions.

Public interfaces now document their errors and invariant-only panics. Mechanical
simplifications preserve the existing safety decisions. `Controller::perform`
borrows its action. Two narrowly scoped test-fixture exceptions retain independent
fault switches and a complete wire-protocol dispatch without changing production
lint policy.

Evidence: `lint-cleanup-parent-verified.json`, `lint-cleanup-parent-tests.log`,
`lint-cleanup-parent-build.log`, `lint-cleanup-parent-format.log`, and
`lint-cleanup-parent-lint.log` under the private evidence root. These results
supersede the earlier package lint advisories, not the runtime prerequisites.

## Remaining readiness prerequisites

These fixtures do not certify production App identity, installation permissions,
private Project access, controller/worker OS isolation, stopped-owner supervision,
native execution inventory, native worker admission, pool receipt integration, or
notification routing. Missing evidence blocks its affected
operation through the `Unprovisioned` interfaces. Later tickets own those native
integrations, reviews, merge delivery, and final readiness evidence.

Delivery installed no service or scheduled task, requested no activation, and used
no live external fixture. Local full-repository test/build/lint attempts remain
blocked by the independently reproduced base-commit failure. Initial
independent Standards and Spec reviews found one import-order violation and two
recovery defects. The corrected implementation includes six regression tests. A further lost-acknowledgement case retains the
discovered comment identity before retrying an exact read. Its failing-before-fix
result is in `parent-discovered-red.log`.
