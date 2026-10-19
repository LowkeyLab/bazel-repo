# Classify remaining maintained executable sources for coverage

Date: 2026-10-19. Repository base: `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
Research ticket: [Classify remaining maintained executable sources for coverage](https://github.com/LowkeyLab/bazel-repo/issues/2444).

## Findings

The remaining executable families need separate collection boundaries. They include shell, build evaluators, Nix, database statements, Askama templates, GraphQL operations, and Caddy routing directives.
A source declaration or passing application test does not establish original-source line coverage for these families.
This report approves no specific exception. Each unproven case remains eligible until evidence establishes its treatment.

The accepted [membership decision][membership] includes production code, test code, and executable helpers.
The accepted [investigation decision][investigation] requires bounded investigations before exceptions.
This report preserves the Python architecture, combined LCOV workflow, thresholds, and Codecov policy.
It does not implement a collector or repeat the named language investigations.
The completed [mechanism report][prior-mechanisms] and [evidence report][prior-evidence] supply the initial baseline and report-completeness findings.
This report adds the remaining-family inventory and precise missing experiments.

## Evidence and inventory method

The pool refreshed `origin/main` before acquisition. The resolved base appears above.
The source audit used tracked paths, file contents, executable modes, source declarations, and Bazel loading dependencies.
Executable mode alone is insufficient. For example, `maven_install.json` carries executable mode but contains dependency metadata.
The audit checked extensionless scripts, hidden packages, embedded commands, templates, and DSLs separately.

The following query completed with exit 0:

```bash
nix develop --command bazel query 'deps(set(//... //.github/...))' --output=xml
```

[Bazel query invocation](https://app.buildbuddy.io/invocation/1ed5c7d2-113f-4158-8e33-88385fab0869).
A second successful query examined `buildfiles(set(//... //.github/...))`.
Both queries inspect the unconfigured graph. They establish loading and declaration evidence, not configured coverage providers or runtime collection.
The source-owner table below uses direct main-repository rule inputs. Loading dependencies appear separately.
An exported file label alone does not establish an executable owner or a collector.

The audit respected [`.bazelignore`][ignore] and existing Git ignore boundaries.
It did not traverse `node_modules`, `target`, `.worktrees`, `.claude`, `.direnv`, or `.local` as maintained source roots.
Tracked IDE metadata remains configuration data. The audit did not treat ignored generated working files as maintained sources.
Future discovery must also inspect eligible untracked files through an ignore-aware scan. A tracked-file audit alone cannot establish that guarantee.

The table accounts for every tracked suffix at this base. Counts include maintained code, generated files, and fixture data before semantic classification.
These counts are an audit snapshot. They must not become a central inclusion list.

| Suffix        | Count | Classification boundary                                                                                             |
| ------------- | ----: | ------------------------------------------------------------------------------------------------------------------- |
| `(none)`      |    92 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.astro`      |    19 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.axl`        |     3 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.bazel`      |   126 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.bazelrc`    |     2 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.bzl`        |    15 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.cc`         |     1 | Empty C++ generator fixture. It contains no executable statements. See the fixture evidence below.                  |
| `.clj`        |     2 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.conf`       |     3 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.css`        |    17 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.edn`        |     1 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.example`    |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.go`         |    65 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.graphql`    |     6 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.html`       |    50 | Angular templates, Askama templates, browser scripts, and static markup require separate treatment.                 |
| `.ico`        |     3 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.iml`        |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.in`         |    85 | Inspect fixture or patch role. A suffix alone does not establish execution or exclusion.                            |
| `.ini`        |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.jpeg`       |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.js`         |     2 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.json`       |    43 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.kt`         |    31 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.lock`       |    14 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.md`         |   134 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.mjs`        |     5 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.mod`        |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.nix`        |     3 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.out`        |   105 | Inspect fixture or patch role. A suffix alone does not establish execution or exclusion.                            |
| `.patch`      |    14 | Inspect fixture or patch role. A suffix alone does not establish execution or exclusion.                            |
| `.pem`        |     2 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.proto`      |     2 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.py`         |    20 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.rs`         |   463 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.sh`         |     7 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.snap`       |    71 | Inspect fixture or patch role. A suffix alone does not establish execution or exclusion.                            |
| `.sql`        |    19 | Inspect executable statements and declarations. Remaining families appear below.                                    |
| `.sum`        |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.svg`        |     2 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.swagger_ui` |     1 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.toml`       |    41 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.ts`         |   170 | Named language investigations. Preserve maintained code and distinguish generated outputs and inert fixture inputs. |
| `.txt`        |    17 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.woff2`      |     4 | Data, styles, documentation, fonts, dependency metadata, or other non-program assets in this snapshot.              |
| `.xml`        |     9 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.yaml`       |     9 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |
| `.yml`        |    15 | Inspect embedded programs and directives. Ordinary metadata alone supplies no executable statement.                 |

There are 124 `BUILD.bazel` files and one `MODULE.bazel` file.
The remaining `.bazel` path belongs to a fixture. Build declarations execute during loading or analysis, outside ordinary test-runtime collectors.

### Maintained code under third-party paths

Do not exclude `3rdparty/` by prefix alone.
The [local Gazelle guide][gazelle-guide] explicitly calls its copied tree locally maintained ordinary monorepo packages.
Its maintained generator, parser, tests, and build macros remain candidates under the accepted policy.
Generated protobuf bindings remain generated outputs. The maintained [protocol schema][schema] remains an original declaration source.

The sole `.cc` path is `generation_tests/existing_rule/foobar.cc` under that tree.
That file is empty. It contains no executable statements.
Its [input BUILD][cc-input] and [expected BUILD][cc-output] preserve the same C++ rule while changing the Rust rule.
The [generation macro][generation] supplies fixture paths through `test_data` to `gazelle_generation_test`.
The root query does not identify a compiled C++ target for this fixture.
The harness consumes this file as fixture data and compares generated BUILD text.
The harness does not compile this empty C++ file.
This establishes non-executable fixture data for this path under the accepted membership policy.
It does not support excluding every maintained fixture or every C++ source by suffix.

The second `.proto` path, `generation_tests/prost/foo.proto`, is a generator fixture.
The maintained protocol schema defines messages and fields.
The [Protobuf language guide][proto-language] distinguishes schema definitions from generated runtime code.
These message declarations need no invented executable-line baseline. Generated language execution belongs to the named language collectors.
A future custom option, embedded program, or changed execution path requires another classification.

## Capability matrix

“Candidate” means source-level feasibility only. It does not mean repository integration or demonstrated runtime behavior.
No new coverage experiment ran during this research.

| Family                              | Owners and execution path                                                                         | Collector candidate                                                         | Zero-hit and branch boundary                                                                                                      | Required adapter or evidence                                                                                 |
| ----------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Shell files and extensionless shell | Five declared binaries/tests, plus hooks, publication, and workspace-status commands              | kcov Bash engine or a Bash tracing adapter                                  | kcov supports script discovery. Exact executable lines, multiline commands, child tracing, and branch records need fixtures.      | Hermetic provisioning, inventory-fed baseline, child receipts, LCOV conversion, original path mapping        |
| Embedded shell                      | Workflow `run`, BUILD `cmd`, Nix strings, package scripts, Renovate commands, Dockerfile `RUN`    | Same shell engine after extraction                                          | Generated temporary scripts need original container offsets. Unexecuted blocks need parser-backed baselines.                      | Preserve container path, source ranges, interpolation boundaries, and child collection                       |
| Bazel Starlark                      | BUILD, MODULE, maintained `.bzl`, loading and analysis callbacks                                  | Evaluator instrumentation remains unproven                                  | CPU profiles do not establish statement hits, disconnected baselines, or branches. Cached loading can suppress events.            | Parser-backed source ranges and evaluator hit events across phases                                           |
| Aspect Starlark                     | Three `.axl` files, Aspect config and Gazelle extension execution                                 | `starlark` 0.14.2 `ProfileMode::Coverage` is a candidate                    | Coverage returns executed source spans. Unexecuted spans and branch dimensions need separate evidence.                            | Aspect evaluator hooks, span-to-line export, independent baseline, and output receipts                       |
| Nix                                 | Three original `.nix` files, flake evaluation and derivation construction                         | Evaluator tracing or a source instrumenter remains unproven                 | Lazy evaluation and cached evaluation complicate hits. Function traces and sampled profiles cannot establish all statement lines. | Exact evaluator version, expression locations, forcing contract, baseline, and embedded shell mapping        |
| PostgreSQL SQL and PL/pgSQL         | Nineteen SQL files, Rust compile data, Go embedded migrations, SQL generation input               | SQL statement-location adapter. `plpgsql_check` is a procedural candidate.  | Statement execution is not branch execution. Anonymous blocks, DDL, multiline spans, and rollback paths need evidence.            | Database-side receipts and original file offsets. Preserve unused statement baselines.                       |
| Askama templates                    | Sixteen `.html` files through `//nicknamer/server/lib:templates` and Rust `compile_data`          | Askama generated-code mappings or template instrumentation remains unproven | Rust method hits do not prove template lines, untaken arms, or unused templates.                                                  | Template AST locations, include identities, generated-code mapping, and original baselines                   |
| GraphQL operations                  | Six original `.graphql` documents without direct rule-input ownership                             | Operation AST plus request instrumentation remains unproven                 | These are executable query/mutation documents. Type generation alone does not prove execution.                                    | Declare ownership, preserve operation/selection locations, link observations across client/server boundaries |
| Caddy routing directives            | Four Caddyfiles in declared package layers                                                        | Directive parser plus request execution hooks remains unproven              | Route selection is behavior. This research established no original-source line collector.                                         | Source-range model, untouched route baseline, ordered route observation, and receipts                        |
| Static declarations and assets      | Protobuf message declarations, ordinary JSON/TOML metadata, fonts, images, CSS, and static markup | No executable-line collector required for proved non-program content        | Classify through syntax and language role. Do not accept arbitrary empty baselines.                                               | Detect embedded programs before assigning this classification                                                |

### Pinned and official source evidence

The repository pins `rules_shell` to `6ce926fa2be0c94b06ef65eba8d2f989676a26a3`.
Its [executable implementation][shell-impl] exports `instrumented_files_info` for `srcs` and selected dependencies.
Its [test implementation][shell-test] wires the C/C++ coverage helper.
Neither inspected path adds a Bash statement collector or an executable-line baseline.
The rule metadata therefore does not establish shell coverage.

The [kcov manual][kcov] documents Bash tracing, script discovery, child-tracing controls, and path replacement.
It also documents Cobertura output. The inspected source tree supplies no LCOV writer.
A candidate integration therefore needs a tested conversion or exporter for the existing LCOV merge.
The candidate revision is `39bda2f4f6fe04529c3479c6494acb6bc78a0857`. The repository does not currently pin kcov.

The [flake][flake] pins Aspect source to `66298af52ed870fb6651eedc0d8ee2a0762caab6`.
Its [Cargo lock][aspect-lock] selects `starlark` 0.14.2. The Nix package also applies an upstream Starlark patch.
The pinned crate source exposes coverage profiling and executed source spans.
The inspected file is `starlark-0.14.2/src/eval/runtime/evaluator.rs`, lines 376, 432, and 450–454.
The [crate archive][starlark] checksum is `9062e866918dc4c9701c98ac99f7f4fa9e4b3b4edce306e147393bc75458c4fc`.
The local Cargo vendor directory supplied this source. The versioned online API page was unavailable during this inspection.
This establishes a candidate API. It does not establish that the patched Aspect evaluator enables or exports it.
Bazel uses its own evaluator. An Aspect API does not establish Bazel support.
The [Bazel profiling guide][bazel-profile] describes profiling data, not a demonstrated original-source coverage adapter.

The host reports Nix 2.34.9. The repository flake pins its inputs, but it does not pin the caller's Nix executable.
The [Nix evaluator profiler][nix-profile] samples function call stacks.
The [Nix evaluation model][nix-evaluation] uses lazy evaluation.
Neither fact supplies an exhaustive executable-line inventory or per-line hit counts.
Nix source evaluation and shell builders require separate observations.

The test image uses PostgreSQL 18. The matching [statement statistics documentation][pg-statements] exposes query statistics, not original repository file identities.
The [plpgsql_check profiler][plpgsql] exposes procedural statement and branch metrics.
Its candidate revision is `0ac96adc13b34d071c4be8ba0ec4f73e6e7520f5`. The repository does not currently provision it.
These APIs do not establish coverage for ordinary SQL files or the repository's anonymous `DO` block.
That block appears in [application login migration][login-migration]. A dedicated experiment must settle its mapping.

The [GraphQL specification][graphql-spec] calls operation definitions executable definitions.
The six maintained documents contain queries or mutations, not merely type declarations.
The lockfile selects Askama 0.16.1. The [Askama documentation][askama] describes conditional, loop, and include constructs.
Those general syntax documents do not establish the pinned macro implementation or its mappings.
The [Nicknamer template][template-example] uses these constructs and receives data through Rust render calls.
The [Caddy directive documentation][caddy] describes ordered HTTP handler execution.
These sources prevent a blanket “configuration has no executable lines” classification.
They do not establish a working collector for any of these original DSL files.

## Exact declaration inventory

The table lists every remaining standalone source candidate for shell, Starlark helpers, Aspect, Nix, SQL, GraphQL, Askama, and Caddy.
BUILD and MODULE declarations form the separate evaluator family described above.
The table is evidence for this revision, not an implementation allowlist.
“Loaded” records a Bazel build-file dependency without a direct source-owner declaration.
“None” means no direct main-repository rule input appeared in the queried graph.
It does not mean that external tools never execute the file.

| Source                                                                                                                                                                                                                                       | Direct rule inputs or loading evidence                                           |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [`.aspect/config.axl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/.aspect/config.axl)                                                                                                             | None                                                                             |
| [`.aspect/gazelle/shell.axl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/.aspect/gazelle/shell.axl)                                                                                               | None                                                                             |
| [`.envrc`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/.envrc)                                                                                                                                     | None                                                                             |
| [`.github/scripts/publish-nix-tool.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/.github/scripts/publish-nix-tool.sh)                                                                           | None                                                                             |
| [`3rdparty/gazelle_rust/upstream/generation_tests/generation_tests.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/generation_tests/generation_tests.bzl)         | Loaded, no direct source declaration                                             |
| [`3rdparty/gazelle_rust/upstream/proto/messages.proto`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/proto/messages.proto)                                           | `//3rdparty/gazelle_rust/upstream/proto:messages_proto`                          |
| [`Dockerfile`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/Dockerfile)                                                                                                                             | None                                                                             |
| [`angular/bzl/ng.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/bzl/ng.bzl)                                                                                                             | `//angular/bzl:ng`                                                               |
| [`angular/projects/mindreadr/Caddyfile`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/mindreadr/Caddyfile)                                                                         | `//angular/projects/mindreadr:caddyfile_layer`                                   |
| [`angular/projects/nicknamer2-web/src/app/graphql/create-name.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/create-name.graphql)           | None                                                                             |
| [`angular/projects/nicknamer2-web/src/app/graphql/create-names.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/create-names.graphql)         | None                                                                             |
| [`angular/projects/nicknamer2-web/src/app/graphql/create-server.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/create-server.graphql)       | None                                                                             |
| [`angular/projects/nicknamer2-web/src/app/graphql/get-dashboard.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/get-dashboard.graphql)       | None                                                                             |
| [`angular/projects/nicknamer2-web/src/app/graphql/get-server-names.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/get-server-names.graphql) | None                                                                             |
| [`angular/projects/nicknamer2-web/src/app/graphql/get-servers.graphql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/nicknamer2-web/src/app/graphql/get-servers.graphql)           | None                                                                             |
| [`angular/projects/pinyin-composer/Caddyfile`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/pinyin-composer/Caddyfile)                                                             | `//angular/projects/pinyin-composer:caddyfile_layer`                             |
| [`angular/projects/predix/Caddyfile`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/angular/projects/predix/Caddyfile)                                                                               | `//angular/projects/predix:caddyfile_layer`                                      |
| [`book_smartz/migrations/001_initial.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/migrations/001_initial.sql)                                                                     | `//book_smartz/storage:lib`                                                      |
| [`book_smartz/migrations/002_application_history.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/migrations/002_application_history.sql)                                             | `//book_smartz/storage:lib`                                                      |
| [`book_smartz/web/actions.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/web/actions.sql)                                                                                           | `//book_smartz/web:bin`                                                          |
| [`book_smartz/web/catalog.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/web/catalog.sql)                                                                                           | `//book_smartz/web:bin`                                                          |
| [`book_smartz/web/identity.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/web/identity.sql)                                                                                         | `//book_smartz/web:bin`                                                          |
| [`bzl/kotlin.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/bzl/kotlin.bzl)                                                                                                                     | `//bzl:kotlin`                                                                   |
| [`flake.nix`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/flake.nix)                                                                                                                               | None                                                                             |
| [`githooks/check-config.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/githooks/check-config.sh)                                                                                                 | None                                                                             |
| [`githooks/pre-commit`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/githooks/pre-commit)                                                                                                           | None                                                                             |
| [`mindreadr/src/test/io/lowkeylab/mindreadr/app/rules.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/mindreadr/src/test/io/lowkeylab/mindreadr/app/rules.bzl)                                   | `//mindreadr/src/test/io/lowkeylab/mindreadr/app:rules`                          |
| [`nicknamer/run_locally.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/run_locally.sh)                                                                                                 | `//nicknamer:run_locally`                                                        |
| [`nicknamer/server/lib/templates/index.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/index.html)                                                               | None                                                                             |
| [`nicknamer/server/lib/templates/layout.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/layout.html)                                                             | None                                                                             |
| [`nicknamer/server/lib/templates/login.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/login.html)                                                               | None                                                                             |
| [`nicknamer/server/lib/templates/login/login_error_message.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/login/login_error_message.html)                       | None                                                                             |
| [`nicknamer/server/lib/templates/login/login_success.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/login/login_success.html)                                   | None                                                                             |
| [`nicknamer/server/lib/templates/names.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names.html)                                                               | None                                                                             |
| [`nicknamer/server/lib/templates/names/add_name_form.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/add_name_form.html)                                   | None                                                                             |
| [`nicknamer/server/lib/templates/names/bulk_add_form.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/bulk_add_form.html)                                   | None                                                                             |
| [`nicknamer/server/lib/templates/names/bulk_add_success.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/bulk_add_success.html)                             | None                                                                             |
| [`nicknamer/server/lib/templates/names/bulk_delete.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/bulk_delete.html)                                       | None                                                                             |
| [`nicknamer/server/lib/templates/names/bulk_delete_table.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/bulk_delete_table.html)                           | None                                                                             |
| [`nicknamer/server/lib/templates/names/edit_name_form.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/edit_name_form.html)                                 | None                                                                             |
| [`nicknamer/server/lib/templates/names/error_message.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/error_message.html)                                   | None                                                                             |
| [`nicknamer/server/lib/templates/names/name_row.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/name_row.html)                                             | None                                                                             |
| [`nicknamer/server/lib/templates/names/names_table.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/names_table.html)                                       | None                                                                             |
| [`nicknamer/server/lib/templates/welcome/call_to_action.html`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/welcome/call_to_action.html)                             | None                                                                             |
| [`nicknamer2/migrations/001_create_names_table.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer2/migrations/001_create_names_table.sql)                                                 | `//nicknamer2/migrations:migrations`                                             |
| [`nicknamer2/migrations/002_create_servers_table.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer2/migrations/002_create_servers_table.sql)                                             | `//nicknamer2/migrations:migrations`                                             |
| [`nix/checks/aspect/default.nix`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nix/checks/aspect/default.nix)                                                                                       | None                                                                             |
| [`nix/packages/aspect/default.nix`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nix/packages/aspect/default.nix)                                                                                   | None                                                                             |
| [`personal_website/Caddyfile`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/Caddyfile)                                                                                             | `//personal_website:caddyfile_layer`, `//personal_website:personal_website_test` |
| [`personal_website/bzl/ts.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/bzl/ts.bzl)                                                                                           | `//personal_website/bzl:ts`                                                      |
| [`personal_website/site_structure_test.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/site_structure_test.sh)                                                                   | `//personal_website:site_structure_test`                                         |
| [`prediction_bot/migrations/001_event_store.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/prediction_bot/migrations/001_event_store.sql)                                                       | `//prediction_bot/lib:lib`                                                       |
| [`prediction_bot/migrations/002_application_login.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/prediction_bot/migrations/002_application_login.sql)                                           | `//prediction_bot/lib:lib`                                                       |
| [`prediction_bot/migrations/003_announcements.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/prediction_bot/migrations/003_announcements.sql)                                                   | `//prediction_bot/lib:lib`                                                       |
| [`predix/internal/migrations/000001_init_schema.down.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000001_init_schema.down.sql)                                     | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000001_init_schema.up.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000001_init_schema.up.sql)                                         | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000002_add_expires_at.down.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000002_add_expires_at.down.sql)                               | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000002_add_expires_at.up.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000002_add_expires_at.up.sql)                                   | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000003_change_house_rake_to_real.down.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000003_change_house_rake_to_real.down.sql)         | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000003_change_house_rake_to_real.up.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000003_change_house_rake_to_real.up.sql)             | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000004_add_uuid_to_users.down.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000004_add_uuid_to_users.down.sql)                         | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/migrations/000004_add_uuid_to_users.up.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/migrations/000004_add_uuid_to_users.up.sql)                             | `//predix/internal/migrations:migrations`                                        |
| [`predix/internal/sql/query.sql`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/predix/internal/sql/query.sql)                                                                                       | None                                                                             |
| [`tools/aspect/delivery_flags.axl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/aspect/delivery_flags.axl)                                                                                   | None                                                                             |
| [`tools/book_smartz_browser_worker/runtime_test.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/book_smartz_browser_worker/runtime_test.sh)                                                 | `//tools/book_smartz_browser_worker:runtime_test`                                |
| [`tools/cargo_lock.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/cargo_lock.bzl)                                                                                                         | Loaded, no direct source declaration                                             |
| [`tools/coverage/coverage.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/coverage/coverage.sh)                                                                                             | `//tools/coverage:coverage`                                                      |
| [`tools/go_mod_download.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/go_mod_download.bzl)                                                                                               | Loaded, no direct source declaration                                             |
| [`tools/lint/linters.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/lint/linters.bzl)                                                                                                     | `//tools/lint:linters`                                                           |
| [`tools/python/defs.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/python/defs.bzl)                                                                                                       | `//tools/python:defs`                                                            |
| [`tools/python/uv_lock.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/python/uv_lock.bzl)                                                                                                 | Loaded, no direct source declaration                                             |
| [`tools/test_images/defs.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/test_images/defs.bzl)                                                                                             | Loaded, no direct source declaration                                             |
| [`tools/topcoat/defs.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/topcoat/defs.bzl)                                                                                                     | Loaded, no direct source declaration                                             |
| [`tools/topcoat/format.bzl`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/topcoat/format.bzl)                                                                                                 | Loaded, no direct source declaration                                             |
| [`tools/workspace_status.sh`](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/workspace_status.sh)                                                                                               | `//tools:workspace_status`                                                       |

### Embedded source inventory

The following containers require extraction or explicit language classification:

- `.github/workflows/*.yml` and `.github/actions/install-nix-tool/action.yml` contain shell `run` blocks and expression substitutions.
- `.github/actions/setup-bazel-ci/action.yml` configures external actions. Its embedded bazelrc value is command configuration, not a Bash block.
- `BUILD.bazel` files contain generator `cmd` strings and generated entrypoint declarations.
- `tools/topcoat/format.bzl` constructs a shell launcher. The policy excludes the generated file. Maintained launcher statements need original-source treatment.
- `flake.nix` and both `nix/**/default.nix` files contain shell strings for builders and checks.
- `Dockerfile` contains `RUN` commands. `.envrc` and `githooks/pre-commit` contain extensionless shell programs.
- `angular/package.json` and `personal_website/package.json` contain package scripts. `renovate.json` contains post-upgrade commands.
- `.pre-commit-config.yaml` and `.vscode/tasks.json` declare tool invocations. Classify orchestration values separately from embedded program statements.
- `ktor_tutorial/resources/static/index.html` contains an inline browser program. It belongs to browser collection with original HTML offsets.
- Angular `.html` templates belong to the existing Angular investigation. Askama `.html` templates require the separate mapping experiment below.
- SQL strings inside maintained Rust and Go remain a database execution boundary. Parent-language line hits do not establish database statement execution.

External JavaScript loaded through `<script src>` remains external dependency code.
Static HTML and CSS do not acquire executable-line requirements merely because a browser consumes them.
Executable template expressions and inline scripts require their own source ranges.
No rule here excludes a maintained program because its path contains `fixtures`, `test`, or `3rdparty`.

## Smallest remaining experiments

Every experiment must preserve raw collector output and combined LCOV at an exact revision.
Each must test a disconnected source, an executed source, and missing or corrupt output.
Each must compare canonical repository-relative paths and preserve positive hits after baseline merging.
Require branch records only when source evidence and execution establish collector support.
Fresh remote execution and hosted upload remain later acceptance evidence.

### Shell and embedded shell

Provision one exact Bash collector under Bazel. Use one script, one sourced helper, and one child-only script.
Include a multiline conditional and an eligible script outside all selected test dependencies.
Extract one YAML shell block and one generated launcher with known original offsets.
Require positive and zero-hit executable records at original file locations. Check whether supported branch records survive conversion.
Remove the child output while functional assertions pass. Require a visible collection failure.
This experiment must also settle interpolation and source-range identities for container files.

### Bazel and Aspect Starlark

Use one maintained `.bzl` function and one `.axl` function with executed and untaken statements.
Enable evaluator coverage hooks independently for Bazel and the pinned patched Aspect build.
Include an unloaded source for baseline discovery and a cached loading run.
Require original paths, parser-backed zero-hit locations, and explicit evidence for unavailable hit events.
Remove one evaluator output. Require failure rather than a filename-only record.
This experiment may establish different mechanisms for Bazel and Aspect.

### Nix

Fix the Nix evaluator version before testing. Use one forced expression, one lazy unforced expression, and one disconnected source.
Include one builder shell block with known original offsets.
Compare evaluator events with independently parsed executable locations.
Require positive and zero-hit records without forcing expressions solely to manufacture coverage.
Remove one expected evaluator output. Require failure.
A sampled profile or successful derivation is insufficient evidence.

### PostgreSQL SQL and PL/pgSQL

Use one original SQL file with multiline DDL, two ordinary statements, and an anonymous procedural block.
Execute a subset through the real database process. Leave one statement and one procedural arm unexecuted.
Include an unused SQL file and duplicate execution from two application tests.
Evaluate statement instrumentation and the pinned procedural candidate separately.
Require original file offsets, real zero-hit locations, supported branches, canonical merging, and missing database-output failure.
The experiment must distinguish generated Go SQL strings from maintained original query sources.

### Askama original templates

Use the pinned Askama dependency with an `if`, a loop, an include, and one unused template.
Render only one branch through the normal Rust test path.
Inspect LLVM mappings before proposing template instrumentation.
Require the original `.html` paths and executable locations. A hit on `render()` is insufficient.
If mappings omit template locations, test one minimal AST-based instrumentation adapter.
Remove its output while Rust assertions pass. Require failure.

### GraphQL executable operations

Declare one maintained original query document as a Bazel input. Keep a second document outside selected test dependencies.
Use a variable, a conditional directive, and a fragment with known source locations.
Execute the first operation through the maintained client and server boundary.
Determine whether operation dispatch, selected fields, and directives supply usable original-location events.
Require a parser-backed zero-hit document and missing-output failure.
Do not substitute generated TypeScript coverage for original operation evidence.
The experiment must state which executable locations and supported branch dimensions the collector measures.

### Caddy routing directives

The tool lock selects Caddy 2.11.7. Use that runtime with ordered handlers, one redirect, one conditional matcher, and one unused route.
Send one request through the normal packaged execution path.
Compare adapted configuration locations with request execution events.
Determine whether an adapter can produce original directive locations without inventing line hits from access logs.
Require an untouched-route baseline and missing-output failure.
The later human decision must settle the executable-location model if language evidence alone cannot settle it.

Protobuf message declarations and ordinary metadata need no runtime experiment after syntax checks establish that no embedded program exists.
SQL, GraphQL operations, Askama constructs, and Caddy handlers cannot receive that classification solely because they use declarative syntax.

## Cross-family handoff and limits

The Node/browser investigations must cover inline browser scripts in original HTML and generated launchers that execute maintained Node code.
The Rust investigation must distinguish Askama template locations from generated Rust method locations.
The Python, Go, and Rust process investigations must preserve database and shell child boundaries.
They must not claim original SQL or shell coverage from parent-language hits.
The ownership implementation must include load-only build files and sources consumed by tools outside Bazel.
It must not require unrelated deployment artifacts merely to discover their maintained source inputs.

No proposed exception appears in this report. The seven experiments define missing evidence before any exception decision.
The only executed operations here were source inspection, inventory commands, Bazel queries, and report checks.
No new collector, coverage fixture, remote worker coverage, or hosted Codecov upload ran.
The query invocation is graph evidence, not coverage evidence.

## Reproduction and report checks

The graph query, build-file query, tracked-path inventory, and source inspection used the exact base above.
Raw graph evidence and command output remain in the task's private receipt directory.
The report's inventory table preserves direct ownership findings for the remaining source candidates.
Formatting, lint, and whitespace check results appear in the ticket resolution with their exact commands.

[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[ignore]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/.bazelignore
[gazelle-guide]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/README.md
[schema]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/proto/messages.proto
[cc-input]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/generation_tests/existing_rule/BUILD.in
[cc-output]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/generation_tests/existing_rule/BUILD.out
[generation]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/3rdparty/gazelle_rust/upstream/generation_tests/generation_tests.bzl
[proto-language]: https://protobuf.dev/programming-guides/proto3/
[shell-impl]: https://github.com/bazel-contrib/rules_shell/blob/6ce926fa2be0c94b06ef65eba8d2f989676a26a3/shell/private/sh_executable.bzl
[shell-test]: https://github.com/bazel-contrib/rules_shell/blob/6ce926fa2be0c94b06ef65eba8d2f989676a26a3/shell/private/sh_test.bzl
[kcov]: https://github.com/SimonKagstrom/kcov/blob/39bda2f4f6fe04529c3479c6494acb6bc78a0857/doc/kcov.1
[flake]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/flake.nix
[aspect-lock]: https://github.com/aspect-build/aspect-cli/blob/66298af52ed870fb6651eedc0d8ee2a0762caab6/Cargo.lock
[starlark]: https://static.crates.io/crates/starlark/starlark-0.14.2.crate
[bazel-profile]: https://bazel.build/advanced/performance/cpu
[nix-profile]: https://nix.dev/manual/nix/2.34/advanced-topics/eval-profiler.html
[nix-evaluation]: https://nix.dev/manual/nix/2.34/language/evaluation
[pg-statements]: https://www.postgresql.org/docs/18/pgstatstatements.html
[plpgsql]: https://github.com/okbob/plpgsql_check/blob/0ac96adc13b34d071c4be8ba0ec4f73e6e7520f5/README.md#profiler
[login-migration]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/prediction_bot/migrations/002_application_login.sql
[graphql-spec]: https://spec.graphql.org/September2025/#ExecutableDefinition
[askama]: https://askama.readthedocs.io/en/stable/template_syntax.html
[template-example]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/nicknamer/server/lib/templates/names/names_table.html
[caddy]: https://caddyserver.com/docs/caddyfile/directives
[prior-mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[prior-evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
