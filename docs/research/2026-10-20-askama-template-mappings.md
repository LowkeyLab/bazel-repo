# Askama original-template coverage mappings

Issue: [Determine Askama original-template coverage mappings](https://github.com/LowkeyLab/bazel-repo/issues/2457).
Predecessor: [Classify remaining maintained executable sources for coverage](https://github.com/LowkeyLab/bazel-repo/issues/2444).
The predecessor supplied source inspection. This report adds native mappings and one bounded AST adapter.

## Result

Native LLVM export contains only `tools/research_askama/native.rs`.
Its measured and empty-profile reports contain no original `.html` identity.
A positive Rust render helper therefore does not establish template coverage.

The bounded adapter produces original template statements and simple Boolean branch outcomes.
It preserves included-template identity and unused-template zeros.
Native and instrumented rendered bytes match exactly, including whitespace.
Both baseline merge orders retain positive counters.
Four collection controls fail after successful Rust assertions.
This establishes feasibility for the tested subset, without approving an exception or production mechanism.

## Pins and native experiment

The refreshed base is `df390e4205234e7ea4d8b09be436739a23de27a2`.
The lockfile selects Askama, `askama_derive`, `askama_macros`, and `askama_parser` version `0.16.1`.
The manifest's requirement is `0.16.0`. The experiment uses the locked version.
Rust is `1.99.1`, commit `58cf8f9c35b355199727627d07ae34a4e2846d67`, with compiler LLVM `23.1.1`.
The existing registered toolchain uses edition `2024` and target `x86_64-unknown-linux-gnu`.
LLVM tools are `23.1.2`.
The evidence manifest records effective compiler metadata and executable hashes.
No production registration or dependency pin changes.

The [Rust coverage documentation](https://doc.rust-lang.org/rustc/instrument-coverage.html) describes LLVM instrumentation and profile export.
The [Askama syntax documentation](https://askama.rs/en/stable/template_syntax.html) defines expressions, conditions, loops, and includes.
Neither document promises original-template LLVM mappings.
The actual native export establishes the observed omission.

The normal Rust test uses `googletest` assertions and `-Cinstrument-coverage`.
It renders one true conditional arm, two loop items, and an included label.
The disconnected template compiles but never renders.
LLVM maps the ordinary Rust render helper, without template locations or template branches.
Its empty-profile baseline also omits all three templates.
An initial macro-only test produced no exportable native map.
The ordinary helper establishes a native call site while the template omission remains.
The final profile writes once at process exit.

## Minimal AST adapter

The adapter uses the pinned [parser archive](https://static.crates.io/crates/askama_parser/askama_parser-0.16.1.crate).
Its checksum is `2c5ae75772275d268b03ab8bdccdd12117b6169ee23256942b34e46c9f476583`.
The inspected APIs are `Parsed::new`, `Parsed::nodes`, `Node::span`, and `Span::byte_range`.
They supply constructs and original byte positions.
The compile action parses all three originals before rendering.
It creates zero metadata from their AST nodes, including the unused template.
It does not execute templates to construct a baseline.

The adapter supports `Expr`, `If`, `Loop`, and `Include`.
It excludes comments and literal text from executable metadata.
It accepts one simple Boolean `if/else` pair and plain loops.
It rejects conditional bindings, additional conditional arms, filtered loops, and loop `else` bodies.
It rejects every other executable AST node.
This subset limits the experiment. It does not exclude unsupported maintained sources from membership.

The transformation inserts empty-string hit calls before expressions, loop entries, and includes.
A Boolean hook records the actual condition outcome.
Askama then compiles the transformed templates through its normal derive macro.
Events retain original template paths and AST byte positions.
The included expression retains `included.html` rather than its caller's identity.
The exact native and instrumented render comparison establishes unchanged output for this fixture only.

The collector checks original source hashes, baseline identity, profile schema, and event identities.
It also requires an independent completion receipt with the exact profile hash.
These checks belong to the temporary adapter, not to Askama or LLVM.
The fixture inventory explicitly names three templates.
Automatic discovery remains untested.

## Known original locations

The oracle classifies original constructs independently from counters.
The final formatter changes line grouping, but the AST preserves original byte ranges.
The evidence contains the complete original and transformed text.

| Original location | Known role                               | Observed result                    |
| ----------------- | ---------------------------------------- | ---------------------------------- |
| `main.html:1`     | Comment followed by condition            | Condition hit, branch counts `1,0` |
| `main.html:2`     | True-arm interpolation                   | Positive hit                       |
| `main.html:3`     | Conditional delimiter                    | No invented `DA`                   |
| `main.html:4`     | Untaken interpolation                    | Real zero hit                      |
| `main.html:5`     | End delimiter followed by loop           | One loop-entry hit                 |
| `main.html:6`     | Loop interpolation                       | Two hits                           |
| `main.html:7`     | End delimiter followed by include        | One include hit                    |
| `main.html:8`     | Static markup                            | No `DA`                            |
| `included.html:1` | Included interpolation                   | One hit under its own identity     |
| `unused.html:1,2` | Disconnected condition and interpolation | Zero `DA` and zero condition arms  |

The comment contributes no AST counter, despite sharing a line with the condition.
Only the simple Boolean condition supplies branch records.
Loop entry and iteration counts do not establish loop branch outcomes.
The adapter emits no loop `BRDA` claim.
Template LCOV contains statements and branches, without invented template functions.
Native Rust function records remain separate in the combined report.

## Failure and merge controls

Both baseline merge orders produce identical counter maps.
Measured positives remain positive when the baseline enters first or last.
The combined report retains three original template identities and one native Rust identity.
It contains no generated-template identity.

Four independent controls run after successful native and instrumented Rust assertions:

1. Remove the template profile.
2. Corrupt the template profile.
3. Remove the completion receipt.
4. Remove the AST baseline input.

Each collector process exits `1` while its functional receipt remains present.
The test restores the original profile and receipt after each control.
These observations establish failure for the tested evidence losses.
They do not prove arbitrary event completeness or crash-safe collection for every interval.
A valid profile with silently omitted events still requires an independent completeness design.

## Capability matrix and candidates

| Mechanism              | Demonstrated capability                                 | Missing observation                                  |
| ---------------------- | ------------------------------------------------------- | ---------------------------------------------------- |
| Native Rust and Askama | Functional render and mapped Rust helper                | Original template identities and baseline            |
| Parser baseline        | Original executable zeros, including unused template    | Automatic discovery and broader syntax               |
| Empty-string insertion | Expression, loop-entry, iteration, and include events   | General whitespace-control and error semantics       |
| Boolean hook           | One observed arm and one real zero arm                  | Compound conditions, bindings, and other branches    |
| Original identities    | Included template and canonical `.html` records         | Inheritance, macros, imports, and nested expansion   |
| Adapter collection     | Source/profile/receipt checks and four visible failures | Arbitrary completeness and concurrent collection     |
| Combined LCOV          | Native Rust and templates, with merge-order equality    | Production selection, aggregation, and hosted upload |

The minimal adapter is a candidate for the demonstrated subset.
A later mechanism decision must address unsupported syntax, discovery, and collector completeness.
Native `render()` coverage alone cannot supply the template denominator.
This report preserves membership, thresholds, combined-report policy, and Codecov policy.

## Reproduction and validation

The fixture is in `tools/research_askama`.
Use [REPRODUCE.md](../../tools/research_askama/REPRODUCE.md) for commands.
The [evidence directory](askama-2457/) contains raw mappings, AST metadata, transformed templates, profiles, receipts, intermediate reports, and combined LCOV.
The manifest records source hashes and actual action/test execution.
All builds and tests use Bazel-managed tools.
Remote build work, remote test execution, cached results, and hosted evidence remain separate.
No hosted CI or Codecov upload ran.
Required Gazelle and full repository formatting passed.
The final [BuildBuddy test](https://app.buildbuddy.io/invocation/9cb83a94-d86c-4f4b-a1ce-43bec3c71c0f) executed remotely for `0.260` seconds with test-result caching disabled.
Its action summary records two executed remote processes, one remote-cache hit, and 123 action-cache hits.
These classes remain separate from hosted CI and upload evidence.

Focused `aspect lint //tools/research_askama/...` exits `0` with seven Clippy style advisories.
They concern iterator formatting, string formatting, function length, and sort style.
The collector retains full-width numeric event identities after correction of seven narrowing advisories.
The lint result does not establish zero findings.
No repository-wide application build ran.

All ten original input hashes match the final executed artifacts.
An independent oracle checks the exact four combined source identities and their nonempty executable records.
It checks the table's template counters and both conditional arm pairs.
It also checks zero baseline counters, merge-order equality, and exact render-byte equality.
The strict prose test reports no hard findings.
