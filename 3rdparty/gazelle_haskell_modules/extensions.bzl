"""Bzlmod bridge for Tweag's WORKSPACE-oriented gazelle_haskell_modules."""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")

_GAZELLE_HASKELL_MODULES_COMMIT = "282cc3b672f91f379666c67f4aabce073bc08c61"

def _gazelle_haskell_modules_dependencies_impl(repository_ctx):
    repository_ctx.file(
        "BUILD.bazel",
        content = """package(default_visibility = ["//visibility:public"])

alias(
    name = "json",
    actual = "{json}",
)
""".format(json = repository_ctx.attr.json),
        executable = False,
    )

gazelle_haskell_modules_dependencies = repository_rule(
    implementation = _gazelle_haskell_modules_dependencies_impl,
    attrs = {
        "json": attr.label(default = "@stackage//:json"),
    },
    local = True,
)

def _gazelle_haskell_modules_buildtools_bridge_impl(repository_ctx):
    repository_ctx.file(
        "BUILD.bazel",
        content = """package(default_visibility = ["//visibility:public"])
""",
        executable = False,
    )
    repository_ctx.file(
        "build/BUILD.bazel",
        content = """package(default_visibility = ["//visibility:public"])

alias(
    name = "build",
    actual = "{buildtools}",
)
""".format(buildtools = repository_ctx.attr.buildtools),
        executable = False,
    )

gazelle_haskell_modules_buildtools_bridge = repository_rule(
    implementation = _gazelle_haskell_modules_buildtools_bridge_impl,
    attrs = {
        # gazelle_haskell_modules is WORKSPACE-era and expects a buildtools repo.
        # In Bzlmod, Gazelle's own go_deps extension owns the single compatible
        # buildtools copy; using a second go_deps instance creates duplicate Go
        # packages at link time. There is no stable apparent repository name for
        # another module extension's generated repo here, so keep the canonical
        # label isolated behind this local alias bridge instead of embedding it
        # in the patched upstream BUILD file.
        "buildtools": attr.label(default = "@@gazelle++go_deps+com_github_bazelbuild_buildtools//build:build"),
    },
    local = True,
)

def _gazelle_haskell_modules_impl(module_ctx):
    root = module_ctx.modules[0]
    json = "@stackage//:json"
    if root.tags.deps:
        if len(root.tags.deps) > 1:
            fail("gazelle_haskell_modules.deps may be called at most once")
        json = root.tags.deps[0].json

    gazelle_haskell_modules_buildtools_bridge(
        name = "gazelle_haskell_modules_buildtools_bridge",
    )

    http_archive(
        name = "io_tweag_gazelle_haskell_modules",
        integrity = "sha256-Qkun7BVbnxL8z/1SkTRDzO2FGAETuiaxEFyIUHoH0gs=",
        patch_args = ["-p1"],
        patches = ["//3rdparty/gazelle_haskell_modules:gazelle_haskell_modules_bzlmod.patch"],
        strip_prefix = "gazelle_haskell_modules-{}".format(_GAZELLE_HASKELL_MODULES_COMMIT),
        urls = ["https://github.com/tweag/gazelle_haskell_modules/archive/{}.tar.gz".format(_GAZELLE_HASKELL_MODULES_COMMIT)],
    )

    gazelle_haskell_modules_dependencies(
        name = "io_tweag_gazelle_haskell_modules_deps",
        json = json,
    )

gazelle_haskell_modules = module_extension(
    implementation = _gazelle_haskell_modules_impl,
    tag_classes = {
        "deps": tag_class(attrs = {
            "json": attr.label(default = "@stackage//:json"),
        }),
    },
)
