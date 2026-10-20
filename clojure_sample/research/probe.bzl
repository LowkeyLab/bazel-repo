"""Bounded research action with declared artifacts from the parent and child JVMs."""

def _resources(_os, _inputs):
    return {"cpu": 1, "memory": 1536}

def _impl(ctx):
    output = ctx.actions.declare_directory(ctx.label.name + ".evidence")
    ctx.actions.run(
        executable = ctx.executable.runner,
        arguments = ["-m", "coverage-research.runner", "parent", output.path],
        tools = [ctx.attr.runner[DefaultInfo].files_to_run],
        outputs = [output],
        env = {"JAVA_TOOL_OPTIONS": "-Xmx512m"},
        mnemonic = "ClojureResearch",
        execution_requirements = {"timeout": "180"},
        progress_message = "Collect bounded original-form Clojure evidence",
        resource_set = _resources,
    )
    return [DefaultInfo(files = depset([output]))]

clojure_research = rule(
    implementation = _impl,
    attrs = {"runner": attr.label(executable = True, cfg = "exec", mandatory = True)},
)
