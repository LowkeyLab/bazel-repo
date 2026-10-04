# Locally maintained Gazelle Rust

The `upstream/` directory is a nested Bazel module copied from
[Calsign/gazelle_rust](https://github.com/Calsign/gazelle_rust) at commit
`747482bfad721b0fe7198c94491d76c32b263ac0`. Its tracked source tree was obtained
with `git archive`; its licence remains at `upstream/LICENSE`.

The root `MODULE.bazel` selects this source with `local_path_override`.
Go import paths and internal labels remain upstream-compatible. The parser and
protobuf code still build with upstream's internal rules_rust configuration.

## Local adaptations

The former `rules_rs_loads.patch` is incorporated directly:

- `rust_language/lang.go` generates rules_rs loads for common Rust rules and
  `cargo_build_script`, retaining the other upstream load families.
- `gazelle_rust_parser/src/BUILD.bazel` explicitly sets edition `2024`.

This baseline retains the upstream generation modes and application behavior.
Unified crate discovery is tracked separately in
[#1957](https://github.com/LowkeyLab/bazel-repo/issues/1957).

## Development

Run commands from the monorepo root:

```sh
nix develop --command bazel run //:gazelle
nix develop --command aspect build //:gazelle_bin @gazelle_rust//rust_parser:rust_parser
nix develop --command aspect test @gazelle_rust//gazelle_rust_parser/tests:parse_test @gazelle_rust//generation_tests:all
nix develop --command aspect test @gazelle_rust//rust_language:gofmt_test
```

The root `.gitattributes` marks the vendored tree `rules-lint-ignored` to
preserve upstream formatting and deliberately incomplete Rust fixtures. Use
the upstream Go format test above for maintained Go source.

Root Gazelle excludes `upstream/`, so it cannot rewrite the maintained plugin or
its generation fixtures. `.bazelignore` also keeps nested packages out of root
`//...` traversal. Run the explicit external targets above to exercise the local
module. Do not run Gazelle over its fixture directories or replace their expected
output wholesale when adapting load statements.

Before changing generation behavior, compare application BUILD output against
the existing generator using identical inputs, and confirm a second generation
run changes no files. Preserve current application exclusions.
