# Askama maintained syntax and original template coverage

The bounded adapter supports a tested subset of maintained Askama templates. It does not support the complete maintained template set.

The next feasibility question is precise. Can a compiler-aligned transformation support inheritance, block overrides, if-let bindings, and conditionals without else, while preserving render bytes and original executable locations?

## Scope and revision

This report resolves [ticket 2485](https://github.com/LowkeyLab/bazel-repo/issues/2485). It follows [ticket 2457](https://github.com/LowkeyLab/bazel-repo/issues/2457#issuecomment-6286591183) and the [approved scope](https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818).

The refreshed base is `39075c1280f357d58b06a9a471e617cd73949bdf`. The research branch is `research/askama-2485`. The experiment restores the earlier fixture from `ef0bd95cf0aadc20ebe2d779295da602bf6ccaf4`.

Current pins remain Askama, askama_parser, askama_macros, and askama_derive 0.16.1. Rust remains 1.99.1. LLVM remains 23.1.2. See [Cargo.lock](../../Cargo.lock) and [MODULE.bazel](../../MODULE.bazel). The parser checksum is `2c5ae75772275d268b03ab8bdccdd12117b6169ee23256942b34e46c9f476583`.

The [inventory](evidence/maintained-source-inventory.json) records all 16 maintained HTML files and their SHA256 hashes. Each research snapshot matches its maintained source byte for byte. The experiment changes no maintained template, application source, coverage threshold, collector policy, or upload policy.

## Native syntax and adapter acceptance

The native test compiles all 16 templates. It renders 21 cases with minimal context types. Cases include inheritance, block overrides, if-let, empty and populated loops, nested includes, field expressions, and method expressions. These cases do not run the production server.

The transformation matrix accepts eight maintained files and rejects eight. Acceptance means that transformation succeeds. It does not prove that each transformed file compiles or preserves render bytes.

| Maintained template              | Transformation result                                                  |
| -------------------------------- | ---------------------------------------------------------------------- |
| `index.html`                     | Rejects `Extends`                                                      |
| `layout.html`                    | Rejects `BlockDef`                                                     |
| `login.html`                     | Rejects `Extends` before its if-let body                               |
| `login/login_error_message.html` | Accepts static content                                                 |
| `login/login_success.html`       | Accepts expressions                                                    |
| `names.html`                     | Rejects `Extends`                                                      |
| `names/add_name_form.html`       | Accepts static content                                                 |
| `names/bulk_add_form.html`       | Rejects `Extends`                                                      |
| `names/bulk_add_success.html`    | Rejects a conditional without else                                     |
| `names/bulk_delete.html`         | Rejects `Extends`                                                      |
| `names/bulk_delete_table.html`   | Accepts Boolean if/else, loop, and expressions                         |
| `names/edit_name_form.html`      | Accepts field expressions                                              |
| `names/error_message.html`       | Accepts expressions                                                    |
| `names/name_row.html`            | Accepts field expressions                                              |
| `names/names_table.html`         | Accepts Boolean method condition, loop, include, and method expression |
| `welcome/call_to_action.html`    | Rejects if-let                                                         |

Four additional controls reject if-let, loop-else, match, and raw blocks. These failures describe adapter limits. They do not approve exclusions from coverage membership. See the [exact matrix](evidence/syntax-matrix.json) and [raw results](evidence/raw/syntax/results.tsv.txt). Each case retains stdout, stderr, and exit status.

Askama documents these syntax forms in its [official syntax reference](https://askama.rs/en/stable/template_syntax.html). The pinned [parser source](https://docs.rs/crate/askama_parser/0.16.1/source/src/node.rs) defines their separate AST nodes.

## Two measured defects and bounded corrections

The unchanged earlier instrumenter changes output in all four whitespace cases. A marker before an expression separates the original whitespace from its suppression flag. The candidate copies the left delimiter flag onto the marker. The candidate matches native render bytes in all four cases.

These cases cover `-`, `~`, and `+` expression flags, escaped values, both Boolean arms, empty and populated loops, and an included template. They do not prove every combination of block whitespace controls or global whitespace configuration. Askama defines flag precedence in its [whitespace documentation](https://askama.rs/en/stable/template_syntax.html#whitespace-control).

The first maintained-table compile exposed another defect. The parser span for `names.is_empty()` identifies the final call suffix. Replacing only that span produced this invalid condition:

```text
names.is_emptycrate::branch("nicknamer/server/lib/templates/names/names_table.html", 3, (()))
```

The [compiler error](evidence/matrix-2.stderr) preserves this failure. The candidate reads the complete simple-if header after the parser's `if` keyword span. It rejects quoted conditions. This is a bounded source transformation, not a complete compiler mapping. The pinned [expression parser](https://docs.rs/crate/askama_parser/0.16.1/source/src/expr.rs) remains the primary source for span behavior.

The native test initially expected named HTML entities. Askama emitted numeric entities. The tests now expect `&#60;&#38;&#62;`. The evidence retains the failed assertions. This correction changes the assertion, not the render output.

## Exact original-source records

The maintained names-table test renders an empty list and a one-row list. Native and instrumented output match byte for byte in both cases. The included row keeps `nicknamer/server/lib/templates/names/name_row.html` as its source identity. Nested paths do not collapse to basenames.

The expected line set comes from the original source and the two render cases:

| Source                   | Expected executable lines and combined hits |
| ------------------------ | ------------------------------------------- |
| `names/names_table.html` | `1:2`, `31:1`, `41:2`                       |
| `names/name_row.html`    | `1:1`, `2:1`, `3:1`, `4:1`, `8:1`, `30:1`   |
| Research `unused.html`   | `1:0`, `2:0`                                |

The condition executes twice. The populated case executes the loop, include, and each row expression once. The total-count expression executes twice. Both condition arms have one hit. The unused condition arms remain zero. Static HTML, text, and comments produce no executable-line records.

See the [expected LCOV](../../tools/research_askama/expected.lcov), [observed LCOV](evidence/raw/subset_collection/valid/combined-zero-first.lcov.txt), [baseline](evidence/raw/transformed-subset/baseline.json.txt), and [raw events](evidence/raw/subset_collection/render/events.json.txt). The [other merge order](evidence/raw/subset_collection/valid/combined-zero-last.lcov.txt) matches exactly.

Loop counters measure entry and body execution. They do not measure loop exit branches. Node identities use original byte offsets and source hashes. This experiment does not establish branch identity stability across source revisions.

Five controls run after functional rendering succeeds. Each control fails collection with a nonzero exit:

- Remove included-template metadata.
- Change an original source hash.
- Insert an event with an unknown source identity.
- Remove functional completion.
- Change the expected event hash.

The [raw collection directory](evidence/raw/subset_collection) retains each changed input and error. Only the supported subset receives `bounded_subset_complete`. Unsupported syntax cannot produce that result through this experiment.

## Validation and reproduction

Five fresh local tests passed. Bazel reused build actions, but it executed all five test processes. The [BEP extract](evidence/final-4-endpoint-and-test-evidence.json) records `linux-sandbox` execution and zero test network time. See [test stdout](evidence/final-test-4.stdout) and [test stderr](evidence/final-test-4.stderr).

Run these commands from the research branch:

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m test \
  --config=ci-remote --remote_executor= --remote_cache= --bes_backend= \
  --remote_upload_local_results=false --jobs=4 \
  --local_resources=cpu=2 --local_resources=memory=2048 \
  --cache_test_results=no \
  //tools/research_askama:maintained_native_test \
  //tools/research_askama:syntax_test \
  //tools/research_askama:whitespace_comparison_test \
  //tools/research_askama:subset_comparison_test \
  //tools/research_askama:subset_collection_test
```

Gazelle passed after source edits. Whole-tree Aspect formatting passed with Java 21 formatter settings. A research-only Prettier exclusion preserves the whitespace fixture bytes. The [format BEP extract](evidence/format-endpoint-and-test-evidence.json) confirms empty cache, executor, and BES endpoints. It also confirms disabled result uploads and bounded resources.

Aspect lint completed with exit 0 and 22 Clippy warnings. The warnings concern research-code style and documentation. This result does not claim warning-free lint. See [lint stderr](evidence/lint-publish.stderr). The bounded tests build the experiment. This research did not run a whole-repository build.

Fresh configured-remote execution and hosted upload evidence remain absent. Automatic approval review rejected the earlier configured-remote action. User approval remains pending with the coordinator. This experiment uses local execution only. It does not repeat the predecessor's established native LLVM omission result.

Raw exported artifacts use `.txt` suffixes to prevent formatter changes. The [artifact manifest](evidence/artifact-hashes.json) records their exact hashes. Published command announcements redact credential values. Private runtime receipts retain raw announcements.

## Recommendation and remaining question

Continue adapter research before selecting it for automatic membership. The experiment proves preserved render bytes and original-source records for the tested subset. It also proves that the earlier adapter cannot process half of the maintained template set.

The remaining feasibility question covers inheritance and block overrides, if-let bindings, conditionals without else, complete condition spans, and branch semantics for loops. A later experiment must preserve native bytes and reject incomplete mappings for those forms. Unsupported syntax must remain visible until that evidence exists.

## Assignment receipts

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment handle: `9a30aebb-f1dc-4927-b81f-0777e449b16e`.
Acquisition operation: `8afc36fd-e7ee-4d1f-b641-f55d6845cc60`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Private pool receipt directories are under `/home/tacascer/.local/state/worktree-pool/agent-receipts/`: `askama-2485-catalog-mGdsNq` and `askama-2485-acquire-DIQPQy`. Both commands completed with exit 0. The final handoff records the exact release result separately.
