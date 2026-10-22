"""Native JavaScript inputs and independent collection metadata."""

load("@aspect_rules_js//js:defs.bzl", _js_test = "js_test")
load("@aspect_rules_js//js:providers.bzl", "JsInfo")

def _coverage_runfiles_impl(ctx):
    sources = []
    existing = {}
    for target in ctx.attr.targets:
        runfiles = target[DefaultInfo].default_runfiles
        if runfiles:
            existing.update({file.short_path: True for file in runfiles.files.to_list()})
        if InstrumentedFilesInfo in target:
            sources.extend(target[InstrumentedFilesInfo].instrumented_files.to_list())
    return [DefaultInfo(runfiles = ctx.runfiles(files = [file for file in depset(sources).to_list() if file.is_source and file.short_path not in existing and not file.owner.workspace_name and file.extension in ("js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx")]))]

_coverage_runfiles = rule(
    implementation = _coverage_runfiles_impl,
    attrs = {"targets": attr.label_list(allow_files = True)},
)

def js_test(name, entry_point, data = [], browser = False, **kwargs):
    """Supply native manifest originals without changing the collector model."""
    if browser:
        environment = kwargs.pop("env", {})
        kwargs["env"] = select({
            "//tools/coverage:javascript_coverage": dict(environment, BROWSER_COVERAGE = "1"),
            "//conditions:default": environment,
        })
    _coverage_runfiles(name = name + ".javascript_sources", targets = data + [entry_point], testonly = True, tags = ["manual"])
    _js_test(
        name = name,
        entry_point = entry_point,
        data = data + [entry_point] + select({
            "//tools/coverage:javascript_coverage": [":" + name + ".javascript_sources"],
            "//conditions:default": [],
        }),
        **kwargs
    )

def _javascript_test_inputs_impl(target, ctx):
    if ctx.label.workspace_name or ctx.rule.kind != "js_test":
        return []
    if InstrumentedFilesInfo not in target:
        fail("Required JavaScript instrumentation metadata is absent: " + str(ctx.label))
    output = ctx.actions.declare_file(ctx.label.name + ".javascript-test-inputs.json")
    artifacts = []
    for dependency in getattr(ctx.rule.attr, "data", []):
        artifacts.extend(dependency[JsInfo].transitive_sources.to_list() if JsInfo in dependency else dependency[DefaultInfo].files.to_list())
    scripts = [file for file in depset(artifacts).to_list() if not file.is_directory and not file.owner.workspace_name and file.extension in ("js", "mjs", "cjs", "jsx", "map")]
    ctx.actions.write(output, json.encode({
        "rule": "//{}:{}".format(ctx.label.package, ctx.label.name),
        "instrumented": ctx.coverage_instrumented(),
        "browser": getattr(ctx.rule.attr, "env", {}).get("BROWSER_COVERAGE") == "1",
        "sources": [file.short_path for file in target[InstrumentedFilesInfo].instrumented_files.to_list() if not file.owner.workspace_name],
        "artifacts": [{"path": file.path, "source": file.short_path} for file in scripts],
    }))
    return [OutputGroupInfo(javascript_test_inputs = depset([output] + scripts))]

javascript_test_inputs = aspect(implementation = _javascript_test_inputs_impl)
