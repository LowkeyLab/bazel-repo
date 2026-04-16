"""
Angular macros.
"""

load("@npm//angular:postcss-cli/package_json.bzl", postcss_cli = "bin")
load("@rules_angular//src/architect:ng_application.bzl", orig_ng_application = "ng_application")
load("@rules_angular//src/architect:ng_test.bzl", orig_ng_test = "ng_test")

def _process_styles_impl(name, visibility, src, out, config, deps, srcs):
    postcss_cli.postcss(
        name = name,
        visibility = visibility,
        srcs = [src, config] + deps + srcs + [
            "//angular:node_modules/@tailwindcss/postcss",
            "//angular:node_modules/postcss",
            "//angular:node_modules/tailwindcss",
        ],
        outs = [out],
        args = [
            "$(rootpath {})".format(src),
            "-o",
            "$(rootpath {})".format(out),
            "--config",
            "$(rootpath {})".format(config),
        ],
    )

process_styles = macro(
    doc = "Processes a CSS file using PostCSS with Tailwind CSS support.",
    implementation = _process_styles_impl,
    attrs = {
        "src": attr.label(
            doc = "The source CSS file.",
            configurable = False,
        ),
        "out": attr.output(
            doc = "The output CSS file.",
        ),
        "config": attr.label(
            doc = "The PostCSS configuration file.",
            default = "//angular:postcssrc",
            configurable = False,
        ),
        "deps": attr.label_list(
            doc = "Additional dependencies (e.g., daisyui).",
            default = [],
            configurable = False,
        ),
        "srcs": attr.label_list(
            doc = "Extra source files for content scanning.",
            default = [],
            configurable = False,
        ),
    },
)

def _ng_application_impl(name, visibility, zonejs, tailwindcss, deps, srcs):
    extra_deps = []
    if zonejs:
        extra_deps.append("//angular:node_modules/zone.js")
    if tailwindcss:
        extra_deps += [
            "//angular:node_modules/@tailwindcss/postcss",
            "//angular:node_modules/postcss",
            "//angular:node_modules/tailwindcss",
            "//angular:postcssrc",
        ]
    orig_ng_application(
        name = name,
        visibility = visibility,
        srcs = srcs,
        deps = deps + extra_deps,
        ng_config = "//angular:ng-config",
        node_modules = "//angular:node_modules",
    )

ng_application = macro(
    doc = "Defines an ng_application with optional dependencies on zone.js and tailwindcss.",
    implementation = _ng_application_impl,
    attrs = {
        "zonejs": attr.bool(
            doc = "If True, includes zone.js as a dependency.",
            default = False,
            configurable = False,
        ),
        "tailwindcss": attr.bool(
            doc = "If True, includes tailwindcss and related packages as dependencies.",
            default = False,
            configurable = False,
        ),
        "deps": attr.label_list(
            doc = "Additional dependencies to include.",
            default = [],
            configurable = False,
        ),
        "srcs": attr.label_list(
            doc = "Source files.",
            default = [],
            configurable = False,
        ),
    },
)

def _ng_test_impl(name, visibility, zonejs, tailwindcss, karma, vitest, deps, srcs):
    extra_deps = []
    if zonejs:
        extra_deps.append("//angular:node_modules/zone.js")
    if tailwindcss:
        extra_deps += [
            "//angular:node_modules/@tailwindcss/postcss",
            "//angular:node_modules/postcss",
            "//angular:node_modules/tailwindcss",
            "//angular:postcssrc",
        ]
    if karma:
        extra_deps += [
            # keep-sorted start
            "//angular:karma-bazel-conf",
            "//angular:node_modules/@types/jasmine",
            "//angular:node_modules/@types/node",
            "//angular:node_modules/jasmine-core",
            "//angular:node_modules/karma",
            "//angular:node_modules/karma-chrome-launcher",
            "//angular:node_modules/karma-coverage",
            "//angular:node_modules/karma-jasmine",
            "//angular:node_modules/karma-jasmine-html-reporter",
            # keep-sorted end
        ]
    if vitest:
        extra_deps += [
            # keep-sorted start
            "//angular:node_modules/@vitest/coverage-v8",
            "//angular:node_modules/jsdom",
            "//angular:node_modules/vitest",
            # keep-sorted end
        ]

    tags = []
    if karma:
        tags.append("no-remote")

    orig_ng_test(
        name = name,
        visibility = visibility,
        srcs = srcs,
        deps = deps + extra_deps,
        ng_config = "//angular:ng-config",
        node_modules = "//angular:node_modules",
        size = "small",
        tags = tags,
    )

ng_test = macro(
    doc = "Defines an ng_test with optional dependencies on zone.js, tailwindcss, karma, and vitest.",
    implementation = _ng_test_impl,
    attrs = {
        "zonejs": attr.bool(
            doc = "If True, includes zone.js as a dependency.",
            default = False,
            configurable = False,
        ),
        "tailwindcss": attr.bool(
            doc = "If True, includes tailwindcss and related packages as dependencies.",
            default = False,
            configurable = False,
        ),
        "karma": attr.bool(
            doc = "If True, includes karma as a dependency.",
            default = False,
            configurable = False,
        ),
        "vitest": attr.bool(
            doc = "If True, includes jsdom and vitest as dependencies.",
            default = False,
            configurable = False,
        ),
        "deps": attr.label_list(
            doc = "Additional dependencies to include.",
            default = [],
            configurable = False,
        ),
        "srcs": attr.label_list(
            doc = "Source files.",
            default = [],
            configurable = False,
        ),
    },
)
