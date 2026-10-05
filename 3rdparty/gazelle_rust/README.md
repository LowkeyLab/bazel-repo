# Locally maintained Gazelle Rust

The `upstream/` directory contains ordinary monorepo packages copied from
[Calsign/gazelle_rust](https://github.com/Calsign/gazelle_rust) at commit
`747482bfad721b0fe7198c94491d76c32b263ac0`. Its tracked source tree was obtained
with `git archive`; its licence remains at `upstream/LICENSE`.

## Integration decision

[Issue #1969](https://github.com/LowkeyLab/bazel-repo/issues/1969) supersedes the
nested-module and independent-toolchain boundary in the
[baseline design](../../docs/superpowers/specs/2026-10-04-gazelle-rust-local-baseline-design.md).
Generator, parser, generated protobuf bindings, attribute macros, and enabled
tests belong to the root build graph. Rust uses the root `rules_rs` toolchain,
including its provider-compatible Prost bridge; Go uses the root SDK and Nogo.
Cargo dependencies live in the root manifest and lockfile, including the existing
`cargo-bazel` 0.18.0 lockfile API. Prost's compiler plugins and matching runtime
come from the pinned root rules' shared Prost configuration. Generated bindings
compile with that shared Rust toolchain; Buf checks the maintained schema.
Clippy checks maintained Rust libraries, binaries, tests, and procedural macros.
The internal protocol's `gazelle.rust.v1` package and virtual import prefix satisfy
Buf's standard checks without changing field numbers or the binary wire format.

Go import paths and source directories retain their upstream names. Standalone
builds, releases, and example maintenance are retired. The removed example's
module traversal, library/binary dependencies, unit tests, integration tests,
and derive-macro dependencies are covered by the existing generation fixtures
(`unified/nested`, `unified/lib_bin`, `crate_tests`, and `cargo/dependencies`).
Its arithmetic and fixed greeting assertions did not add generator behavior.

## Local adaptations

The former `rules_rs_loads.patch` is incorporated directly:

- `rust_language/lang.go` generates rules_rs loads for common Rust rules and
  `cargo_build_script`, retaining the other upstream load families.
- `gazelle_rust_parser/src/BUILD.bazel` explicitly sets edition `2024`.

Additional test integration adaptations:

- `upstream/BUILD.bazel` explicitly includes `@gazelle//language/defaults` in
  the Rust-only fixture generator, avoiding a repository-relative implicit
  label in the current Gazelle macro.
- `gazelle_rust_parser/tests/parse_test.rs` resolves fixture data with the
  repository-aware runfiles macro, so it works under root package labels.
- Generation goldens use rules_rs for newly generated loads. Existing loads,
  rule attributes, dependency labels, and diagnostic expectations are retained.

The root `.gitattributes` preserves whitespace in upstream patch files because
patch context contains significant leading spaces and tabs.

The maintained implementation uses one crate-discovery pipeline, tracked in
[#1957](https://github.com/LowkeyLab/bazel-repo/issues/1957), against
[#1955](https://github.com/LowkeyLab/bazel-repo/issues/1955). The maintainer's
subsequent instruction requires library/binary roots named `lib.rs`/`main.rs`,
superseding the specification's arbitrary-filename root requirement. Integration
runners retain standard Rust test layouts; build scripts retain `build.rs`.

## Crate discovery

Configuration/root planning runs before Gazelle visits children. Each root is
traversed independently through Rust module declarations; ownership is shared
when multiple crates compile the same file. Ordinary unreferenced sources do
not become targets. Other Gazelle language plugins still run in owned directories.

Cargo manifests are optional. Virtual workspace manifests do not replace Bazel
crate discovery. Competing adjacent and `src/` roots require explicit root
configuration; Gazelle does not silently choose a layout.
Existing BUILD targets supply names, kinds,
features, edition, and other user configuration. Without manifests, `src/lib.rs`
and `src/main.rs` (or adjacent `lib.rs`/`main.rs`) establish conventional roots.
Without an explicit kind, a library root declaring function-like, attribute, or
derive procedural macros generates a `rust_proc_macro` rule.
Generated names use the package directory basename, replacing hyphens with
underscores for crate names. When library and binary coexist, the library target
gets `_lib` and retains the unsuffixed crate name. Collisions require an explicit
unique target name; there is no automatic `_rs` renaming.

Explicit `crate_root` wins; otherwise a single matching `lib.rs`/`main.rs` source
identifies the root. Multiple candidates require configuration. Unprotected
`srcs` are refreshed from the graph, and `# keep` remains authoritative. Existing
source globs are not a discovery boundary; generated source lists are explicit.

Configured dependencies on build scripts remain intact. Dependencies outside
managed packages or the configured external-crate prefix are retained
conservatively because discovery cannot establish their target kind.

Unit-test targets refer to their crate via `crate`; dependencies used only by
unit tests stay on that test target. Native shared/static libraries also receive
unit-test targets. A platform-transitioned library uses a native library companion
with the same root, crate name, and features for unit tests. Without a companion
or an explicitly configured test, Gazelle diagnoses the native test crate needed
instead of generating a host harness against foreign-target dependencies.
Dedicated integration runners use Cargo
manifest targets, explicit Bazel test roots, or `tests/*.rs` and
`tests/<name>/main.rs` containing tests/a main function. Candidate files owned
by another runner are support modules. Build scripts use `build.rs`.

Module traversal supports both external layouts, nested/inline modules, literal
`#[path]`, nested `cfg_attr` path selection, boolean predicates, configured
features, and test-only modules. Unknown platform choices retain possible paths
and carry their conditions through inline and external module traversal. Missing or
ambiguous modules invalidate the crate's generation; an existing rule is left
intact. Nested BUILD boundary crossings require layout/configuration repair,
rather than generating illegal parent-package paths. Exclusions suppress
inferred roots; explicitly configured crates still traverse their declarations,
including modules deliberately excluded from independent Gazelle discovery.

This is source-level discovery, not Cargo or rustc equivalence. Macro-generated
module declarations and generated crate roots need explicit build configuration.
The parser does not execute build scripts or expand Rust macros.

## Application migration

Twenty custom library/binary root files moved to conventional names. Target
labels and crate names were preserved, so dependent-label rewrites were not
needed. The Nicknamer2 single-file targets are intentional separately imported
crates, not redundant module targets. Existing grouped crates in Hearthstone,
Kafka Calculator, Book Smartz, Prediction Bot, and Nicknamer retain their crate
boundaries. No nested application BUILD file was removed. The subsequent
[#1960 cleanup](workarounds.md) removes redundant application exclusions and
records the narrow dependency configuration that remains necessary.

The exact source moves are recorded in [the migration inventory](migration.md).
Legacy generator fixtures now configure intentional roots explicitly and use
conventional filenames; resolver, alias, procedural-macro, and keep-comment
coverage remains enabled. New `unified/` fixtures exercise crate discovery.

## Development

Run all commands from the monorepo root:

```sh
nix develop --command bazel run //:gazelle
nix develop --command aspect format --scope=all
nix develop --command aspect test //3rdparty/gazelle_rust/upstream/generation_tests/... //3rdparty/gazelle_rust/upstream/gazelle_rust_parser/tests:parse_test //3rdparty/gazelle_rust/upstream/macro/tests:ignore_test
nix develop --command aspect build //...
nix develop --command aspect test //...
nix develop --command aspect lint
git diff --check
```

Root recursive checks discover the maintained packages and enabled tests.
The fixture binary remains Rust-only; the application generator retains all its
language plugins. The original unsupported `standard_unused_crates` fixture
remains manual. The attribute test compiles the existing annotated import as a
Rust test target, so recursive tests exercise that contract too.

Root Gazelle excludes generation fixture workspaces and malformed parser samples,
which remain data. Narrow `.gitattributes` protections keep those inputs, golden
outputs, and significant patch whitespace unchanged by formatting. Maintained
Go, Rust, Starlark, and documentation use normal repository formatting.

Before changing generation behavior, compare application BUILD content and file
membership against the existing generator on identical inputs. Run generation
again and require no further changes. Inspect fixture changes explicitly; never
regenerate golden output wholesale to hide behavior drift. Consult
[the workaround inventory](workarounds.md) before removing dependency overrides.

## Validation history

The stage-1 baseline compared 106 BUILD files against the patched remote
generator. Before the #1969 migration, root generation left a clean checkout
unchanged and all 63 enabled generation fixtures plus the parser target passed.
The separate Go formatting test is replaced by root formatting and Nogo.
