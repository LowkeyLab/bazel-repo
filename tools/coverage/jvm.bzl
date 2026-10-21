"""JVM declarations with standard JaCoCo class analysis."""

load("@rules_java//java:defs.bzl", _java_binary = "java_binary", _java_library = "java_library", _java_test = "java_test")
load("@rules_java//java/common:java_info.bzl", "JavaInfo")
load("@rules_kotlin//kotlin:jvm.bzl", _kt_jvm_binary = "kt_jvm_binary", _kt_jvm_library = "kt_jvm_library", _kt_jvm_test = "kt_jvm_test")

def _jvm_runtime_jars(target):
    info = target[JavaInfo]
    if info.compilation_info:
        return info.compilation_info.runtime_classpath
    return info.transitive_runtime_jars

def _jvm_test_inputs_impl(target, ctx):
    if ctx.label.workspace_name or ctx.rule.kind not in ("java_test", "kt_jvm_test"):
        return []
    if InstrumentedFilesInfo not in target:
        fail("Required configured JVM instrumentation metadata is absent: " + str(ctx.label))
    output = ctx.actions.declare_file(ctx.label.name + ".jvm-test-inputs.json")
    instrumented = [target[InstrumentedFilesInfo].instrumented_files]
    runtime_jars = [_jvm_runtime_jars(target)]
    for child in getattr(ctx.rule.attr, "data", []):
        if JavaInfo in child and InstrumentedFilesInfo in child and child[DefaultInfo].files_to_run.executable:
            instrumented.append(child[InstrumentedFilesInfo].instrumented_files)
            runtime_jars.append(_jvm_runtime_jars(child))
    sources = [file.short_path for file in depset(transitive = instrumented).to_list() if file.is_source and not file.owner.workspace_name and file.extension in ("java", "kt", "kts")]
    ctx.actions.write(output, json.encode({
        "rule": "//{}:{}".format(ctx.label.package, ctx.label.name),
        "sources": sources,
        "jars": [jar.path for jar in depset(transitive = runtime_jars).to_list()],
    }))
    return [OutputGroupInfo(jvm_test_inputs = depset([output]))]

jvm_test_inputs = aspect(implementation = _jvm_test_inputs_impl)

def _jvm_coverage_sources_impl(ctx):
    sources = [file for file in ctx.files.srcs if file.is_source and file.extension in ("java", "kt", "kts") and not file.owner.workspace_name]
    outputs = []
    if ctx.configuration.coverage_enabled:
        if sources and (not ctx.coverage_instrumented(ctx.attr.target) or not ctx.coverage_instrumented()):
            fail("A required JVM coverage root is outside the instrumentation filter: " + str(ctx.attr.target.label))
        jars = [output.class_jar for output in ctx.attr.target[JavaInfo].java_outputs]
        report = ctx.actions.declare_file(ctx.label.name + ".xml")
        metadata = ctx.actions.declare_file(ctx.label.name + ".json")
        ctx.actions.write(metadata, json.encode({
            "rule": "//{}:{}".format(ctx.attr.target.label.package, ctx.attr.target.label.name),
            "jars": [jar.path for jar in jars],
            "sources": [file.short_path for file in sources],
            "children": ["//{}:{}".format(child.label.package, child.label.name) for child in ctx.attr.data if not child.label.workspace_name and JavaInfo in child and child[DefaultInfo].files_to_run.executable],
        }))
        args = ctx.actions.args()
        args.add(report)
        args.add_all(jars, before_each = "--classfiles")
        ctx.actions.run(
            executable = ctx.executable._analyzer,
            arguments = [args],
            inputs = jars,
            outputs = [report],
            mnemonic = "JvmBaselineCoverage",
        )
        outputs = [report, metadata] + jars
    return [
        DefaultInfo(files = depset(sources)),
        OutputGroupInfo(jvm_coverage_baselines = depset(outputs)),
    ]

_jvm_coverage_sources = rule(
    implementation = _jvm_coverage_sources_impl,
    attrs = {
        "target": attr.label(mandatory = True, providers = [JavaInfo]),
        "srcs": attr.label_list(allow_files = True),
        "data": attr.label_list(allow_files = True),
        "_analyzer": attr.label(default = "//tools/coverage:jvm_analyzer", executable = True, cfg = "exec"),
    },
)

def _baseline(name, kwargs, testonly = False):
    _jvm_coverage_sources(
        name = name + ".jvm_coverage",
        target = ":" + name,
        srcs = kwargs.get("srcs", []),
        data = kwargs.get("data", []),
        testonly = testonly or kwargs.get("testonly", False),
        tags = ["manual", "jvm-coverage"],
    )

def java_library(name, **kwargs):
    """Declare a library and its JaCoCo baseline."""
    _java_library(name = name, **kwargs)
    _baseline(name, kwargs)

def java_binary(name, **kwargs):
    """Declare an executable and its JaCoCo baseline."""
    _java_binary(name = name, **kwargs)
    _baseline(name, kwargs)

def java_test(name, **kwargs):
    """Declare a test and its JaCoCo baseline without test execution."""
    _java_test(name = name, **kwargs)
    _baseline(name, kwargs, testonly = True)

def kt_jvm_library(name, **kwargs):
    """Declare a Kotlin library and its JaCoCo baseline."""
    _kt_jvm_library(name = name, **kwargs)
    _baseline(name, kwargs)

def kt_jvm_binary(name, **kwargs):
    """Declare a Kotlin executable and its JaCoCo baseline."""
    _kt_jvm_binary(name = name, **kwargs)
    _baseline(name, kwargs)

def kt_jvm_test(name, **kwargs):
    """Declare a Kotlin test and its JaCoCo baseline without test execution."""
    _kt_jvm_test(name = name, **kwargs)
    _baseline(name, kwargs, testonly = True)
