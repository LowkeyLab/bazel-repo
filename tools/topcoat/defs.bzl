"""Experimental declared caller metadata for Topcoat's Rust macros."""

load("@aspect_rules_js//js:defs.bzl", "js_run_binary")
load("@bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")
load("@rules_pkg//pkg:tar.bzl", "pkg_tar")
load("@rules_rs//rs:rust_binary.bzl", "rust_binary")
load("@rules_rs//rs:rust_library.bzl", "rust_library")

def _manifest_impl(ctx):
    output = ctx.actions.declare_file(ctx.label.name + "/Cargo.toml")
    lines = [
        "[package]",
        'name = "' + ctx.attr.package_name + '"',
        'version = "0.0.0"',
        'edition = "2024"',
        "[dependencies]",
    ]
    for alias, package in sorted(ctx.attr.dependencies.items()):
        lines.append(alias + ' = { package = "' + package + '", version = "*" }')
    ctx.actions.write(output, "\n".join(lines) + "\n")
    environment = ctx.actions.declare_file(ctx.label.name + "/macro.env")
    env = "CARGO_MANIFEST_DIR=" + output.dirname + "\n"
    if ctx.file.iconify_json:
        env += "OUT_DIR=" + ctx.file.iconify_json.dirname.rsplit("/", 1)[0] + "\n"
    ctx.actions.write(environment, env)
    return [DefaultInfo(files = depset([output])), OutputGroupInfo(environment = depset([environment]))]

_manifest = rule(
    implementation = _manifest_impl,
    attrs = {
        "package_name": attr.string(mandatory = True),
        "iconify_json": attr.label(allow_single_file = True),
        "dependencies": attr.string_dict(mandatory = True),
    },
)

def _consumer(rule, name, package_name, macro_dependencies, asset_inputs, topcoat_deps, iconify_json = None, **kwargs):
    if not package_name or not macro_dependencies:
        fail("Topcoat callers require explicit package and macro dependency names")
    asset_inputs = asset_inputs + ([iconify_json] if iconify_json else [])
    manifest = name + "_manifest"
    _manifest(name = manifest, package_name = package_name, dependencies = macro_dependencies, iconify_json = iconify_json)
    native.filegroup(name = name + "_macro_env", srcs = [":" + manifest], output_group = "environment")
    native.filegroup(
        name = name + "_topcoat_inputs",
        srcs = kwargs.get("srcs", []) + asset_inputs + [":" + manifest] + topcoat_deps,
    )
    rule(
        name = name,
        compile_data = kwargs.pop("compile_data", []) + asset_inputs + [":" + manifest],
        rustc_env_files = kwargs.pop("rustc_env_files", []) + [":" + name + "_macro_env"],
        rustc_env = kwargs.pop("rustc_env", {}) | {
            "CARGO_CRATE_NAME": kwargs.get("crate_name", name),
            "CARGO_PKG_NAME": package_name,
        },
        **kwargs
    )

def topcoat_rust_library(name, package_name, macro_dependencies, asset_inputs = [], topcoat_deps = [], **kwargs):
    """Compile a library with declared manifest metadata and asset inputs."""
    _consumer(rust_library, name, package_name, macro_dependencies, asset_inputs, topcoat_deps, **kwargs)

def topcoat_rust_binary(name, package_name, macro_dependencies, asset_inputs = [], topcoat_deps = [], **kwargs):
    """Compile a binary with declared manifest metadata and asset inputs."""
    _consumer(rust_binary, name, package_name, macro_dependencies, asset_inputs, topcoat_deps, **kwargs)

def topcoat_tailwind(name, cli, css, srcs, deps = []):
    """Generate CSS with a locked npm CLI and explicitly scanned sources."""
    stage = name + "_sources"
    copy_to_directory(name = stage, srcs = [css] + srcs, out = stage, root_paths = [native.package_name()])
    js_run_binary(
        name = name,
        tool = cli,
        copy_srcs_to_bin = False,
        srcs = [":" + stage] + deps,
        outs = [name + ".css"],
        args = [
            "--input",
            native.package_name() + "/" + stage + "/" + css.split(":")[-1].split("/")[-1],
            "--output",
            native.package_name() + "/" + name + ".css",
        ],
    )

def topcoat_assets(name, srcs, deps = []):
    """Collect direct and explicit transitive asset input companions."""
    native.filegroup(name = name, srcs = srcs + deps)

def _package_impl(ctx):
    output = ctx.actions.declare_directory(ctx.label.name)
    inputs = depset(ctx.files.inputs)
    manifest = ctx.actions.declare_file(ctx.label.name + ".inputs.json")
    ctx.actions.write(manifest, json.encode([
        {"path": f.path, "relative": f.short_path.removeprefix("../")}
        for f in inputs.to_list()
    ]))
    ctx.actions.run(
        executable = ctx.executable._bundler,
        arguments = [ctx.executable.binary.path, output.path, manifest.path],
        inputs = depset([ctx.executable.binary, manifest], transitive = [inputs]),
        outputs = [output],
        tools = [ctx.attr._bundler[DefaultInfo].files_to_run],
        mnemonic = "TopcoatBundle",
        execution_requirements = {"block-network": "1"},
    )
    return [DefaultInfo(files = depset([output]))]

_package = rule(
    implementation = _package_impl,
    attrs = {
        "binary": attr.label(executable = True, cfg = "target", mandatory = True),
        "inputs": attr.label_list(allow_files = True),
        "_bundler": attr.label(default = Label("//tools/topcoat:bundle"), executable = True, cfg = "exec"),
    },
)

def topcoat_package(name, binary, inputs):
    """Bundle an executable and declared inputs as one adjacent-assets archive."""
    layout = name + "_layout"
    _package(name = layout, binary = binary, inputs = inputs)
    pkg_tar(name = name, srcs = [":" + layout], strip_prefix = layout)
