"""
Angular macros.
"""

load("@npm//angular:postcss-cli/package_json.bzl", postcss_cli = "bin")
load("@rules_angular//src/architect:ng_application.bzl", orig_ng_application = "ng_application")
load("@rules_angular//src/architect:ng_test.bzl", orig_ng_test = "ng_test")

def process_styles(name, src, out, config = "//angular:postcssrc", deps = [], **kwargs):
    """
    Processes a CSS file using PostCSS with Tailwind CSS support.

    Args:
        name: The name of the target.
        src: The source CSS file.
        out: The output CSS file.
        config: The PostCSS configuration file (default: //angular:postcssrc).
        deps: Additional dependencies (e.g., daisyui).
        **kwargs: Additional arguments passed to js_run_binary (e.g. srcs for content scanning).
    """
    extra_srcs = kwargs.pop("srcs", [])

    postcss_cli.postcss(
        name = name,
        srcs = [src, config] + deps + extra_srcs + [
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

def ng_application(zonejs = False, tailwindcss = False, deps = [], **kwargs):
    """
    Defines an ng_application with optional dependencies on zone.js and tailwindcss.

    Args:
        zonejs (bool): If True, includes zone.js as a dependency.
        tailwindcss (bool): If True, includes tailwindcss and related packages as dependencies.
        deps (list): Additional dependencies to include.
        **kwargs: Additional keyword arguments passed to ng_application.
    """
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
        deps = deps + extra_deps,
        ng_config = "//angular:ng-config",
        node_modules = "//angular:node_modules",
        **kwargs
    )

def ng_test(zonejs = False, tailwindcss = False, karma = False, vitest = False, deps = [], **kwargs):
    """
    Defines an ng_test with optional dependencies on zone.js and tailwindcss.

    Args:
        zonejs (bool): If True, includes zone.js as a dependency.
        tailwindcss (bool): If True, includes tailwindcss and related packages as dependencies.
        karma (bool): If True, includes karma as a dependency.
        vitest (bool): If True, includes jsdom and vitest as dependencies.
        deps (list): Additional dependencies to include.
        **kwargs: Additional keyword arguments passed to ng_test.
    """
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
            "//angular:node_modules/jsdom",
            "//angular:node_modules/vitest",
            # keep-sorted end
        ]

    orig_ng_test(
        deps = deps + extra_deps,
        ng_config = "//angular:ng-config",
        node_modules = "//angular:node_modules",
        size = "small",
        **kwargs
    )
