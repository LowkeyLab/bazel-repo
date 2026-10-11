"""Resolve pinned Cargo/rustc and declared publishing inputs for runtime validation."""

load(":providers.bzl", "CargoPublishInfo")

def _runtime_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    package = ctx.attr.package[CargoPublishInfo]
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    paths = [ctx.executable._driver, package.package, package.contract, tc.cargo, tc.rustc]
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
unset PYTHONOPTIMIZE PYTHONPATH PYTHONHOME
runfiles=$(cd -P "${{BASH_SOURCE[0]}}.runfiles/_main" && pwd)
output_root=$(cd -P "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)/{name}.runs
exec "$runfiles/{driver}" "$@" --package "$runfiles/{package}" --contract "$runfiles/{contract}" --cargo "$runfiles/{cargo}" --rustc "$runfiles/{rustc}" --sysroot "$runfiles/{sysroot}" --output-root "$output_root"
""".format(name = ctx.label.name, driver = paths[0].short_path, package = package.package.short_path, contract = package.contract.short_path, cargo = tc.cargo.short_path, rustc = tc.rustc.short_path, sysroot = tc.sysroot_short_path), is_executable = True)
    runfiles = ctx.runfiles(files = paths, transitive_files = tc.all_files)
    runfiles = runfiles.merge(ctx.attr._driver[DefaultInfo].default_runfiles)
    return [DefaultInfo(executable = script, runfiles = runfiles)]

cargo_publish_runtime = rule(
    implementation = _runtime_impl,
    executable = True,
    attrs = {
        "package": attr.label(mandatory = True, providers = [CargoPublishInfo]),
        "_driver": attr.label(default = Label("//tools/cargo_publish:verify"), executable = True, cfg = "exec"),
    },
    toolchains = ["@rules_rust//rust:toolchain_type"],
)
