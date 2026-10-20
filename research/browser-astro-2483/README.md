# Browser and Astro original-source mapping experiment

The [supplemental configured BuildBuddy validation](REMOTE-VALIDATION.md) records the later validation.
The sections below retain the original local experiment and its mapping limits.

This report answers [Test complete browser and Astro original-source mappings](https://github.com/LowkeyLab/bazel-repo/issues/2483).
The refreshed base is `39075c1280f357d58b06a9a471e617cd73949bdf`.
The experiment reuses the bounded fixture from `bf895c6435b785eca47df3decb87dbdcae1d1259`.
It changes no production application source or coverage workflow.

## Result

A checked AST bridge restores original processed-script line hits in the fixture.
An AST is the compiler's structured representation of source syntax.
Exact compiler mappings and raw Node ranges also restore the fixture's original template-expression hits.
The resulting partial report excludes HTML comments and static HTML.
Compiler-only analysis supplies real zero records for the disconnected component's server expressions and browser script.

The maintained Personal Website also passes a real Chromium interval through its local application URL.
Its homepage and replay action execute ten captured scripts with 829 precise ranges.
Those scripts expose no source-map URL in the default maintained artifact.
This browser run therefore establishes generated-script execution, without maintained original-source coverage.

The converter also reports positive default-argument counts when every caller supplies the relevant property.
Those counts do not establish fallback execution.
The fixture bridge and conditional records do not resolve that discrepancy.
A complete collector mechanism remains unproved.

## Current pins and execution boundaries

The current lockfile retains Astro `7.4.0`, `@astrojs/compiler-rs` `0.5.2`, Playwright `1.64.1`, and `ast-v8-to-istanbul` `1.0.7`.
The Node runtime is `24.15.0`.
The research browser remains Chromium `156.0.8078.4`.
Its archive SHA256 is `c3e50596aef52c58f8fda3c6b3411c33d34c88b177b3ed1c2243359fd3b8ba20`.
The branch restores that research-only archive declaration and adds no NPM dependency.

All new execution is local.
Every build command disables the remote executor, remote cache, event backend, and local-result uploads.
Commands bound the Bazel JVM to 1536 MiB, local CPU capacity to two, and local memory capacity to 2048 MiB.
Some build actions reuse the local action cache.
The server probe and browser driver execute as local `bazel run` programs.
They are not cached test results.

The server probe compiles the three fixture components before rendering.
It renders `Main` with true and false props, then renders `TestOnly` with a true prop.
The disconnected component never renders.
Its throwing frontmatter does not execute to manufacture zeros.
The [server completion receipt](evidence/server/server-completion.json.txt) records functional success and the collection boundary.

The fixture build and maintained site build execute before browser collection.
The browser server serves those existing artifacts.
It does not compile Astro or execute server templates during navigation.
The Node browser driver performs HTTP service and CDP orchestration.
Its own execution does not count as browser source execution.
CDP records Chromium script identifiers, text, ranges, and completion before closure.

## Checked processed-script bridge

The native bundle map reaches a reformatted Astro virtual script.
The bridge requires exact byte identity between that map's `sourcesContent` and the compiler's virtual-script output.
The [map-chain identity record](evidence/chain/map-chain-identity.json.txt) preserves the actual map and virtual-source identity.
The raw [bundle map](evidence/browser/Main.astro_astro_type_script_index_0_lang.Bb-C5Svw.js.map.txt) retains the complete first mapping stage.

The bridge parses the original Astro script and the compiler virtual script.
It compares their complete AST structures after removing source positions and literal spelling metadata.
It then pairs corresponding executable statement nodes.
A converted virtual statement must match exactly one paired statement start.
The bridge refuses ambiguous matches.

The [paired node records](evidence/chain/script-bridge.json.txt) map virtual positions to original component positions.
The [processed-line records](evidence/chain/processed-original-lines.json.txt) contain positive hits at `Main.astro:14` and `Main.astro:15`.
The browser's click assertion supplies independent functional evidence for that handler.

A compiler-only TypeScript generic probe also retains structural identity through this stage.
It does not establish browser execution for the modified TypeScript probe.
A separate semantic mutation changes `browserLabel(true)` to `browserLabel(false)`.
The bridge rejects that changed structure.
It does not invent correspondence after an unverified transformation.

## Original executable inventory and template observations

The original Astro AST supplies the executable inventory before any coverage conversion.
The independent oracle specifies these exact line sets:

| Source               | Expected executable lines     |
| -------------------- | ----------------------------- |
| `Main.astro`         | 3, 4, 5, 8, 9, 10, 14, 15, 19 |
| `Disconnected.astro` | 2, 3, 5, 6                    |
| `TestOnly.astro`     | 2, 4                          |

The AST inventory matches all fifteen expected lines.
It includes interpolation, a conditional expression, a callback, processed scripts, and an inline script.
It excludes delimiters, imports without executable initializers, comments, and static markup in this fixture.
These fixture classifications do not establish a universal Astro inventory.

The native statement converter lacks a distinct record for some executed template expressions.
It also assigns a positive statement record to the HTML comment.
The new candidate starts from original executable nodes instead.
It uses compiler maps to identify generated expressions of the expected syntax type.
It accepts a generated expression only when one candidate maps inside the original expression's source interval.
It then selects the narrowest raw V8 range containing that generated expression.

The [template evidence](evidence/chain/template-expression-evidence.json.txt) retains every original interval, generated interval, selected range, and count.
`Main.astro:8` and `Main.astro:9` each receive count two.
The callback line receives count four.
`TestOnly.astro:4` receives count one.
The comment and static button markup receive no executable record.
This candidate tests the fixture's ASCII source positions and expression forms.
It does not prove every Unicode position boundary or Astro syntax form.

## Supported conditional records and remaining branch discrepancy

Four explicit conditional expressions have stable identities from their original source offsets.
The [expected branches](evidence/chain/independent-expected-branches.json.txt) specify both arms independently from conversion.
The [original conditional records](evidence/chain/original-conditional-records.json.txt) preserve those identities and observed counts.

| Original conditional   | Observed arm counts |
| ---------------------- | ------------------- |
| `Main.astro:9`         | 1, 1                |
| `Main.astro:19`        | 1, 0                |
| `Disconnected.astro:5` | 0, 0                |
| `TestOnly.astro:4`     | 1, 0                |

The bounded matcher requires one converted conditional region per original conditional line.
It rejects multiple candidate regions instead of choosing one.
The LCOV block identifier is the original conditional's start offset.
The source path and arm index complete its identity.

The [default-argument discrepancy](evidence/chain/default-argument-gap.json.txt) remains separate.
`Main` receives an explicit `enabled` property in both calls.
`TestOnly` also receives an explicit `enabled` property.
Their default initializers therefore have zero expected executions.
The converter reports default-argument counts two and one.
The partial report does not treat those counters as proved original branches.
This omission is an unresolved mechanism requirement, not an approved coverage exception.
The earlier helper's synthetic branch-region discrepancy also remains unresolved in the retained raw merge report.

## Baselines and merge orders

The [compiler-only baseline](evidence/chain/original-baseline.lcov) contains all fifteen original executable lines with zero counts.
It also contains zero records for both arms of each explicit conditional.
The disconnected client assignment at line six remains present without loading its component in a browser.

The [forward partial report](evidence/chain/partial-forward.lcov) and [reverse partial report](evidence/chain/partial-reverse.lcov) have identical bytes.
They preserve server, processed-browser, inline-browser, and test-component hits under three canonical original paths.
They retain disconnected zeros and the four explicit conditional identities.
They do not claim complete maintained-site or repository coverage.
The existing combined LCOV workflow remains unchanged.

## Maintained application URL and missing maps

The research browser serves the actual `//personal_website:build` output at `/maintained/`.
The [route receipt](evidence/browser/maintained-route.json.txt) records the exact local URL and artifact path.
The browser checks the maintained heading and activates its replay button.
It observes the maintained animation's opacity change before collection.
No synthetic page replaces this maintained route.

The [maintained raw profile](evidence/browser/maintained-profile.json.txt) contains ten scripts and 829 ranges.
The [script summary](evidence/chain/maintained-summary.json.txt) preserves each URL, script identifier, range count, text artifact, and missing map URL.
The [artifact comparison](evidence/maintained-artifacts.json) shows that captured module text equals the actual built JavaScript bytes.
The [maintained HTML](evidence/browser/maintained-index.html.txt) preserves the served page.

All ten parsed script records have an empty source-map URL.
The candidate therefore cannot connect these generated ranges to maintained original sources through the required mapping chain.
It preserves those ranges without assigning invented original paths or line records.
The controlled fixture's successful bridge does not remove this maintained-artifact gap.

## Missing-output controls

Five separate controls change private copies after maintained functional success:

1. Remove the maintained browser profile.
2. Corrupt that profile.
3. Remove the maintained completion receipt.
4. Remove a captured maintained script's text.
5. Remove the fixture bundle map.

Each candidate process exits one.
Each copied functional receipt remains true.
The [control record](evidence/controls.json) separates these results.
The original browser probe also retains a functionally successful interrupted interval and its rejected collection command.
These are temporary adapter checks, not intrinsic completeness guarantees from Chromium or Astro.

## Remaining feasibility question

Can a coverage-only build retain the maintained site's full source-map chain and preserve original identities through its actual bundling and script transformations?
That experiment must apply the checked bridge to maintained scripts, not infer support from this fixture.
It must also establish faithful default-initializer and other supported branch records without synthetic counts.
A complete mechanism must reject ambiguous original-node matches and missing required maps.
The present report approves no collector exception or production implementation.

The Python architecture, remaining Python acceptance work, percentage thresholds, and Codecov policy remain outside this experiment.
No configured-remote execution, hosted CI, deployment, or upload evidence exists for this branch.
Remote approval remains pending under the coordinator's recorded approval-review rejection.

## Sources and reproduction

[Reproduction commands](REPRODUCE.md) retain the exact local configuration and separate collection stages.
Raw compiler, browser, map, and generated-source exports use a `.txt` suffix to prevent formatters from changing their bytes.
Remove that suffix when reconstructing the original runtime directory.
Command logs remove trailing whitespace. Private runtime receipts retain the original log bytes.

Primary references:

- [Astro script processing](https://docs.astro.build/en/guides/client-side-scripts/) describes bundling and TypeScript support for processed scripts.
- [CDP precise coverage](https://chromedevtools.github.io/devtools-protocol/tot/Profiler/#method-startPreciseCoverage) defines the browser counter interval.
- [CDP script metadata](https://chromedevtools.github.io/devtools-protocol/tot/Debugger/#event-scriptParsed) defines script positions and source-map URLs.
- [Pinned converter](https://unpkg.com/ast-v8-to-istanbul@1.0.7/dist/index.mjs) supplies the native statement and branch conversion used for comparison.
- [Experiment source](../../tools/research_astro/chain.mjs) records the checked bridge, original inventory, range sampling, and refusal conditions.

## Validation

The local server render, maintained build, fixture build, Chromium interval, and checked mapping assertions passed.
All five controls failed as expected after functional success.
Gazelle and whole-tree formatting passed with remote uploads disabled.
Focused hard lint passed with KeepSorted and no findings.
That lint result supplies no ESLint or ShellCheck evidence.
The [validation record](evidence/validation.json) separates source checks from local execution and local cache reuse.
The [source hashes](evidence/source-sha256.json) match the final executed collector artifacts and original compiler-map inputs.
The research branch has no pull request and requires no merge.
