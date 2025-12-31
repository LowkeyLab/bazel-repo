"""
Angular macros.
"""

load("@rules_angular//src/architect:ng_application.bzl", orig_ng_application = "ng_application")
load("@rules_angular//src/architect:ng_test.bzl", orig_ng_test = "ng_test")

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

def ng_test(zonejs = False, tailwindcss = False, karma = False, deps = [], **kwargs):
    """
    Defines an ng_test with optional dependencies on zone.js and tailwindcss.

    Args:
        zonejs (bool): If True, includes zone.js as a dependency.
        tailwindcss (bool): If True, includes tailwindcss and related packages as dependencies.
        karma (bool): If True, includes karma as a dependency.
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
            "//angular:node_modules/@types/node",
            "//angular:node_modules/karma",
            "//angular:node_modules/karma-chrome-launcher",
            "//angular:node_modules/karma-coverage",
            "//angular:node_modules/karma-jasmine",
            "//angular:node_modules/karma-jasmine-html-reporter",
            "//angular:node_modules/@types/jasmine",
            "//angular:node_modules/jasmine-core",
            # keep-sorted end
        ]

    orig_ng_test(
        deps = deps + extra_deps,
        ng_config = "//angular:ng-config",
        node_modules = "//angular:node_modules",
        size = "small",
        **kwargs
    )
