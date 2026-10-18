"""Python coverage and test conventions for Gazelle-generated targets."""

load("@aspect_rules_py//py:defs.bzl", _py_binary = "py_binary", _py_library = "py_library", _py_test = "py_test")

def _coverage_tool():
    return select({
        "//tools/python:coverage_enabled": "//tools/python:coverage_baseline",
        "//conditions:default": None,
    })

def _coverage_sources_impl(ctx):
    production = not ctx.attr.testonly and not ctx.label.workspace_name and not ctx.label.package.startswith("3rdparty/")
    source_attributes = ["srcs", "main"] if production else []
    sources = []
    if ctx.executable.tool and ctx.coverage_instrumented():
        for attribute in source_attributes:
            for source in getattr(ctx.files, attribute):
                if source.is_source and not source.owner.workspace_name and source.extension == "py":
                    sources.append(source)
    sources = depset(sources).to_list()
    baselines = []
    if sources:
        baseline = ctx.actions.declare_file(ctx.label.name + ".lcov")
        arguments = ctx.actions.args()
        arguments.add(baseline)
        arguments.add_all(sources)
        ctx.actions.run(
            executable = ctx.executable.tool,
            arguments = [arguments],
            inputs = sources,
            outputs = [baseline],
            mnemonic = "PythonCoverageBaseline",
        )
        baselines.append(baseline)
    return [
        DefaultInfo(files = depset(), runfiles = ctx.runfiles()),
        coverage_common.instrumented_files_info(
            ctx,
            source_attributes = source_attributes,
            extensions = ["py"],
            metadata_files = sources,
            baseline_coverage_files = baselines,
        ),
    ]

_coverage_sources = rule(
    implementation = _coverage_sources_impl,
    attrs = {
        "srcs": attr.label_list(allow_files = True),
        "main": attr.label(allow_files = True),
        "tool": attr.label(executable = True, cfg = "exec"),
    },
)

def _coverage_data(name, srcs, main, kwargs):
    metadata = name + ".python_coverage"
    testonly = {"testonly": kwargs["testonly"]} if "testonly" in kwargs else {}
    _coverage_sources(
        name = metadata,
        srcs = srcs,
        main = main,
        tool = _coverage_tool(),
        tags = ["manual"],
        **testonly
    )
    return select({
        "//tools/python:coverage_enabled": [":" + metadata],
        "//conditions:default": [],
    })

def py_library(name, srcs = [], data = [], tags = [], **kwargs):
    """Declare a library with a production coverage baseline.

    Args:
        name: Target name.
        srcs: Library source files.
        data: Runtime dependencies.
        tags: Existing target tags.
        **kwargs: Remaining rules_py library attributes.
    """
    _py_library(
        name = name,
        srcs = srcs,
        data = data + _coverage_data(name, srcs, None, kwargs),
        tags = tags + ["python-coverage"],
        **kwargs
    )

def py_binary(name, srcs = [], main = None, data = [], tags = [], **kwargs):
    """Declare a binary with a production coverage baseline.

    Args:
        name: Target name.
        srcs: Binary source files.
        main: Binary entry point.
        data: Runtime dependencies.
        tags: Existing target tags.
        **kwargs: Remaining rules_py binary attributes.
    """
    _py_binary(
        name = name,
        srcs = srcs,
        main = main,
        data = data + _coverage_data(name, srcs, main, kwargs),
        tags = tags + ["python-coverage"],
        **kwargs
    )

def py_test(name, main = None, deps = [], **kwargs):
    """Use pytest for generated test sources instead of a source entry point.

    Args:
        name: Target name.
        main: Gazelle's generated entry point, replaced by pytest.
        deps: Dependencies inferred by Gazelle.
        **kwargs: Remaining rules_py test attributes.
    """
    deps = list(deps)
    if "@pypi//pytest" not in deps:
        deps.append("@pypi//pytest")
    if "@pypi//coverage" not in deps:
        deps.append("@pypi//coverage")
    if "//tools/python:coverage_runtime" not in deps:
        deps.append("//tools/python:coverage_runtime")
    _py_test(
        name = name,
        pytest_main = True,
        deps = deps,
        **kwargs
    )
