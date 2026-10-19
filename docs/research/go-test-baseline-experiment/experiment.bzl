"""Bounded baseline experiment. No test package is compiled or executed."""

load("@rules_go//go/private:context.bzl", "go_context", "go_rule")

def _experiment_impl(ctx):
    go = go_context(ctx)
    result = ctx.actions.declare_directory(ctx.label.name + ".evidence")
    args = go.builder_args(go)
    args.add_all(ctx.files.srcs, before_each = "-src")
    args.add("-covdata", go.toolchain._covdata)
    args.add("-importpath", "example.invalid/baseline")
    args.add("-v")
    args.add("-work")
    inputs = ctx.files.srcs + [go.toolchain._covdata]
    if ctx.attr.absent_metadata:
        fake = ctx.actions.declare_file(ctx.label.name + "/pkg/tool/linux_amd64/cover")
        ctx.actions.write(fake, "#!/bin/sh\nexit 0\n", is_executable = True)
        inputs.append(fake)
        args.add("-sdk", fake.path.removesuffix("/pkg/tool/linux_amd64/cover"))
    ctx.actions.run_shell(
        inputs = depset(inputs, transitive = [go.sdk.headers, go.sdk.tools]),
        tools = [go.toolchain._builder],
        outputs = [result],
        arguments = [result.path, go.toolchain._builder.path, "baselinecoverage", args],
        command = """
result="$1"
shift
mkdir -p "$result"
printf '%s\\n' "$@" > "$result/arguments.txt"
"$@" -o "$result/baseline.lcov" > "$result/stdout.txt" 2> "$result/stderr.txt"
status=$?
printf '%s\\n' "$status" > "$result/exit-status"
work=$(sed -n 's/.*WORK=//p' "$result/stderr.txt" | head -1)
if test -n "$work" && test -d "$work"; then
    cp -R "$work" "$result/work"
    rm -rf "$work"
fi
exit 0
""",
        env = go.env,
        mnemonic = "GoBaselineExperiment",
    )
    return [DefaultInfo(files = depset([result]))]

baseline_experiment = go_rule(
    _experiment_impl,
    attrs = {
        "srcs": attr.label_list(allow_files = True),
        "absent_metadata": attr.bool(),
    },
)
