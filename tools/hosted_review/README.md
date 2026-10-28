# Hosted review polling

This Rust command requests and polls Codex reviews on GitHub.com.
It preserves request identities outside reusable checkouts.
It does not merge PRs or replace review and CI acceptance gates.

## Commands

Run the first request for an exact PR head explicitly:

```sh
nix develop --command bazel run //tools/hosted_review -- request \
  --repo LowkeyLab/bazel-repo --pr <number> --head <full-commit-sha> \
  --now "$(date -u +%s)"
```

Use the same state directory for every later observation and resumed run:

```sh
nix develop --command bazel run //tools/hosted_review -- poll \
  --repo LowkeyLab/bazel-repo --pr <number> --head <full-commit-sha> \
  --now "$(date -u +%s)"
```

`--now` supplies Unix seconds. The decision function does not read a clock.
The default directory is `$XDG_STATE_HOME/hosted-review`, or
`$HOME/.local/state/hosted-review` when `XDG_STATE_HOME` is absent.
Use `--state-dir` to select another private directory outside Git checkouts.
Retain that directory across runs. Do not select a replacement directory to bypass saved state.

The command prints one JSON result. A nonzero exit means the observation or effect failed.
Do not treat an error as evidence that a review needs another request.

## Request rules

The `request` action permits the first request when no review exists for this head.
An existing incomplete review without saved history returns `untracked_head`.
Repeated `request` calls use the same eligibility rules as `poll` for a tracked head.
The `poll` action never creates a first request for an untracked head.
A changed head needs a separate explicit first request.

A retry requires all these facts:

- The PR is open and its head matches `--head`.
- The authenticated GitHub response identifies the Codex bot as user `199175422`,
  login `chatgpt-codex-connector[bot]`, and type `Bot`.
- Its review marker identifies an incomplete code review for this exact head.
- The last request for this head has a confirmed comment ID.
- More than 3,600 seconds passed since that request.

Exactly 3,600 seconds is insufficient. A completed review for another head does not establish completion.
Missing, unknown, malformed, or failed provider reads do not establish timeout eligibility.
The provider marker is `codex-review-state:v1`. A provider format change requires inspection.
This adapter does not infer completion from reactions or human text.

## Track an existing request

Import an existing request without sending another comment:

```sh
nix develop --command bazel run //tools/hosted_review -- track \
  --repo LowkeyLab/bazel-repo --pr <number> --head <full-commit-sha> \
  --request-comment <comment-id> --now "$(date -u +%s)"
```

The comment must belong to the authenticated caller.
It must contain `@codex review` and this line with the full commit hash:

```text
Review commit `<full-commit-sha>`.
```

The provider must report an incomplete review for that current head.
The command retains the comment ID and its server creation time.
Repeating `track` does not replace saved history or reset the timeout.
An unbound comment or an automatic review without request evidence needs manual inspection.

## Persistence and uncertain outcomes

A lock serializes callers for the same repository and PR.
The store synchronizes each intent before the outgoing request.
Each intent retains a unique request identifier, commit, caller identity, and explicit request time.
A confirmed request also retains the GitHub comment ID and server creation time.
The store retains previous requests when another request becomes eligible.

The command checks PR state and head again immediately before sending.
It also checks fresh provider evidence and newly observed external review requests.
GitHub comment creation does not provide an atomic head-condition operation.
A PR can still change after the final read. Review acceptance must verify the current head separately.

If the response is missing or invalid, the saved intent remains uncertain.
A later observation can confirm the exact marked comment from the same authenticated caller.
An absent match does not authorize another send, regardless of elapsed time.
Do not delete state or reset an uncertain intent. Inspect the saved identity and remote comments.

A newer request outside this command has no verified head binding in this store.
The `external_request` result stops automatic retries for that observation.
Inspect the external request before further action.

Common results are `requested`, `waiting`, `complete`, `closed`, `head_changed`,
`untracked_head`, `uncertain`, `reconciled`, `tracked`, `already_tracked`, and `external_request`.
A final-read change can also produce `changed_before_send` or
`provider_unavailable_before_send`. The retained intent prevents an unsafe retry.

## Verification design

The policy tests exercise the production decision function with explicit time and review evidence.
They verify the timeout boundary, completion, closure, missing evidence, and uncertain requests.
These are unit tests. The observed configured execution took 0.1 seconds per test target.

The command tests exercise production parsing, decisions, GitHub command construction,
real file persistence, and restart. Each test has private temporary state.
A controlled `gh` process supplies read responses as stubs.
Its recorded comments serve a mock role for the externally visible outgoing request.
Tests assert command outcomes and outgoing comments. They do not assert internal query counts.

These integration tests verify command wiring, state preservation, and external communication.
They do not prove live GitHub or hosted-provider compatibility.
No live review request is necessary for these tests.
