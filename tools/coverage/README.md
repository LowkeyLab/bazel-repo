# Coverage discovery

Run the existing coverage command:

```bash
nix develop --command bazel run //tools/coverage -- //...
```

The command writes `coverage-inventory.json` before it runs coverage.
The inventory contains maintained sources, Bazel owners, rule kinds, tags,
and scope exclusions. Git supplies tracked and untracked files. Git ignores
untracked build output. The inventory also respects `.bazelignore`.
Package discovery includes hidden directories. It uses no project list.
An eligible source without a Bazel owner causes command failure.

Instrumentation includes targets in the main repository. The command selects the
existing Python baseline rules automatically, including disconnected sources.
Deployment and container targets stay outside the additional coverage roots.
Existing test targets, execution filters, reports, and thresholds retain their
behavior. The inventory retains sources whose owners use `manual`, `no-remote`,
or `no-coverage` tags.

The Python integration selects baselines for maintained production sources, tests,
and executable helpers. Test-only roots select metadata without changing test
execution filters. The inventory also reports JavaScript/TypeScript and Astro sources.
The integrations below retain collector evidence and source-specific limits.
Measurement requires collector evidence.
Ticket #2509 adds reconciliation across collector families.

The scope excludes WebAssembly execution, Clojure, SQL and procedural bodies,
Askama templates, GraphQL documents, Nix, shell, Starlark, Caddy configuration,
and non-executable content. Eligible surrounding code remains in the inventory.
Generated output, vendored code, and external dependencies remain outside scope.

Run discovery acceptance from an assigned checkout:

```bash
nix develop --command bazel run //tools/coverage:discovery_test
```

The acceptance test creates temporary ordinary and hidden packages. It verifies undeclared
source failure, ownership, execution tags, and real Python baseline records.
The existing Python membership acceptance command checks imported execution,
subprocess collection, generated output, and preservation of Go records.

Git attributes record exact source exclusions. Set `coverage-scope` to
`vendored`, `generated`, or `non-executable` only when that classification applies.
The command rejects other values. Execution tags retain eligible source membership.
The existing vendored Gazelle subtree and generated GraphQL output use these
attributes. New ordinary projects require no attribute or central registration.

Run automated command tests:

```bash
nix develop --command aspect test //tools/coverage:inventory_test
```

These tests supply controlled Git and Bazel protocol responses. They verify
ownership, scope, and failure handling through the executable. Discovery
acceptance retains real Git, Bazel, and collector execution.

## Final source reconciliation

The coverage command checks every eligible source after the collectors finish.
It writes `coverage-reconciliation.json` only after reconciliation passes.
The receipt identifies measured sources, zero-hit sources, empty executable sets,
and exact collector limits. Scope exclusions remain separate.
Secure Hash Algorithm (SHA) hashes bind the receipt to its inventory, report, and collector receipts.
Each hash uses SHA-256.
Native baseline validation compares artifact bytes with the digest and length in
[Bazel build events](https://github.com/bazelbuild/bazel/blob/9.3.1/src/main/java/com/google/devtools/build/lib/buildeventstream/proto/build_event_stream.proto).
An incomplete output group or changed artifact causes failure.
These checks preserve valid empty baselines from configured source selection.

An omission requires a source-specific limit that explicitly permits a missing record.
Branch and child-process limits cannot permit missing required records.
Configured collector records must retain their executable lines.
Python checks use the public
[`Coverage.analysis2` API](https://coverage.readthedocs.io/en/latest/api_coverage.html#coverage.Coverage.analysis2).
This API distinguishes an empty executable set from missing executable lines.
Reconciliation removes excluded records. It preserves eligible records and measured counts.
It adds no coverage threshold.

The hosted workflow checks both the initial report and the final merged report.
It retains both reconciliation receipts and the original collector receipts.

Run the command tests through Bazel:

```bash
nix develop --command aspect test //tools/coverage:reconcile_test
```

## Go sources

Gazelle uses shared declarations for Go libraries, executables, and tests.
Each declaration adds independent metadata. Discovery selects that metadata
through Bazel ownership, including hidden and disconnected packages.
The added roots collect baselines without test execution.
The native target supplies its public `GoArchive` source providers.
Native transitions select direct and embedded sources before baseline collection.
A source rule exposes these files through `DefaultInfo`. This gives Bazel the
local sources that it requires before it retains custom baseline files.
The coverage build filter includes their `go-coverage` tag.
Existing test filters retain their behavior.

The [pinned rules_go baseline builder](https://github.com/bazel-contrib/rules_go/blob/f5a8f6a338fb9d7862f0f2247bec6da17711e392/go/tools/builders/baseline_coverage.go)
uses the SDK cover tool and coverage metadata to obtain real executable lines. Internal and external test sources
use that same builder. Build constraints exclude sources for the selected
configuration. Declaration-only sources retain the builder's empty executable
set. Baselines and measured reports use the native Bazel `LCOV` merger.
The report command applies the inventory's Go exclusions after native merging.
It checks Go executable records and their line totals before HTML generation.
Other language records retain their contents.
Go contributes its maintained line model. This integration adds no branch records.

The pinned collector patch rejects missing profiles, empty profiles, invalid
modes, corrupt records, and scanner failures. It propagates profile-generation
errors and report-write errors. Child collector errors fail the child process.
A valid header with no executable records remains valid.
The patch preserves line attribution and execution counts.

Run the real collector checks and command acceptance:

```bash
nix develop --command aspect test //tools/go:collector_test
nix develop --command bazel run //tools/coverage:go_membership_test
```

The collector tests use subprocesses with separate output directories.
They verify collector failures and measured records through the
[maintained converter](https://github.com/bazel-contrib/rules_go/blob/f5a8f6a338fb9d7862f0f2247bec6da17711e392/go/tools/bzltestutil/lcov.go).
Command acceptance covers these cases:

- Ordinary and hidden packages.
- Internal and external test sources.
- Sources without executable lines and sources with build constraints.
- Build tags and the `cgo`, `race`, and `msan` settings.
- Platform selection and source filegroups.
- Measured tests and a separate Go executable.
- Report preservation and required-profile failures after successful functional tests.
- Child collector errors after successful ordinary execution.

Child collection requires a Bazel-instrumented Go executable and an inherited
`COVERAGE_DIR`. The acceptance test uses a real `os/exec` child.
Pass `--evidence <directory>` after `--` to retain commands, inputs, and reports.
The existing hosted workflow runs this acceptance and retains its evidence.

The command retains configured Go baseline sources and executable lines in
`coverage-go-baselines.json`. Build events identify a required baseline artifact
for each selected Go root. The report validator rejects absent roots, artifacts,
source records, or executable lines. Native build constraints select these
baselines. The validator preserves measured counts and other language records.

## Native Rust sources

Rust declarations add coverage roots for libraries, executables, and tests.
Baseline actions extract Rust archive objects through
[`llvm-ar`](https://llvm.org/docs/CommandGuide/llvm-ar.html).
They run [`llvm-cov export -empty-profile`](https://llvm.org/docs/CommandGuide/llvm-cov.html)
with the configured LLVM tools. The native Bazel merger combines baselines and
observations. It retains measured hits.

Discovery includes ordinary and hidden packages. Metadata roots build test
executables without test execution. Existing execution filters retain their
behavior. The command rejects required roots outside the instrumentation filter.

The command writes `coverage-rust-baselines.json`. This receipt identifies configured
roots, executable lines, and exact mapping limits. Missing roots, artifacts,
source records, or exported executable lines cause failure.

LLVM sometimes reports more lines in `LF` than it identifies with `DA` records.
The receipt records this limit for each affected source and rule. It retains
the raw artifact path. The integration adds no line identities.
Some procedural test macros produce no native mapping for their source file.
Sources without mapping retain an exact object and diagnostic reference.
This status does not establish zero coverage.

Separate native executables in a Rust test's `data` receive matching coverage
objects. The coverage manifest includes their maintained sources.
For an executable inside an archive, declare its original Rust binary target in
the test's `data` too. An archive alone does not expose the matching LLVM object.
A declared child can remain unexecuted. LLVM profiles cannot distinguish this case
from a child whose complete profile disappeared. The receipt identifies this
guarantee limit for each child source, rule, and object. Acceptance removes only
the child profile and retains the available object and profile IDs.
The collector still requires the main test profile when executable mappings exist.
Coverage links retain ELF build IDs on configured Linux targets.
The collector uses LLVM object and profile IDs to require matching input.
It rejects invalid required profiles, missing objects, and failed LLVM export. Native line coverage uses stable Rust. WebAssembly execution remains
outside scope. The integration adds no nightly branch instrumentation.

Run local collector acceptance from an assigned checkout:

```bash
nix develop --command bazel run //tools/coverage:rust_membership_test -- --evidence /absolute/new/private/directory
```

The command checks hidden and ordinary packages, disconnected baselines,
generated sources, empty mappings, measured tests, separate executables, and
required-output failures.
It retains command results, raw profiles, reports, and object evidence.

Evidence directories must be new and outside the checkout.
Local mode clears remote execution, cache, and Build Event Service (BES) endpoints.
Use `--execution configured` to inherit the caller's Bazel configuration.
The evidence records actual execution and cache status.
Configuration alone does not establish remote execution.
The existing hosted workflow runs configured acceptance and retains its evidence.

## Java and Kotlin sources

Shared JVM declarations add configured class-analysis roots for libraries,
executables, and tests. Gazelle selects these declarations for Java.
Kotlin declarations use the same adapter. Discovery selects ordinary, hidden,
and disconnected sources without a project list. Metadata roots do not execute tests.

The analyzer uses the pinned JaCoCo libraries from Bazel's Java tools.
It uses the documented [Analyzer](https://www.jacoco.org/jacoco/trunk/doc/api/org/jacoco/core/analysis/Analyzer.html),
[CoverageBuilder](https://www.jacoco.org/jacoco/trunk/doc/api/org/jacoco/core/analysis/CoverageBuilder.html),
and [ExecFileLoader](https://www.jacoco.org/jacoco/trunk/doc/api/org/jacoco/core/tools/ExecFileLoader.html)
APIs. Standard class analysis supplies zero-hit baselines without class execution.
The native JVM runner supplies raw execution profiles.
The adapter analyzes original classes from each configured owner separately.
It preserves class names and applies standard JaCoCo filters.
Coverage builds enable the documented
[rules_kotlin ABI-jar option](https://bazel-contrib.github.io/rules_kotlin/kotlin#define_kt_toolchain).
This keeps instrumented runtime bytecode outside Kotlin inline compilation.
Ordinary builds retain their existing configuration.

The command writes `coverage-jvm/receipt.json`.
A metadata aspect reads the public [InstrumentedFilesInfo](https://bazel.build/rules/lib/providers/InstrumentedFilesInfo) provider for each configured native JVM test.
Only eligible Java/Kotlin sources in that configured dependency closure require profiles.
An inactive Java dependency does not require profiles from an excluded Clojure execution.
Missing or invalid configured test metadata causes collection failure.

The directory retains raw profiles, configured class jars, metadata, baseline XML,
and measured XML. The receipt identifies source mappings, execution evidence,
and exact collector limits. Build events use local file URIs for downloaded outputs.
Missing roots, required class inputs, required profiles, corrupt data, and incomplete
source analysis cause command failure. Other language records retain their contents.
Clojure remains outside scope.

A unique source filename within a configured owner supplies an exact path.
This permits package/directory mismatches and duplicate filenames across owners.
Ambiguous filenames within one owner retain source-specific limits.
Identical class bytes under distinct original sources cannot identify the executed owner.
The adapter omits affected measured records and records the exact class and source limits.
Different configured versions of one class retain their own standard JaCoCo results.
Inactive `select()` sources retain explicit configuration limits, including empty selected source sets.
The adapter does not guess their owners.
Standard Kotlin filters and compiler mappings retain their inline attribution limits.
Out-of-range compiler locations retain an exact source and report reference.
These limits do not establish measured zero coverage.
Configured compiler output identifies sources that emit no class.
The receipt records these empty executable sets without adding executable lines.

The native runner can emit empty profiles when its agent is inactive.
The receipt records these files separately.
An executed JVM test requires at least one valid, nonempty profile.
JaCoCo profiles cannot distinguish an unexecuted child from a child whose complete profile disappeared.
The receipt records this limit for each declared child source and rule.

JaCoCo supplies binary line coverage and branch totals for each source line.
LCOV retains line hits. Standard XML and the receipt retain branch totals.
The maintained LCOV merger does not retain totals without individual branch records.
The adapter adds no branch identities or execution counts.
The receipt records this branch limit for affected sources.

Run the real command checks from an assigned checkout:

```bash
nix develop --command aspect test //tools/coverage:jvm_report_test
nix develop --command bazel run //tools/coverage:jvm_membership_test -- --execution configured --evidence /absolute/new/private/directory
```

The acceptance checks retain commands, fixture inputs, build events, raw reports,
and configured class evidence. Local mode clears execution, cache, and Build Event Service (BES) endpoints.
Configured mode uses the repository's remote Linux configuration.
Read the execution evidence before claiming fresh remote execution.

JVM profile validation uses each native test's configured runtime jars and original sources, including declared JVM executable children. Each required raw profile must pass standard JaCoCo parsing. A profile with class data must match a class ID in those exact configured artifacts. A session-only profile is valid zero coverage. A profile with neither sessions nor class data is incomplete. Unrelated profiles cause failure and preserve the existing report.

Inactive source limits retain each declaring owner, even if another owner selects the same source. Receipts retain branch totals per owner. They omit aggregate branch totals when configured owners report different branch states, including an owner with no branches. The source-specific limit links the separate analyses.

Native Java binaries expose their runtime jars through the public [`JavaInfo.compilation_info.runtime_classpath`](https://bazel.build/rules/lib/providers/java_compilation_info#runtime_classpath) API. Their `transitive_runtime_jars` set is empty by design. The adapter uses the public transitive runtime set when compilation metadata is absent. Exact configured artifacts establish profile identity even when `-g:none` removes source names. Source attribution then remains an explicit limit.

The command prepares the analyzer with `bazel run --script_path`. Bazel materializes the selected executable and its runtime files. The report does not infer an analyzer path from `bazel info bazel-bin`.

## Node and Angular sources

The existing command selects JavaScript and TypeScript sources from the independent
inventory. Native baselines use the pinned rules_js collector and its manifest
interface. No project list controls native membership. This local analysis runs
through a Bazel-prepared executable. It does not execute disconnected applications.
The native Bazel LCOV merger combines baselines and measured records.

The [pinned collector](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js)
supplies its standard JavaScript empty-script baseline semantics. Native JavaScript
records retain that model, including its branch semantics.
Native TypeScript and JSX empty-script fallbacks do not establish original
executable records. The adapter retains these raw reports as evidence. It records
exact source and rule limits and excludes unsupported fallback records from the
combined report. Supported mapped records retain their counts.
The native library configuration requests documented TypeScript source maps and
inline sources. The native original-manifest limit can still prevent attribution.
The adapter adds no original executable lines or branch identities.

Angular projects use the documented
[coverageInclude option](https://angular.dev/cli/test) for their source directories.
The shared BUILD adapter derives this option from each project root. This includes
disconnected application entry points and libraries without a project list.
All six Vitest configurations use one adapter. Reports use the writable test
temporary directory. Missing reports, incomplete records, invalid paths, invalid
counts, and invalid branches cause failure. Ordinary tests need no coverage report.
The adapter uses the documented
[autoAttachSubprocess option](https://vitest.dev/config/coverage.html#coverage-autoattachsubprocess).

The command writes `coverage-javascript/receipt.json`. Configured native test
metadata uses the public `InstrumentedFilesInfo` provider. Each executed native
JavaScript test requires configured metadata, a valid report, and valid Node
profiles. Native report membership must match its configured original sources.
Native profiles do not declare the expected process set. A valid parent profile
cannot prove that every executed child profile remains available. The receipt
records this limit for each configured source and test rule. Detectable missing
or invalid required profiles still cause failure.

Angular reports must retain eligible application sources under their explicit
inclusion patterns. Test-source, declaration, and runner-configuration omissions
retain exact source and rule limits. An Angular project whose builder does not
execute retains a separate limit. Its documented inclusion option requires a test
run. Such a limit does not establish measured zero coverage.

The receipt distinguishes native baselines, observed sources, execution strategy,
and local cache status. It links exact reports, profiles, configured inputs, and
limits. The command excludes generated sources, GraphQL documents, and Angular
HTML records. Eligible clients and neighboring application code remain included.
The JavaScript validator preserves its input when required output fails. Low and zero percentages add
no failure. Native collector attribution and lowered branch semantics remain in effect.

Run command and adapter checks:

```bash
nix develop --command aspect test //tools/coverage:javascript_report_test //tools/coverage:vitest_adapter_test
```

Command tests use controlled build-event and report inputs with the real native
baseline collector and Bazel merger. These tests establish validation behavior.
Real coverage acceptance must establish measured execution separately.

Run real configured acceptance:

```bash
nix develop --command bazel run //tools/coverage:javascript_membership_test -- --execution configured --evidence /absolute/new/private/directory
```

The acceptance command creates ordinary and hidden packages. It checks native
TypeScript attribution or its exact original-manifest limit, native Node children,
Angular children, disconnected sources,
and preservation of other language records. Local mode clears execution, cache,
and BES endpoints. Configured mode retains execution and cache evidence.
The existing hosted workflow runs the same command and retains its evidence.

## Browser and Astro sources

Browser targets use the shared JavaScript test declaration with `browser = True`.
The configured metadata identifies these targets without a project list.
The Astro builds request Vite source maps only in the coverage configuration.
The documented [extension resolution option](https://vite.dev/config/shared-options.html#resolve-extensions)
selects maintained TypeScript before adjacent Bazel-emitted JavaScript.
This lets Vite map original inputs without a custom source-map transformation.
Ordinary builds retain their existing output configuration.

The browser harness uses the documented
[Playwright Chromium coverage API](https://playwright.dev/docs/api/class-coverage).
The maintained [v8-to-istanbul converter](https://github.com/istanbuljs/v8-to-istanbul)
and Istanbul reporter supply source mapping, executable records, and branch semantics.
The adapter adds no compiler transformation or original branch identities.

Each page retains its raw profile, built scripts, source maps, converted report,
and source receipt in Bazel's undeclared test outputs.
The command reads individual Bazel outputs. It also accepts archived outputs.
The report command checks required files, profile ranges, map contents, and
report membership before it changes the combined report.
Missing or invalid required output causes failure.

Original attribution requires one unambiguous inventory path and exact original
source content in the supplied map. Supported original records merge with
existing language records. Generated and external records remain outside the
combined report. The raw archive retains their mapping evidence.

Each absent Astro source receives its own inventory owners and evidence location
in `coverage-javascript/receipt.json`. Browser reports omit Astro's build-time template execution. An unavailable original map or disconnected
original remains a collector limit. That limit leaves coverage unknown.
The adapter supplies no processed-script bridge or replacement source map.
