## gazelle\_rust

A Gazelle language plugin for Rust; automatic dependency management for Rust projects built with
Bazel.

References:

- [Gazelle](https://github.com/bazelbuild/bazel-gazelle)
- [rules\_rust](https://github.com/bazelbuild/rules_rust)
- [Bazel](https://bazel.build/)

## Maintenance

This copy is maintained as root monorepo packages. See the
[maintenance guide](../README.md#development) for shared toolchains and checks.
Run `nix develop --command bazel run //:gazelle` from the monorepo root.
Standalone module installation and releases are no longer supported here.

## Generated targets

The [generation fixtures](generation_tests/) show generated targets. The following rules are supported:

- `rust_library`
- `rust_binary`
- `rust_test`
- `rust_proc_macro`
- `rust_shared_library`
- `rust_static_library`

The locally maintained generator uses one crate-root pipeline. See
[the maintained discovery contract](../README.md#crate-discovery) for root
naming, module ownership, tests, diagnostics, and supported semantics.

Existing BUILD targets provide configuration; source membership follows module
declarations. Library and binary roots use `lib.rs` and `main.rs`. Cargo manifests
are optional and do not select a different generation mode.

`gazelle:rust_default_edition` specifies the toolchain edition so matching Cargo
editions need not be repeated in generated rules.

## Crate features

gazelle\_rust reads the set of features in the `crate_features` attribute and skips adding
dependencies which are not needed for the enabled features.

When a Cargo manifest is present, the initial `crate_features` list is generated from the default
features list in `Cargo.toml`. This behavior for newly-generated targets can be further customized
with the following directives which apply to the package in which they are defined and subpackages:

```py
# gazelle:rust_default_features <true|false>
# gazelle:rust_feature <feature_name> <true|false>
```

`gazelle:rust_default_features false` disables use of the default features from `Cargo.toml`,
analogous to `default_features = false` in cargo.

`gazelle:rust_feature my_feature <true|false>` adds an override for a specific feature name to add
or remove that feature from the set of features added to newly-generated targets. The feature will
only be added to a rust target if that feature is present in `Cargo.toml` for that package.

## Assigning dependencies

gazelle\_rust parses each source file and identifies any path that looks like an external crate
dependency. For example `some_lib::Foobar` implies a new dependency on `some_lib` unless `some_lib`
is already in scope.

This approach is fairly robust. Please see [`rust_parser/parser.rs`](./rust_parser/parser.rs) for
implementation details and the [parser
tests](https://github.com/Calsign/gazelle_rust/tree/main/rust_parser/test_data) for the range of
cases covered.

For each dependency, gazelle\_rust identifies the crate in the project (or crate universe
dependency) providing that crate name. gazelle\_rust raises an error if the crate could not be found
or more than one crate with that name was found.

This means there is a global namespace of crates within the project. If this poses an issue for you,
you can use the gazelle [`resolve`
directive](https://github.com/bazelbuild/bazel-gazelle#directives) to configure which target is
selected on a per-directory basis.

## Crate universe

The [crate-universe fixtures](generation_tests/crate_universe/) cover external
and vendored dependency labels. Root dependencies are configured in the monorepo
Cargo manifest and lockfile.

Different configurations of crate universe use either a cargo lockfile (`Cargo.lock`) or a custom
lockfile (`Cargo.Bazel.lock`), and gazelle\_rust supports both. Use the directive
`gazelle:rust_cargo_lockfile` to indicate a cargo lockfile and `gazelle:rust_lockfile` to indicate a
custom lockfile. These options are mutually exclusive.

Additionally, you must tell rules\_rust the prefix for all crate universe labels using the
`gazelle:rust_crates_prefix` directive, e.g. `@crates//:` for a repository rule approach or
`//3rdparty/crates:` for a vendored approach.

## Ignoring dependencies

Some situations are too complex for gazelle\_rust to handle, such as platform-conditional
dependencies. It is possible that fancy support could be added in the future, but for now you must
handle this manually by ignoring the dependency in the source file and potentially adding [`# keep`
comments](https://github.com/bazelbuild/bazel-gazelle#keep-comments) in the build file.

To tell gazelle\_rust to ignore a dependency, you can add the `#[gazelle::ignore]` attribute macro
to a use item. For example:

```rust
// the tokio runtime is not supported in wasm
#[cfg(not(target_arch = "wasm32"))]
#[gazelle::ignore]
use tokio::runtime::Runtime;
```

Then in the build file:

```py
rust_library(
    name = "maybe_tokio",
    deps = select({
        "@platforms//cpu:wasm32": [],
        "//conditions:default": [
            "//3rdparty/crates:tokio",
        ],
    }),
)

rust_library(
    name = "some_cool_cross_platform_thing",
    deps = [
        ":maybe_tokio",  # keep
    ],
    ...
)
```

See the [macro crate](./macro) for more information about the ignore macro.
