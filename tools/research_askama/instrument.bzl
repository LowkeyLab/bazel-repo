"""Bounded research-only Askama template transformation."""

def _instrument_impl(ctx):
    output = ctx.actions.declare_directory(ctx.label.name)
    env = ctx.actions.declare_file(ctx.label.name + ".env")
    ctx.actions.run(
        executable = ctx.executable.tool,
        inputs = ctx.files.srcs,
        outputs = [output],
        arguments = [output.path] + [source.path for source in ctx.files.srcs],
        mnemonic = "AskamaResearchInstrument",
    )
    ctx.actions.write(env, "CARGO_MANIFEST_DIR=" + output.path + "\n")
    tc = ctx.toolchains["@rules_rust//rust:toolchain_type"]
    compiler = ctx.actions.declare_file(ctx.label.name + ".compiler.txt")
    ctx.actions.run_shell(
        inputs = tc.all_files,
        outputs = [compiler],
        arguments = [tc.rustc.path, compiler.path, tc.target_flag_value, str(tc.coverage_supported)],
        command = '\"$1\" -vV > \"$2\"; printf \"target=%s\\ncoverage_supported=%s\\n\" \"$3\" \"$4\" >> \"$2\"',
        mnemonic = "AskamaResearchCompilerMetadata",
    )
    return [DefaultInfo(files = depset([output])), OutputGroupInfo(env = depset([env]), compiler = depset([compiler]))]

instrument = rule(
    implementation = _instrument_impl,
    toolchains = ["@rules_rust//rust:toolchain_type"],
    attrs = {
        "srcs": attr.label_list(allow_files = [".html"]),
        "tool": attr.label(executable = True, cfg = "exec", mandatory = True),
    },
)
