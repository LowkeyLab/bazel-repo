"""Update the root Cargo lockfile using the Bazel-managed Rust toolchain."""

def _cargo_lock_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
runfiles_root="$PWD"
export RUSTC="$runfiles_root/{rustc}"
cargo_path="$runfiles_root/{cargo}"
cd "$BUILD_WORKSPACE_DIRECTORY"
exec "$cargo_path" update --workspace
""".format(rustc = tc.rustc.short_path, cargo = tc.cargo.short_path), is_executable = True)
    return [DefaultInfo(executable = script, runfiles = ctx.runfiles(transitive_files = tc.all_files))]

cargo_lock = rule(
    implementation = _cargo_lock_impl,
    executable = True,
    toolchains = ["@rules_rust//rust:toolchain_type"],
)
