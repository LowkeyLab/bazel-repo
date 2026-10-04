# Rust BUILD workaround inventory

Cleanup for [#1960](https://github.com/LowkeyLab/bazel-repo/issues/1960),
after unified crate discovery landed in `fc8321f8` (#1961, implementing #1957).

## Removed

| Area                       | Removed configuration                                                                        | Replacement / preserved behavior                                                                                                                                                                                                                    |
| -------------------------- | -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Root                       | `book_smartz/storage/*`, `prediction_bot/*`, and `nicknamer/*` exclusions                    | Generate every application Rust package; vendored and unrelated language exclusions remain.                                                                                                                                                         |
| Hearthstone simulator      | 15 module exclusions, 11 source-entry keeps, and 10 duplicate test sources in `compile_data` | All modules, including test modules, remain in generated `srcs`; retain `//:Cargo.toml` and `CARGO_MANIFEST_DIR`.                                                                                                                                   |
| Pinyin WASM                | `src`/`tests` exclusions, two protected source globs, and 14 dependency keeps                | Explicit generated sources for both library variants; preserve crate names, native test companion, WASM platform, bindgen command, and integration runner.                                                                                          |
| Book Smartz domain/storage | Package ignores; storage image-data/dependency keeps                                         | Narrow CloudEvents mapping; preserve migrations, fixture inputs, image data, environment, execution properties, and integration roots. Unit tests inherit library dependencies instead of redundantly listing the crate and `thiserror`.            |
| Prediction Bot             | Library ignore, source glob, image-data/dependency keeps                                     | Narrow CloudEvents mapping and explicit sources; retain macro-only `tracing` below. `health_test` now uses `crate = ":bin"`, running the existing `health::tests` once under the binary crate. The composition integration runner remains separate. |
| Nicknamer                  | Library source globs and shared integration source/dependency expressions                    | Generated per-runner lists include the common modules each runner owns. Remove unused `COMMON_SRCS`/`COMMON_DEPS`; retain shared image, snapshot, environment, and execution configuration and all ten runner labels.                               |
| Nicknamer2                 | Direct test-image dependency keeps and data/compile-data keeps                               | Dependencies are discovered from direct imports. Non-generated data and compile-data attributes remain, including SQL migrations and runtime images.                                                                                                |
| Rust test images           | Explicit `runfiles` mapping and old dependency spelling                                      | Automatic `@rules_rust//tools/runfiles` aliases the old `@rules_rust//rust/runfiles`; keep only the generated label.                                                                                                                                |

## Retained narrow configuration

| Location                                   | Configuration                                                                     | Reason                                                                                                                                                                                                                                  |
| ------------------------------------------ | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Book Smartz domain; Prediction Bot library | `gazelle:resolve rust cloudevents @crates//:cloudevents-sdk`                      | The imported crate name differs from its package label; generation otherwise drops the required dependency.                                                                                                                             |
| Nicknamer migration binary                 | `gazelle:resolve rust cli @crates//:sea-orm-migration`                            | `cli` arrives through `sea_orm_migration::prelude::*`; the resolver cannot infer that external re-export.                                                                                                                               |
| Nicknamer migration library                | `gazelle:rust_ignore_import async_trait`                                          | SeaORM's prelude re-exports the attribute macro. Without this narrow ignore, generation adds nonexistent `@crates//:async-trait`; the focused build confirmed that failure. The direct SeaORM dependency remains.                       |
| Nicknamer server library                   | `@crates//:config` with `# keep`                                                  | The external crate is used inside a local module also named `config`. Source-level resolution drops it as a local name; the focused build confirmed E0433. Scope-aware external/local name disambiguation remains outside this cleanup. |
| Prediction Bot library                     | `@crates//:tracing` with `# keep`                                                 | `audit/logging.rs` references it inside `macro_rules! emit`. The parser does not expand macro bodies; removing this dependency caused E0433 in the library build. General macro dependency discovery is outside this cleanup.           |
| Rust test images                           | Versioned `futures_util` and `tokio_util` resolve mappings                        | Automatic resolution chooses nonexistent unversioned labels; preserve the existing `futures-util-0.3.34` and `tokio-util-0.7.19` labels.                                                                                                |
| Root                                       | `gazelle:rust_cargo_lockfile Cargo.lock`; `gazelle:rust_crates_prefix @crates//:` | These select the repository's dependency inventory and label namespace, rather than suppress generation.                                                                                                                                |
| Root                                       | `gazelle:exclude 3rdparty/gazelle_rust/upstream`                                  | The maintained nested module and its deliberately hand-authored fixtures must not be regenerated as application packages. Their parser/generation/format tests run explicitly.                                                          |

No application Rust package exclusions or whole-package ignores remain.
The root exclusions for `githooks`, `tools`, Angular, and Kotlin projects,
`tools/test_images`' Starlark ignore, and other language-specific directives
are unchanged and are not application Rust suppression. Template, migration,
and snapshot globs remain intentional non-Rust build inputs. Generated
`crate_root` attributes are retained. No generator or Rust source changes were
needed.

## Validation

Verified on 2026-10-05 through the repository Nix/Bazel environment:

- `nix develop --command bazel run //:gazelle`: no unresolved-import or merge diagnostics; repeated generation leaves the checkout byte-for-byte unchanged.
- `nix develop --command aspect test //hearthstone_simulator/... //pinyin-composer/wasm/... //book_smartz/... //prediction_bot/... //nicknamer/... //nicknamer2/... //test_images/rust/... @gazelle_rust//gazelle_rust_parser/tests:parse_test @gazelle_rust//generation_tests/... @gazelle_rust//rust_language:gofmt_test`: all 99 targets passed (34 application and 65 maintained generator/parser/format targets).
- `nix develop --command aspect format --scope=all`: passed.
- `nix develop --command aspect build //...`: passed, 384 targets.
- `nix develop --command aspect test //...`: all 76 targets passed (cached).
- `nix develop --command aspect lint`: passed, no findings.
- `git diff --check`: passed.

A source-list comparison against the starting commit found no lost sources:
only Prediction Bot's health-test modules move from a standalone module runner
into the binary crate's unit runner. Its test log confirms all six health tests
pass. Target names, explicit crate names, runtime data, environment, execution
properties, and platform settings are unchanged. The only compile-data removal
is Hearthstone's ten Rust files already present in `srcs`.

The first focused attempts exposed the missing SeaORM `async-trait` label and
missing `tracing`/`config` dependencies described above; validation passed after
applying those narrow workarounds. One Bazel server termination recovered on
Aspect's automatic retry. Existing Nix/toolchain deprecations and upstream
Java/native-access warnings are non-blocking.
