"""Bounded observations of loading, analysis, and registered shell execution."""

print("RESEARCH_LOADING")

def _phase_impl(ctx):
    print("RESEARCH_ANALYSIS")
    output = ctx.actions.declare_file(ctx.label.name + ".txt")
    ctx.actions.run_shell(
        outputs = [output],
        command = "printf 'RESEARCH_EXECUTION\\n' > \"$1\"",
        arguments = [output.path],
        mnemonic = "StarlarkResearchShell",
    )
    return [DefaultInfo(files = depset([output]))]

phase = rule(
    implementation = _phase_impl,
    attrs = {"subject": attr.label()},
)
