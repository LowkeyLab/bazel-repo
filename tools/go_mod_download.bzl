"""Download Go module checksums without building the Go launcher or nogo."""

load("@platforms//host:constraints.bzl", "HOST_CONSTRAINTS")

def _go_mod_download_impl(ctx):
    sdk = ctx.toolchains["@rules_go//go:toolchain"].sdk
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
runfiles_root="$PWD"
export GOROOT="$runfiles_root/{goroot}"
go_path="$runfiles_root/{go}"
cd "$BUILD_WORKSPACE_DIRECTORY"
exec "$go_path" mod download all
""".format(goroot = sdk.root_file.short_path.rsplit("/", 1)[0], go = sdk.go.short_path), is_executable = True)
    sdk_files = depset(
        [sdk.go],
        transitive = [sdk.headers, sdk.srcs, sdk.libs, sdk.tools],
    )
    return [DefaultInfo(executable = script, runfiles = ctx.runfiles(transitive_files = sdk_files))]

go_mod_download = rule(
    implementation = _go_mod_download_impl,
    executable = True,
    toolchains = ["@rules_go//go:toolchain"],
    exec_compatible_with = HOST_CONSTRAINTS,
)
