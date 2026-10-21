"""Go declarations with independent source baselines."""

load("@rules_go//go:def.bzl", "go_context", "go_rule", _go_binary = "go_binary", _go_library = "go_library", _go_test = "go_test")
load("@rules_go//go/private/actions:baseline_coverage.bzl", "baseline_coverage_kwargs")

def _go_coverage_sources_impl(ctx):
    go = go_context(ctx, goos = ctx.attr.goos, goarch = ctx.attr.goarch)
    sources = [source for source in ctx.files.srcs if source.is_source and source.extension == "go"]
    baseline = baseline_coverage_kwargs(go, ctx, sources)
    if go.coverage_enabled and go.coverage_instrumented and sources and not baseline:
        fail("The Go toolchain did not supply the required baseline collector.")
    return [
        DefaultInfo(files = depset()),
        coverage_common.instrumented_files_info(
            ctx,
            source_attributes = ["srcs"],
            extensions = ["go"],
            **baseline
        ),
    ]

_go_coverage_sources = go_rule(
    _go_coverage_sources_impl,
    attrs = {
        "srcs": attr.label_list(allow_files = True),
        "goos": attr.string(default = "auto"),
        "goarch": attr.string(default = "auto"),
        "cgo": attr.bool(),
        "_go_context_data": attr.label(default = "@rules_go//:go_context_data"),
    },
)

def _baseline(name, srcs, kwargs):
    _go_coverage_sources(
        name = name + ".go_coverage",
        srcs = srcs,
        goos = kwargs.get("goos", "auto"),
        goarch = kwargs.get("goarch", "auto"),
        cgo = kwargs.get("cgo", False),
        testonly = kwargs.get("testonly", False),
        tags = ["manual", "go-coverage"],
    )

def go_library(name, srcs = [], **kwargs):
    """Declare a library and its source baseline.

    Args:
        name: Library target name.
        srcs: Library source files.
        **kwargs: Existing Go library attributes.
    """
    _baseline(name, srcs, kwargs)
    _go_library(name = name, srcs = srcs, **kwargs)

def go_binary(name, srcs = [], **kwargs):
    """Declare an executable and its source baseline.

    Args:
        name: Executable target name.
        srcs: Executable source files.
        **kwargs: Existing Go executable attributes.
    """
    _baseline(name, srcs, kwargs)
    _go_binary(name = name, srcs = srcs, **kwargs)

def go_test(name, srcs = [], **kwargs):
    """Declare a test and a baseline that does not execute the test.

    Args:
        name: Test target name.
        srcs: Test source files.
        **kwargs: Existing Go test attributes.
    """
    _baseline(name, srcs, dict(kwargs, testonly = True))
    _go_test(name = name, srcs = srcs, **kwargs)
