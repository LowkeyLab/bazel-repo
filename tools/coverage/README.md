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

This first integration selects Python production baselines. The inventory also
reports eligible Python tests, helpers, Go, native Rust, Java/Kotlin,
JavaScript/TypeScript, and Astro sources. Their remaining collector integrations
belong to tickets #2503 through #2508. Measurement requires collector evidence.
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
