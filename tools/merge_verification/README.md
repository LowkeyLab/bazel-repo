# Merge verification

Use this Rust command before and after an authorized squash merge for `LowkeyLab/bazel-repo` into `main`.
It reads GitHub.com through the authenticated `gh` command.
It returns structured evidence and preserves private raw receipts outside Git worktrees.
Merge execution remains the delivery workflow's responsibility.

## Inputs and commands

Prepare a JSON array of reviewed changed paths in a private file:

```json
["tools/example/main.rs", "tools/example/BUILD.bazel"]
```

Include both paths for a rename. Include removed paths.
Prepare the independent source reviews as a separate JSON file:

```json
{
  "head": "<full-reviewed-commit>",
  "standards_findings": 0,
  "spec_findings": 0
}
```

Those counts are caller-supplied review evidence. The command checks their head and zero findings.
It does not perform either source review.
Keep the source review record and reviewed paths with the task receipts.

Before merge, run:

```sh
nix develop --command bazel run //tools/merge_verification -- before \
  --repo LowkeyLab/bazel-repo --pr <number> --head <full-reviewed-commit> \
  --reviewed-files <absolute-reviewed-files.json> \
  --source-review <absolute-source-review.json> \
  --receipt-dir <absolute-private-receipt-root>
```

A successful version 1 result has `passed: true` and exit status zero.
The output names its unique `receipt_directory`.
Preserve that directory's `result.json` for the next phase.
Use the same reviewed head and file inputs when you merge.
Bind the merge operation to that head with its expected-head condition.
A passing observation is a snapshot. GitHub can change after its final read.

After merge, run:

```sh
nix develop --command bazel run //tools/merge_verification -- after \
  --repo LowkeyLab/bazel-repo --pr <number> --head <full-reviewed-commit> \
  --reviewed-files <absolute-reviewed-files.json> \
  --source-review <absolute-source-review.json> \
  --before-receipt <absolute-before-receipt-directory/result.json> \
  --receipt-dir <absolute-private-receipt-root>
```

The after result identifies the actual merge commit, reviewed paths, changed paths, and changes already present on the target.
It requires a successful before receipt for the same repository, PR, head, and paths.
Each reviewed path must match the merged Git blob, type, and file mode.
A removed path must remain absent. A rename includes its old and new paths.
Changes already present on the advanced target remain valid when their reviewed contents match.
Unrelated target changes can remain in the merged tree.
Whole-tree equality does not determine acceptance.
The squash commit must not introduce changes outside the reviewed paths.

## Acceptance policy

Before acceptance, the command requires:

- An open PR with the exact reviewed head and GitHub's confirmation that the PR can merge.
- The complete expected changed paths and independent source review counts of zero.
- Accepted published checks and workflow results for that head.
- The `test`, `lint`, and `format` checks from GitHub Actions.
- All additional required checks from current branch rulesets and classic protection.
- The `Bazel Tests` and `Coverage` workflows.
- An accepted published `codecov/patch` result for source changes.
- The latest authenticated Codex review for this head, completed with both findings flags false.
- No active change request and no unresolved review thread.
- A final PR read with the same head, target base, and GitHub's confirmation that the PR can merge.

Only a change whose entire reviewed path set has a Markdown extension receives the documentation-only exception.
The extension comparison ignores case.
That exception permits an absent `codecov/patch` result. It does not permit a failed published `codecov/patch` result.
The other checks and workflow requirements also apply to documentation changes.
GitHub identities bind the required Actions and Codecov checks to their applications.

The Codex identity requires user ID `199175422`, login `chatgpt-codex-connector[bot]`, and type `Bot`.
Review acceptance requires an exact commit match in `codex-review-state:v1`.
An authenticated completed review for an older commit cannot accept the current commit.

The transport reads every page with [GitHub command-line pagination](https://cli.github.com/manual/gh_api).
It follows review-thread cursors and verifies available collection totals.
Failed reads, malformed JSON, missing fields, count changes, incomplete collections, and truncated trees stop acceptance.
Verification after merge uses [Git tree objects](https://docs.github.com/en/rest/git/trees) and immutable commit identities.
A missing classic protection rule is valid only when GitHub explicitly reports `Branch not protected` with status `404`.
Other protection errors stop acceptance.

## Receipts and boundaries

The default receipt root is `$XDG_STATE_HOME/merge-verification`, or `$HOME/.local/state/merge-verification`.
An explicit root must be absolute, private, and outside Git repositories.
Each invocation creates a separate directory with mode `0700` and files with mode `0600`.
Each API command preserves its arguments, raw stdout, raw stderr, and exit status before response interpretation.
The directory also retains input identities, source review data, and `result.json`, including acceptance failures.
The after phase also retains its supplied before result.
An invalid receipt location stops before API reads.
An operating-system failure to create or write receipts prevents acceptance.
Preserve these runtime records with the task handoff.
They can contain private PR and review text.

Continue to verify task ownership, issue blockers, local checkout state, required validation, and authorized merge execution in the delivery workflow.
The command supplies GitHub acceptance evidence. It does not transfer assignments, recover worktrees, merge PRs, close tickets, or release workspaces.

## Test boundary

The command tests use private files and a controlled `gh` executable.
Its responses are stubs for the external GitHub service.
The command's production parsing, checks, cursor handling, result output, and receipt persistence remain real.
Assertions inspect acceptance results, merged file identities, and private receipts.
The tests reject API mutations.
They do not prove live GitHub or hosted-review compatibility.
The delivery workflow must retain a live before and after observation for its own PR.
