"""Private runfiles launcher for the pinned composed formatter."""

def _runfile(ctx, file):
    return file.short_path[3:] if file.short_path.startswith("../") else ctx.workspace_name + "/" + file.short_path

def _format_impl(ctx):
    script = ctx.actions.declare_file(ctx.label.name + ".sh")
    tool = ctx.executable._tool
    rustfmt = ctx.file._rustfmt
    ctx.actions.write(script, """#!/usr/bin/env bash
set -euo pipefail
runfiles="${{RUNFILES_DIR:-$0.runfiles}}"
exec "$runfiles/{tool}" "$runfiles/{rustfmt}" "$@"
""".format(tool = _runfile(ctx, tool), rustfmt = _runfile(ctx, rustfmt)), is_executable = True)
    runfiles = ctx.runfiles(files = [tool, rustfmt]).merge(ctx.attr._tool[DefaultInfo].default_runfiles).merge(ctx.attr._rustfmt[DefaultInfo].default_runfiles)
    return [DefaultInfo(executable = script, runfiles = runfiles)]

format_adapter = rule(
    implementation = _format_impl,
    executable = True,
    attrs = {
        "_tool": attr.label(default = Label("//tools/topcoat:format_impl"), executable = True, cfg = "exec"),
        "_rustfmt": attr.label(default = Label("@rustfmt_linux_x86_64_1_99_1//:rustfmt_bin"), allow_single_file = True, cfg = "exec"),
    },
)
