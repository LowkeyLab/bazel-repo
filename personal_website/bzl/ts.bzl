"""TypeScript rule adapters for the Personal Website."""

load("@aspect_rules_ts//ts:defs.bzl", "ts_project")

def personal_website_ts_library(name, srcs, tsconfig_types = None, **kwargs):
    """Builds a reusable Personal Website TypeScript library."""
    kwargs.setdefault("source_map", True)
    ts_project(
        name = name,
        srcs = srcs,
        assets = srcs,
        allow_js = True,
        declaration = True,
        extends = "//personal_website:tsconfig",
        preserve_jsx = True,
        tsconfig = {
            "include": [],
            "compilerOptions": {
                "inlineSources": kwargs["source_map"],
                "allowImportingTsExtensions": False,
                "noEmit": False,
                "types": ["astro/client"] + (tsconfig_types or []),
            },
        },
        **kwargs
    )

def personal_website_ts_config(name, srcs, tsconfig_types = None, **kwargs):
    """Type-checks a Personal Website tooling configuration without emitting JS."""
    ts_project(
        name = name,
        srcs = srcs,
        allow_js = True,
        no_emit = True,
        preserve_jsx = True,
        extends = "//personal_website:tsconfig",
        tsconfig = {
            "include": [],
            "compilerOptions": {
                "types": ["astro/client"] + (tsconfig_types or []),
            },
        },
        **kwargs
    )

def personal_website_ts_test_sources(name, **kwargs):
    """Compiles Gazelle-discovered tests for the individual js_test runners."""
    personal_website_ts_library(name = name, testonly = True, **kwargs)
