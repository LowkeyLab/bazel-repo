"""Rust declarations with LLVM source baselines."""

load("@rules_rust//rust:defs.bzl", "rust_common", _rust_binary = "rust_binary", _rust_library = "rust_library", _rust_test = "rust_test")

_RUST_TOOLCHAIN = "@rules_rust//rust:toolchain_type"

def _short_path(file):
    return file.short_path

def _rust_coverage_inputs_impl(ctx):
    return [DefaultInfo(files = ctx.attr.target[InstrumentedFilesInfo].instrumented_files)]

_rust_coverage_inputs = rule(
    implementation = _rust_coverage_inputs_impl,
    attrs = {"target": attr.label(mandatory = True)},
)

def _rust_coverage_sources_impl(ctx):
    target = ctx.attr.target
    crate = target[rust_common.crate_info]
    sources = target[InstrumentedFilesInfo].instrumented_files
    toolchain = ctx.toolchains[_RUST_TOOLCHAIN]
    baselines = []
    if ctx.configuration.coverage_enabled:
        if not ctx.coverage_instrumented(target) or not ctx.coverage_instrumented():
            fail("A required Rust coverage root is outside the instrumentation filter: " + str(target.label))
        if not toolchain.coverage_supported:
            fail("The native Rust toolchain must support LLVM coverage.")
        output = ctx.actions.declare_file(ctx.label.name + ".lcov")
        metadata = ctx.actions.declare_file(ctx.label.name + ".json")
        args = ctx.actions.args()
        args.add("--llvm-cov", toolchain.llvm_cov)
        args.add("--llvm-ar", ctx.executable._llvm_ar)
        args.add("--llvm-profdata", toolchain.llvm_profdata)
        args.add("--object", crate.output)
        args.add("--output", output)
        args.add("--metadata", metadata)
        files = sources.to_list() + crate.srcs.to_list() + crate.compile_data.to_list()
        if rust_common.dep_info in target:
            for dependency in target[rust_common.dep_info].transitive_crates.to_list():
                files += dependency.srcs.to_list() + dependency.compile_data.to_list()
        originals = {file.short_path: True for file in sources.to_list() if file.is_source}
        generated = [file for file in depset(files).to_list() if not file.is_source and not file.owner.workspace_name and file.extension == "rs" and file.short_path not in originals]
        for child in ctx.attr.data:
            if not child.label.workspace_name and rust_common.crate_info in child and child[rust_common.crate_info].type == "bin":
                for source in child[InstrumentedFilesInfo].instrumented_files.to_list():
                    if source.is_source and not source.owner.workspace_name and source.extension == "rs":
                        args.add("--child-source")
                        args.add(source.short_path)
                        args.add("//{}:{}".format(child.label.package, child.label.name))
                        args.add(child[rust_common.crate_info].output.path)
        args.add_all(generated, before_each = "--generated-source")
        args.add_all(generated, before_each = "--generated-source", map_each = _short_path)
        ctx.actions.run(
            executable = ctx.executable._baseline,
            arguments = [args],
            inputs = depset([crate.output] + toolchain.llvm_lib),
            tools = [toolchain.llvm_cov, toolchain.llvm_profdata, ctx.executable._llvm_ar],
            use_default_shell_env = True,
            outputs = [output, metadata],
            mnemonic = "RustBaselineCoverage",
        )
        baselines.append(output)
    return [
        DefaultInfo(files = sources),
        OutputGroupInfo(rust_coverage_baselines = depset(baselines + ([metadata] if baselines else []))),
        coverage_common.instrumented_files_info(
            ctx,
            source_attributes = ["srcs"],
            extensions = ["rs"],
            baseline_coverage_files = baselines,
        ),
    ]

_rust_coverage_sources = rule(
    implementation = _rust_coverage_sources_impl,
    attrs = {
        "target": attr.label(mandatory = True, providers = [rust_common.crate_info]),
        "srcs": attr.label_list(),
        "data": attr.label_list(allow_files = True),
        "_llvm_ar": attr.label(default = "@llvm//tools:llvm-ar", executable = True, allow_single_file = True, cfg = "exec"),
        "_baseline": attr.label(default = "//tools/coverage:rust_baseline", executable = True, cfg = "exec"),
    },
    toolchains = [_RUST_TOOLCHAIN],
)

def _baseline(name, kwargs, testonly = False):
    _rust_coverage_inputs(
        name = name + ".rust_coverage_inputs",
        target = ":" + name,
        testonly = testonly or kwargs.get("testonly", False),
        tags = ["manual"],
    )
    _rust_coverage_sources(
        srcs = [":" + name + ".rust_coverage_inputs"],
        name = name + ".rust_coverage",
        target = ":" + name,
        data = kwargs.get("data", []),
        testonly = testonly or kwargs.get("testonly", False),
        tags = ["manual", "rust-coverage"],
    )

def rust_library(name, **kwargs):
    """Declare a native library and its LLVM baseline."""
    _rust_library(name = name, **kwargs)
    _baseline(name, kwargs)

def _identity_flags(kwargs):
    kwargs["rustc_flags"] = kwargs.get("rustc_flags", []) + select({
        str(Label("//tools/coverage:rust_elf_coverage")): ["-Clink-arg=-Wl,--build-id=sha1"],
        "//conditions:default": [],
    })

def rust_binary(name, **kwargs):
    """Declare a native executable and its LLVM baseline."""
    _identity_flags(kwargs)
    _rust_binary(name = name, **kwargs)
    _baseline(name, kwargs)

def rust_test(name, **kwargs):
    """Declare a test and a baseline that does not execute the test."""
    _identity_flags(kwargs)
    kwargs.setdefault("coverage_readobj", Label("//tools/coverage:llvm-readobj"))
    _rust_test(name = name, **kwargs)
    _baseline(name, kwargs, testonly = True)
