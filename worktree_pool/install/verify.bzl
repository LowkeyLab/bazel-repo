"""Install an explicit verified archive with the pinned tools and shared version."""

load("//worktree_pool:package_metadata.bzl", "WORKTREE_POOL_VERSION")

def _verify_impl(ctx):
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    paths = [ctx.executable.driver, tc.cargo, tc.rustc, ctx.file.git_source, ctx.file.zlib_source, ctx.executable.cli_tests]

    # A bazel run launcher has its own adjacent runfiles tree; never assume caller CWD.
    arguments = " ".join(['"$runfiles/%s"' % f.short_path for f in paths])
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
runfiles=$(cd -P "${{BASH_SOURCE[0]}}.runfiles/_main" && pwd)
exec {arguments} "$@" --expected-version {version} --sysroot "$runfiles/{sysroot}" --runfiles-root "$runfiles/.." --workspace _main
""".format(arguments = arguments, version = WORKTREE_POOL_VERSION, sysroot = tc.sysroot_short_path), is_executable = True)
    runfiles = ctx.runfiles(files = paths, transitive_files = tc.all_files)
    runfiles = runfiles.merge(ctx.attr.driver[DefaultInfo].default_runfiles)
    runfiles = runfiles.merge(ctx.attr.cli_tests[DefaultInfo].default_runfiles)
    return [DefaultInfo(executable = script, runfiles = runfiles)]

verify_install = rule(
    implementation = _verify_impl,
    executable = True,
    attrs = {
        "driver": attr.label(executable = True, cfg = "exec", mandatory = True),
        "git_source": attr.label(allow_single_file = True, mandatory = True),
        "zlib_source": attr.label(allow_single_file = True, mandatory = True),
        "cli_tests": attr.label(executable = True, cfg = "target", mandatory = True),
    },
    toolchains = ["@rules_rust//rust:toolchain_type"],
)
