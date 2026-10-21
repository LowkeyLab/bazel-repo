"""Go declarations with independent source baselines."""

load("@rules_go//go:def.bzl", "GoArchive", "go_context", "go_rule", _go_binary = "go_binary", _go_library = "go_library", _go_test = "go_test")

_GO_SETTINGS = {name: str(Label("@rules_go//go/config:" + name)) for name in ["tags", "pure", "race", "msan"]}
_PLATFORM_SETTING = "//command_line_option:platforms"
_ORIGINAL_SETTING = str(Label("//tools/go:coverage_original_settings"))
_CONFIGURATION_KEYS = _GO_SETTINGS.values() + [_PLATFORM_SETTING]

def _settings_impl(_ctx):
    return []

go_coverage_settings = rule(
    implementation = _settings_impl,
    build_setting = config.string(flag = False),
    doc = "Keep inherited settings for native source selection.",
)

def _restore_configuration_impl(settings, _attr):
    result = json.decode(settings[_ORIGINAL_SETTING])
    result[_ORIGINAL_SETTING] = ""
    return result

_restore_configuration = transition(
    implementation = _restore_configuration_impl,
    inputs = [_ORIGINAL_SETTING],
    outputs = _CONFIGURATION_KEYS + [_ORIGINAL_SETTING],
)

def _coverage_configuration_impl(settings, attr):
    result = dict(settings)
    for name in ["pure", "race", "msan"]:
        value = getattr(attr, name)
        if value != "auto":
            result[_GO_SETTINGS[name]] = value == "on"
    if attr.gotags:
        result[_GO_SETTINGS["tags"]] = sorted({tag: True for tag in attr.gotags})
    require_cgo = attr.race == "on" or attr.msan == "on"
    if require_cgo:
        if attr.pure == "on":
            fail("The race and msan modes require cgo.")
        result[_GO_SETTINGS["pure"]] = False
    elif attr.pure == "on":
        result[_GO_SETTINGS["race"]] = False
        result[_GO_SETTINGS["msan"]] = False
    if attr.goos != "auto" or attr.goarch != "auto":
        if attr.goos == "auto" or attr.goarch == "auto":
            fail("Set both goos and goarch for a cross-platform baseline.")
        suffix = "_cgo" if require_cgo or attr.pure == "off" else ""
        platform = "@rules_go//go/toolchain:" + attr.goos + "_" + attr.goarch + suffix
        result[_PLATFORM_SETTING] = [str(Label(platform))]
    if not settings[_ORIGINAL_SETTING]:
        original = {key: settings[key] for key in _GO_SETTINGS.values()}
        original[_PLATFORM_SETTING] = [str(platform) for platform in settings[_PLATFORM_SETTING]]
        result[_ORIGINAL_SETTING] = json.encode(original)
    return result

_coverage_configuration = transition(
    implementation = _coverage_configuration_impl,
    inputs = _CONFIGURATION_KEYS + [_ORIGINAL_SETTING],
    outputs = _CONFIGURATION_KEYS + [_ORIGINAL_SETTING],
)

def _go_coverage_inputs_impl(ctx):
    target = ctx.attr.target
    archive = target[GoArchive]
    providers = [archive.source] + [direct.source for direct in archive.direct]
    sources = [source for info in providers if info.label == target.label for source in info.srcs]
    return [DefaultInfo(files = depset([source for source in sources if source.is_source and source.extension == "go"]))]

_go_coverage_inputs = rule(
    implementation = _go_coverage_inputs_impl,
    attrs = {"target": attr.label(mandatory = True, providers = [GoArchive])},
)

def _go_coverage_sources_impl(ctx):
    go = go_context(ctx, goos = ctx.attr.goos, goarch = ctx.attr.goarch)
    sources = ctx.files.srcs
    baselines = []
    if go.coverage_enabled and sources and not go.coverage_instrumented:
        fail("The instrumentation filter excludes eligible Go sources: %s" % ctx.label)
    if go.coverage_enabled and go.coverage_instrumented and sources:
        if not go.toolchain._covdata:
            fail("The Go toolchain did not supply the required baseline collector.")
        output = ctx.actions.declare_file(ctx.label.name + ".lcov")
        arguments = go.builder_args(go)
        arguments.add_all(sources, before_each = "-src")
        arguments.add("-covdata", go.toolchain._covdata)
        arguments.add("-importpath", go.importpath)
        arguments.add("-o", output)
        ctx.actions.run(
            executable = go.toolchain._builder,
            arguments = ["baselinecoverage", arguments],
            inputs = depset(sources + [go.toolchain._covdata], transitive = [go.sdk.headers, go.sdk.tools]),
            outputs = [output],
            env = go.env,
            mnemonic = "GoBaselineCoverage",
            toolchain = "@rules_go//go:toolchain",
        )
        baselines.append(output)
    return [
        DefaultInfo(files = depset()),
        coverage_common.instrumented_files_info(
            ctx,
            source_attributes = ["srcs"],
            extensions = ["go"],
            baseline_coverage_files = baselines,
        ),
    ]

_go_coverage_sources = go_rule(
    _go_coverage_sources_impl,
    cfg = _coverage_configuration,
    attrs = {
        "srcs": attr.label_list(cfg = _restore_configuration),
        "goos": attr.string(default = "auto"),
        "goarch": attr.string(default = "auto"),
        "gotags": attr.string_list(),
        "pure": attr.string(default = "auto", values = ["auto", "on", "off"]),
        "race": attr.string(default = "auto", values = ["auto", "on", "off"]),
        "msan": attr.string(default = "auto", values = ["auto", "on", "off"]),
        "cgo": attr.bool(),
        "_go_context_data": attr.label(default = "@rules_go//:go_context_data"),
        "_allowlist_function_transition": attr.label(default = "@bazel_tools//tools/allowlists/function_transition_allowlist"),
    },
)

def _baseline(name, kwargs):
    testonly = {"testonly": kwargs["testonly"]} if "testonly" in kwargs else {}
    _go_coverage_inputs(
        name = name + ".go_coverage_inputs",
        target = ":" + name,
        tags = ["manual"],
        **testonly
    )
    _go_coverage_sources(
        name = name + ".go_coverage",
        srcs = [":" + name + ".go_coverage_inputs"],
        goos = kwargs.get("goos", "auto"),
        goarch = kwargs.get("goarch", "auto"),
        gotags = kwargs.get("gotags", []),
        pure = kwargs.get("pure", "auto"),
        race = kwargs.get("race", "auto"),
        msan = kwargs.get("msan", "auto"),
        cgo = kwargs.get("cgo", False),
        tags = ["manual", "go-coverage"],
        **testonly
    )

def go_library(name, srcs = [], **kwargs):
    """Declare a library and its source baseline.

    Args:
        name: Library target name.
        srcs: Library source files.
        **kwargs: Existing Go library attributes.
    """
    _baseline(name, kwargs)
    _go_library(name = name, srcs = srcs, **kwargs)

def go_binary(name, srcs = [], **kwargs):
    """Declare an executable and its source baseline.

    Args:
        name: Executable target name.
        srcs: Executable source files.
        **kwargs: Existing Go executable attributes.
    """
    _baseline(name, kwargs)
    _go_binary(name = name, srcs = srcs, **kwargs)

def go_test(name, srcs = [], **kwargs):
    """Declare a test and a baseline that does not execute the test.

    Args:
        name: Test target name.
        srcs: Test source files.
        **kwargs: Existing Go test attributes.
    """
    _baseline(name, dict(kwargs, testonly = True))
    _go_test(name = name, srcs = srcs, **kwargs)
