"""Offline analysis rejection fixtures through the public cargo_publish macro."""

load("@bazel_skylib//lib:unittest.bzl", "analysistest", "asserts")
load("@rules_rs//rs:rust_binary.bzl", "rust_binary")
load("@rules_rs//rs:rust_library.bzl", "rust_library")
load("@rules_rs//rs:rust_proc_macro.bzl", "rust_proc_macro")
load("@rules_rust//cargo:defs.bzl", "cargo_build_script")
load(":defs.bzl", "cargo_publish")

def _rejection_impl(ctx):
    env = analysistest.begin(ctx)
    asserts.expect_failure(env, ctx.attr.message)
    return analysistest.end(env)

_rejection_test = analysistest.make(
    _rejection_impl,
    expect_failure = True,
    attrs = {"message": attr.string()},
)

_extra_flags_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_rustc_flags")): ["--cfg=custom"]})

_singular_flag_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_rustc_flag")): ["--cfg=custom"]})

_extra_env_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_rustc_env")): ["CUSTOM=value"]})

_per_crate_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:per_crate_rustc_flag")): ["//tools/cargo_publish/fixtures@--cfg=custom"]})

_exec_flags_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_exec_rustc_flags")): ["--cfg=custom"]})

_exec_singular_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_exec_rustc_flag")): ["--cfg=custom"]})

_exec_env_test = analysistest.make(_rejection_impl, expect_failure = True, attrs = {"message": attr.string()}, config_settings = {str(Label("@rules_rust//rust/settings:extra_exec_rustc_env")): ["CUSTOM=value"]})

def _publish(name, library = ":library", **kwargs):
    options = dict(
        library = library,
        package_name = "publishing-fixture",
        version = "0.1.0",
        description = "A declared publishing fixture",
        license = "MIT",
        repository = "https://github.com/LowkeyLab/bazel-repo",
        rust_version = "1.96",
        readme = "README.md",
        package_files = {"README.md": "README.md"},
        tags = ["manual"],
    )
    options.update(kwargs)
    cargo_publish(name = name, **options)

def _reject(name, message, library = ":library", **kwargs):
    _publish(name, library, **kwargs)
    _rejection_test(name = name + "_test", size = "small", tags = ["no-lint"], target_under_test = ":" + name + "_files", message = message)

def publishing_rejection_tests():
    """Declare supported-boundary fixtures without building rejected inputs."""
    _reject("escaping_asset", "package path must be relative", package_files = {"README.md": "../README.md"})
    _reject("conflicting_asset", "conflicting package path src/lib.rs", package_files = {"README.md": "src/lib.rs"})
    _reject("directory_conflict", "conflicting file/directory package path", package_files = {"README.md": "src"})
    _reject("reserved_path", "conflicts with generated Cargo file", package_files = {"README.md": "Cargo.toml"})
    _reject("missing_readme", "missing package_files readme path", package_files = {})
    for name, setting, value in [
        ("publisher_flags", "rustc_flags", ["--cfg=custom"]),
        ("publisher_environment", "rustc_env", {"CUSTOM": "value"}),
        ("publisher_features", "crate_features", ["custom"]),
        ("publisher_compile_data", "compile_data", ["README.md"]),
        ("publisher_alwayslink", "alwayslink", True),
    ]:
        rust_library(
            name = name + "_library",
            srcs = ["src/nested/value.rs"],
            crate_root = "src/nested/value.rs",
            edition = "2024",
            tags = ["manual"],
            **{setting: value}
        )
        diagnostic = "missing package_files mapping" if setting == "compile_data" else "unsupported publisher " + setting

        # Map no compile data so its explicit selection cannot be hidden by
        # the readme mapping in an otherwise valid publishing fixture.
        assets = {"LICENSE.txt": "README.md"} if setting == "compile_data" else {"README.md": "README.md"}
        _reject(name, diagnostic, library = ":" + name + "_library", package_files = assets)
    native.genrule(
        name = "generated_source",
        outs = ["generated.rs"],
        cmd = "echo 'pub const ANSWER: u32 = 42;' > $@",
        tags = ["manual"],
    )
    rust_library(
        name = "generated_library",
        srcs = [":generated_source"],
        crate_root = ":generated_source",
        edition = "2024",
        tags = ["manual"],
    )
    _reject("generated_input", "unsupported generated/escaping source", library = ":generated_library")
    rust_proc_macro(
        name = "unsupported_library",
        srcs = ["src/nested/value.rs"],
        crate_root = "src/nested/value.rs",
        edition = "2024",
        tags = ["manual"],
    )
    _reject("unsupported_type", "unsupported library crate type proc-macro", library = ":unsupported_library")

    # Lint aspects cannot register actions on analysis_test=true rules.
    for name, test in [
        ("global_flags", _extra_flags_test),
        ("global_singular_flag", _singular_flag_test),
        ("global_environment", _extra_env_test),
        ("per_crate_setting", _per_crate_test),
        ("exec_flags", _exec_flags_test),
        ("exec_singular_flag", _exec_singular_test),
        ("exec_environment", _exec_env_test),
    ]:
        test(name = name + "_test", size = "small", tags = ["no-lint"], target_under_test = ":publish_files", message = "unsupported compiler settings")

    for name, setting, value in [
        ("binary_dylib", "link_std_dylib", True),
        ("binary_linker_script", "linker_script", "README.md"),
        ("binary_stamping", "stamp", 1),
        ("conditional_binary_flags", "rustc_flags", select({"//conditions:default": ["--cfg=custom"]})),
        ("binary_environment_file", "rustc_env_files", ["README.md"]),
    ]:
        rust_binary(name = name + "_target", srcs = ["src/bin/main.rs"], edition = "2024", tags = ["manual"], **{setting: value})
        diagnostic = "unsupported publisher " + setting
        if setting == "rustc_env_files":
            diagnostic = "unsupported publisher rustc_env/rustc_env_files"
        _reject(name, diagnostic, library = None, binaries = {":" + name + "_target": "fixture-cli"})
    cargo_build_script(name = "publisher_script", srcs = ["src/bin/main.rs"], edition = "2024", tags = ["manual"])
    rust_library(name = "script_library", srcs = ["src/lib.rs", "src/nested/value.rs"], crate_root = "src/lib.rs", edition = "2024", deps = [":publisher_script"], tags = ["manual"])
    _reject("publisher_build_script", "unsupported dependency", library = ":script_library")
    _reject("generated_binary", "unsupported generated/escaping source", library = None, binaries = {":generated_binary_target": "fixture-cli"})
    rust_binary(name = "generated_binary_target", srcs = [":generated_source"], edition = "2024", tags = ["manual"])
    _reject("selected_test_as_binary", "unsupported binary crate type", library = None, binaries = {":unit_test": "fixture-cli"}, testonly = True)
    _reject("selected_binary_as_test", "selected test must be a local Rust test target", tests = [":binary"])
