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
execution filters. The inventory also reports native Rust, Java/Kotlin,
JavaScript/TypeScript, and Astro sources. Their remaining collector integrations
belong to tickets #2505 through #2508. Measurement requires collector evidence.
Pending integrations supply no accepted collector limits.
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

## Go sources

Gazelle uses shared declarations for Go libraries, executables, and tests.
Each declaration adds independent metadata. Discovery selects that metadata
through Bazel ownership, including hidden and disconnected packages.
The added roots do not compile packages or execute tests.
The coverage build filter includes their `go-coverage` tag.
Existing test filters retain their behavior.

The [pinned rules_go baseline builder](https://github.com/bazel-contrib/rules_go/blob/f5a8f6a338fb9d7862f0f2247bec6da17711e392/go/tools/builders/baseline_coverage.go)
uses the SDK cover tool and coverage metadata to obtain real executable lines. Internal and external test sources
use that same builder. Build constraints exclude sources for the selected
configuration. Declaration-only sources retain the builder's empty executable
set. Baselines and measured reports use the native Bazel LCOV merger.
The report command applies the inventory's Go exclusions after native merging.
It checks Go executable records and their line totals before HTML generation.
Other language records retain their contents.
Go contributes its maintained line model. This integration adds no branch records.

The pinned collector patch rejects missing profiles, empty profiles, invalid
modes, corrupt records, and scanner failures. It propagates profile-generation
errors and report-write errors. A valid header with no executable records remains
valid. The patch does not change line attribution or execution counts.

Run the real collector checks and command acceptance:

```bash
nix develop --command aspect test //tools/go:collector_test
nix develop --command bazel run //tools/coverage:go_membership_test
```

The collector checks use subprocesses with separate output directories.
They check invalid required output and known measured records through the
[maintained converter](https://github.com/bazel-contrib/rules_go/blob/f5a8f6a338fb9d7862f0f2247bec6da17711e392/go/tools/bzltestutil/lcov.go). Command acceptance checks ordinary and hidden packages,
internal and external test sources, declarations, and build constraints.
It also checks measured tests, a separate Go executable, report preservation,
and required-profile failures after successful functional tests.
Pass `--evidence <directory>` after `--` to retain commands, inputs, and reports.
The existing hosted workflow runs this acceptance and retains its evidence.
