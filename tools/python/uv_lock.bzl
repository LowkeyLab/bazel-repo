"""Update the Python lockfile using Bazel-managed uv and Python."""

load("@platforms//host:constraints.bzl", "HOST_CONSTRAINTS")

def _uv_lock_impl(ctx):
    runtime = ctx.toolchains["@bazel_tools//tools/python:toolchain_type"].py3_runtime
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
runfiles_root="$PWD"
python_path="$runfiles_root/{python}"
uv_path="$runfiles_root/{uv}"
cd "$BUILD_WORKSPACE_DIRECTORY"
exec "$uv_path" lock --python "$python_path" --no-python-downloads "$@"
""".format(python = runtime.interpreter.short_path, uv = ctx.file._uv.short_path), is_executable = True)
    return [DefaultInfo(
        executable = script,
        runfiles = ctx.runfiles(files = [ctx.file._uv], transitive_files = runtime.files),
    )]

uv_lock = rule(
    implementation = _uv_lock_impl,
    executable = True,
    attrs = {
        "_uv": attr.label(default = "@uv//:uv", allow_single_file = True, cfg = "exec"),
    },
    toolchains = ["@bazel_tools//tools/python:toolchain_type"],
    exec_compatible_with = HOST_CONSTRAINTS,
)
