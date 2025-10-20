"""Angular build rules for Bazel

This module provides macros for building Angular applications with Bazel:
- ng_project: TypeScript compilation with Angular compiler
- ng_esbuild: Bundle Angular applications with esbuild and Angular linker
"""

load("@aspect_rules_esbuild//esbuild:defs.bzl", "esbuild")
load(":ts.bzl", "ts_project")

def ng_project(name, **kwargs):
    """The rules_js ts_project() configured with the Angular ngc compiler."""
    ts_project(
        name = name,

        # Compiler
        tsc = "//angular-ngc/tools:ngc",
        supports_workers = False,

        # Any other ts_project() or generic args
        **kwargs
    )

def ng_esbuild(name, **kwargs):
    """The rules_esbuild esbuild() configured with the Angular linker configuration
    """

    # Extract existing deps and add config dependencies
    deps = kwargs.pop("deps", [])

    esbuild(
        name = name,
        config = "//angular-ngc/tools:ngc.esbuild.mjs",
        deps = deps + [
            "//angular-ngc:node_modules/@angular/compiler-cli",
            "//angular-ngc:node_modules/@babel/core",
        ],
        **kwargs
    )
