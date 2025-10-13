"""Angular application macros for Bazel builds"""

load("@aspect_bazel_lib//lib:copy_to_directory.bzl", "copy_to_directory")
load("@aspect_rules_esbuild//esbuild:defs.bzl", "esbuild")
load("//tools/angular:ng.bzl", "ng_esbuild", "ng_project")

# Common dependencies of Angular applications
APPLICATION_DEPS = [
    "//:node_modules/@angular/common",
    "//:node_modules/@angular/core",
    "//:node_modules/@angular/forms",
    "//:node_modules/@angular/platform-browser",
    "//:node_modules/@types/node",
    "//:node_modules/rxjs",
    "//:node_modules/tslib",
]

NG_DEV_DEFINE = {
    "process.env.NODE_ENV": "'development'",
    "ngJitMode": "false",
}
NG_PROD_DEFINE = {
    "process.env.NODE_ENV": "'production'",
    "ngDevMode": "false",
    "ngJitMode": "false",
}

def ng_application(name, deps = [], assets = None, html_assets = None, visibility = ["//visibility:public"]):
    """
    Bazel macro for compiling an Angular application. Creates {name}, prod, dev targets.

    Projects structure:
      main.ts
      index.html
      polyfills.ts
      styles.css
      app/
        **/*.{ts,css,html}

    Args:
      name: the rule name
      deps: direct dependencies of the application
      assets: assets to include in the file bundle
      html_assets: assets to insert into the index.html
      visibility: visibility of the primary targets ({name})
    """
    assets = assets if assets else native.glob(["assets/**/*"], allow_empty = True)
    html_assets = html_assets if html_assets else []

    app_srcs = native.glob(
        ["main.ts", "app/**/*"],
    )

    # Primary app source
    ng_project(
        name = "_app",
        srcs = app_srcs,
        deps = deps + APPLICATION_DEPS,
        visibility = ["//visibility:private"],
    )

    # App polyfills source + bundle.
    ng_project(
        name = "_polyfills",
        srcs = ["polyfills.ts"],
        deps = ["//:node_modules/zone.js"],
        visibility = ["//visibility:private"],
    )
    esbuild(
        name = "polyfills-bundle",
        entry_point = "polyfills.js",
        srcs = [":_polyfills"],
        define = {"process.env.NODE_ENV": "'production'"},
        config = {
            "resolveExtensions": [".mjs", ".js"],
        },
        metafile = False,
        format = "esm",
        minify = True,
        visibility = ["//visibility:private"],
    )

    _pkg_web(
        name = "prod",
        entry_point = "main.js",
        entry_deps = [":_app"],
        html_assets = html_assets,
        assets = assets,
        production = True,
        visibility = ["//visibility:private"],
    )

    _pkg_web(
        name = "dev",
        entry_point = "main.js",
        entry_deps = [":_app"],
        html_assets = html_assets,
        assets = assets,
        production = False,
        visibility = ["//visibility:private"],
    )

    # The default target: the prod package
    native.alias(
        name = name,
        actual = "prod",
        visibility = visibility,
    )

def _pkg_web(name, entry_point, entry_deps, html_assets, assets, production, visibility):
    """ Bundle and create runnable web package.

      For a given application entry_point, assets and defined constants... generate
      a bundle using that entry and constants, package all content into a resulting directory.
    """

    bundle = "bundle-%s" % name

    ng_esbuild(
        name = bundle,
        entry_points = [entry_point],
        srcs = entry_deps,
        define = NG_PROD_DEFINE if production else NG_DEV_DEFINE,
        format = "esm",
        output_dir = True,
        splitting = True,
        metafile = False,
        minify = production,
        visibility = ["//visibility:private"],
    )

    copy_to_directory(
        name = name,
        srcs = [":%s" % bundle, ":polyfills-bundle", ":index.html"] + html_assets + assets,
        root_paths = ["."],
        visibility = visibility,
    )
