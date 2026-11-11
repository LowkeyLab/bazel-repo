# Repository dispatch, hosted review, and merge integration

Research for [#3137](https://github.com/LowkeyLab/bazel-repo/issues/3137), under [map #3135](https://github.com/LowkeyLab/bazel-repo/issues/3135). Read-only inspection on November 11, 2026.

**Source commit:** `65ab6e77b5d745b67bd882f32b9a915a300cb3f3`. All repository links below are pinned to this commit. Issue comments and live API observations are mutable evidence, not commit-pinned contracts.

## Outcome

Reuse the existing GitHub issue protocol and four Rust command interfaces. They cover publication, pool receipts, hosted-review requests/wakes, and merge verification. They do **not** form a complete dispatcher. The missing integration owns job/run/worker identity, approval and selection, durable execution/repair accounting, independent review provenance, expected-head merge execution, and recovery after uncertain effects.

**Rollout blocker:** the map requires human-controlled deployment, but the checked-in Deploy workflow automatically follows successful main-branch Bazel Tests and runs OCI delivery. Resolve that conflict before enabling automatic implementation merges; do not merely promise that the dispatcher will not call deployment. [Deployment source][deploy]

## 1. Eligibility, ownership, and native graph

`ready-for-agent` means fully specified, not currently unblocked. The publisher deliberately labels fully specified tickets even with open blockers. Selection must independently establish an open issue, every blocker closed, correct developer ownership, and complete dependency reads. Exclude PRs from issue enumeration. Limited issue lists are previews, not exhaustive discovery. [Tracker][tracker] [Labels][labels] [Publication contract][publication]

The tracker distinguishes:

- **Assignment:** developer ownership, retained across sequential sessions; no session claim is required merely to continue assigned sequential work.
- **Dispatch:** explicit ticket-to-worker-session mapping during authorized parallel coordination. The coordinator serializes selection and records coordinator/worker identifiers in a ticket comment before dispatching exactly one worker. Fresh eligibility and assignment reads precede dispatch.
- **Recovery:** uncertain worker status pauses dispatch. Replacement needs confirmation the old worker stopped. Coordinator replacement requires maintainer authorization and reconstruction from comments, not inference from elapsed time. A return to sequential operation also needs a documented handoff. [Tracker §§ Selecting and claiming work–Recovery][tracker]

For maps, resume eligible retained assignments before selecting unassigned children, in native sub-issue order. Read every page of `/issues/{map}/sub_issues` and `/issues/{child}/dependencies/blocked_by`; incomplete data prevents starting. Generic tracker guidance permits explicit body-reference fallback when native links are unavailable, but the publication specification requires native links and stops rather than substituting body references. Map #3135 also requires native links: the dispatcher must not silently relax that requirement. [Tracker][tracker] [Approved publication spec #2401](https://github.com/LowkeyLab/bazel-repo/issues/2401)

**Live observation:** `gh issue view` read the complete bodies/comments for #3137 and #3135. #3137 was open, assigned to `tacascer`, and its paginated native blocker endpoint returned `[]`. Its explicit worker dispatch comment identifies this research session and limits authority to research. The map's children were #3136–#3141. These observations authorize neither dispatcher activation nor implementation merging.

**Gap:** no examined tool provides a general eligibility/claim/dispatch command. Reuse the protocol and checked identity patterns, not a readiness-label-only queue. Map #3135 adds approved implementation issues and AFK research intake; the approval representation, cross-map ordering, AFK/HITL classification, and single-worker enforcement still require decisions (#3138/#3141). Do not treat a decision issue as implementation work.

## 2. Reusable interfaces and durable state

### Publication: `//tools/ticket_publication`

Approved spec #2401 owns the publication plan/batch terms. `prepare`, `status`, `publish`, `export`, and explicit `recover` commands retain original batch identity, plan fingerprint, markers, and separate issue number/numeric ID/node ID. Native graph verification precedes labels; fresh final reads establish completion. `status` reads remote state without writes or saved-progress updates. The fingerprint is **not** an approval signature. [README][publication]

Useful implementation seams include `github::Client::relationships`, `parent`, `graph::observe`, and `labels::observe`. Relationship reads retain repository-qualified identities, reject duplicates/PRs, and paginate; mutations use numeric IDs and `replace_parent: false`. These are source-level seams inside a binary, not a promised public dispatcher library. Prefer the JSON CLI where its publication scope fits. [GitHub adapter][publication-api] [Graph][graph] [Labels implementation][publication-labels]

Local locks cannot coordinate copied state across machines. Stop the previous publisher before transfer. Uncertain creation with no matching marker remains uncertain; another create needs explicit duplicate-risk authorization. Publication is not execution ownership or a job ledger. [README][publication]

### Workspace: `//tools/pool_receipt`

Accepted ADR 0001 makes `LowkeyLab/worktree-pool` authoritative after cutover; it supersedes the older Bazel-packaging direction in worktree spec #2064. Do not rebuild the removed monorepo pool. The wrapper executes **one explicitly supplied** `worktree-pool --json` command and preserves raw arguments/output/status before interpretation. [ADR][pool-adr] [Approved historical spec #2064](https://github.com/LowkeyLab/bazel-repo/issues/2064) [Wrapper][pool]

Retain task, assignment handle, acquisition operation, repository/worktree identities, and original acquisition receipt. `command_identity.operation_id` can identify the present command rather than the acquisition. Read lossless path bytes from `selected-assignment.json`; display paths are not filesystem authority. A successful inspection does not transfer ownership. [Wrapper][pool]

Outcomes remain `completed/0`, `rejected/2`, `pending/3`, `unknown/4`. Capture failure can follow a completed mutation. Stop effects on pending/unknown; inspect exact recorded identities. Never choose the newest assignment, reacquire blindly, expand capacity, infer abandonment, or recover automatically. Exact recovery action/identity still needs human approval. Stop active writers and preserve unfinished work before release; use the retained built wrapper after shutting down Bazel when releasing its own checkout. [Wrapper][pool] [Production command][pool-main]

**Gap:** the standalone pool contract/installed version and inherited host guidance were not independently verified here. The wrapper's tests use a fake pool. Job concurrency of one is separate from pool capacity and does not justify changing that capacity.

### Hosted review: `//tools/hosted_review`

Use `request`, `track`, `poll`, and single-observation `wake`, with the same private state root across resumes. A per-repository/PR lock, synchronized intent before send, UUID marker, authenticated caller, comment ID, and server timestamp protect request identity. An uncertain send stays uncertain until its exact comment is reconciled; an absent match is not retry authority. External requests stop automatic retries. [README][hosted] [Store][hosted-store]

Retry requires an authenticated incomplete review for the exact current head and **more than** 3,600 seconds since a confirmed request. Exactly 3,600 is insufficient. First requests are explicit; `poll` cannot bootstrap an untracked head. PR/provider evidence is reread before send, but GitHub comment creation cannot atomically condition on head identity. [Policy][hosted-policy] [README][hosted]

The adapter requires bot ID `199175422`, login `chatgpt-codex-connector[bot]`, type `Bot`, and `codex-review-state:v1` with exact `code_review.commit_sha`. Completion alone does not establish no findings. Reactions/human text are not substitutes. [Adapter][hosted-api]

`wake` retains the original acquisition receipt and automation identity; it does not inspect or release ownership. It deduplicates notification transitions across restarts. Persist-before-return can lose notification delivery after a crash. `external_work_complete` is not merge acceptance or proof of complete required-CI coverage. [Wake implementation][wake] [README][hosted]

## 3. Exact-head acceptance and merge

`//tools/merge_verification` is the maintained **read-only** gate, not the merger. It supports this repository, `main`, and squash delivery. Inputs include full reviewed SHA, complete reviewed paths (both rename paths/removals), and `{head, standards_findings:0, spec_findings:0}`. Review counts are caller assertions: the verifier neither runs Matt Pocock `/code-review` nor proves reviewer independence. The integration must retain the actual independent Standards and Spec outputs and their exact diff/spec/head identities. [Inputs][merge-main] [README][merge]

Before acceptance checks:

- Exact open head, confirmed mergeability, complete expected paths, and a final unchanged head/base observation.
- Published checks for that head and latest commit statuses; fixed Actions `test`, `lint`, `format`; additional current ruleset/classic-protection required contexts and app identities.
- `Bazel Tests` and `Coverage` workflow presence; app-bound `codecov/patch` for non-Markdown changes. All-Markdown scope permits absence, never published failure.
- Latest authenticated exact-head Codex marker completed with **both** findings flags false; no active change request; every review thread resolved. Failed, malformed, incomplete, changing-count, or truncated reads reject acceptance. [Production policy][merge-policy] [Transport][merge-api]

**Preserve the actual policy:** completed checks/workflows accept `success`, `neutral`, or `skipped`; statuses require `success`. The verifier rejects any observed nonaccepted check/workflow, not only required names. Workflow presence is matched by name and head. Do not relabel this as “every required check executed successfully.” Deciding whether strict execution, stronger workflow identity, rerun handling, or broader branch-rule coverage is needed belongs to #3139; do not weaken current gates. [Policy lines 37–43, 161–256][merge-policy]

The checked-in CI inventory explains those names: `run-tests.yml` runs `aspect format`, `aspect lint //...`, and `aspect test //...` after Nix-tool preparation; `coverage.yml` reconciles coverage and uploads with `fail_ci_if_error: true`. Path/label-dependent acceptance workflows and reusable Nix preparation also exist, so static workflow names alone cannot establish the live required inventory. Copilot setup workflows are not hosted Codex acceptance. Local implementation validation remains separate: immediate Gazelle after source edits, full-scope formatting, focused tests, local serving checks, full build, and lint in the Nix shell. A green hosted snapshot does not replace these obligations. [Tests workflow][tests-workflow] [Coverage workflow][coverage-workflow] [Development][development]

**Live branch observation:** current effective rules required Actions `test` and `lint`, linear history, signatures, and squash-only pull requests; code-owner review was enabled with required approving count zero. Classic required-status protection returned the explicit `404`/`Branch not protected` result accepted by the adapter. These are mutable observations, not the full verifier inventory or proof of token merge permission. The verifier also requires format, coverage, and Codex independently of server-enforced rules. [Rules endpoint](https://api.github.com/repos/LowkeyLab/bazel-repo/rules/branches/main) [Transport exception][merge-api]

**Recommended integration sequence, not implementation authority:** retain validation and independent reviews for current head → hosted completion → successful `before` receipt → authorized squash merge with expected-head condition → read remote outcome → `after` using the same head/path/source-review inputs and successful before receipt → reconcile issue/result → preserve work and release only when safe. A new head invalidates old acceptance. Do not use `--admin` or enable native auto-merge early on the assumption that branch protection encodes these extra gates. `gh pr merge --match-head-commit` provides the expected-head option; the before snapshot cannot make all later external state atomic. [Merge contract][merge] [Official CLI manual](https://cli.github.com/manual/gh_pr_merge)

After acceptance compares reviewed blob SHA/type/mode at each path against the actual single-parent squash commit. Deletions remain absent; unrelated advanced-base changes are allowed; the squash cannot introduce changes outside reviewed scope. Whole-tree equality is wrong. A lost merge response needs observation, not blind repeat; a merged PR awaiting successful `after` must remain a distinct unfinished job state. The after phase verifies content, not renewed pre-merge review/CI acceptance. [Policy][merge-policy]

## 4. Authority, remaining integration, and evidence limits

Existing automation is not universally human-only: `update-multitool.yml` enables `gh pr merge --auto --squash` for its dependency PR; Renovate also declares automerge policies. These scoped paths are neither authority to merge unrelated PRs nor substitutes for map #3135's stronger review gates. The map expressly requires automatic implementation merge after all gates, while this research does not activate it. [Multitool][multitool] [Renovate][renovate]

**Decisions still needed:** durable job/run/worker correlation and one active worker; idempotent start/PR/merge/escalation intents; restart reconciliation; approval and HITL boundaries; trustworthy review records; and one shared **three-repair-cycle** ledger across source review, hosted review, and CI. None of the examined helper state models supplies that job ledger. Hosted timeout retries are request management, not an established repair-budget definition. New commits/restarts must not reset the budget. On exhaustion, persist a terminal automatic-stop state and issue findings/attempt history, then invoke `/ask-matt` for existing implementation work or `/triage` for incoming reassessment, with real HITL preserved. Define ambiguous attempts, infra failures, and escalation delivery recovery before implementation. [Map #3135](https://github.com/LowkeyLab/bazel-repo/issues/3135) [Hosted state][hosted-store]

**Evidence levels:** inspected production policy and test source, but ran no builds/tests/validation. Command tests use controlled `gh`/pool processes and real private files; they prove intended local contracts only when executed, not live availability. Existing completion comments report historical live before/after success (#2777), installed-pool compatibility (#2778), and bounded hosted observations (#2775). Their private raw receipts were not available here. [Merge tests][merge-tests] [Historical merge evidence](https://github.com/LowkeyLab/bazel-repo/issues/2777#issuecomment-6470774045) [Pool evidence](https://github.com/LowkeyLab/bazel-repo/issues/2778)

A fresh read of [PR #2819's bot comment](https://github.com/LowkeyLab/bazel-repo/pull/2819#issuecomment-6470344937) confirmed the adapter identity/marker, exact historical head `ee68713e1419ddb686dfcd8bdaa5cd88668a14d8`, completed status, and false findings flags. This proves one retained provider response shape, not a new review, timeout/retry behavior, provider SLA, or future compatibility. No live reviews, dispatches, merges, releases, or deployment actions occurred. The Deploy workflow has no visible human environment gate; downstream infrastructure controls remain unverified.

Loaded `/research` and read the required agent/domain/glossary guidance. No dispatcher glossary context exists in the root map. `asd-ste100` was unavailable; plain accurate English was used. URL extraction failed because the configured backend was search-only; official CLI search and local `gh` help confirmed expected-head support. No repository files or remote state were changed. Parent owns branch publication and ticket/map reconciliation.

[tests-workflow]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/.github/workflows/run-tests.yml
[coverage-workflow]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/.github/workflows/coverage.yml
[development]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/docs/agents/development.md
[tracker]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/docs/agents/issue-tracker.md
[labels]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/docs/agents/triage-labels.md
[publication]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/ticket_publication/README.md
[publication-api]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/ticket_publication/github.rs#L227-L357
[graph]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/ticket_publication/graph.rs
[publication-labels]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/ticket_publication/labels.rs
[pool-adr]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/docs/adr/0001-extract-worktree-pool.md
[pool]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/pool_receipt/README.md
[pool-main]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/pool_receipt/main.rs
[hosted]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/hosted_review/README.md
[hosted-store]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/hosted_review/store.rs
[hosted-policy]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/hosted_review/policy.rs
[hosted-api]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/hosted_review/github.rs#L121-L164
[wake]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/hosted_review/wake.rs
[merge]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/merge_verification/README.md
[merge-main]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/merge_verification/main.rs#L37-L64
[merge-policy]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/merge_verification/verify.rs
[merge-api]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/merge_verification/github.rs
[merge-tests]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/tools/merge_verification/command_test.rs
[deploy]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/.github/workflows/deploy.yml#L6-L50
[multitool]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/.github/workflows/update-multitool.yml#L24-L41
[renovate]: https://github.com/LowkeyLab/bazel-repo/blob/65ab6e77b5d745b67bd882f32b9a915a300cb3f3/renovate.json
