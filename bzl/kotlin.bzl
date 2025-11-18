"""
Kotlin rules.
"""

load("@rules_kotlin//kotlin:jvm.bzl", "kt_jvm_test")

def kt_junit5_test(name, test_package, srcs = [], deps = [], runtime_deps = [], **kwargs):
    kt_jvm_test(
        name = name,
        srcs = srcs,
        main_class = "org.junit.platform.console.ConsoleLauncher",
        args = ["execute", "--select-package=" + test_package],
        deps = deps + [
            "@maven//:org_junit_jupiter_junit_jupiter_api",
            "@maven//:org_junit_jupiter_junit_jupiter_params",
            "@maven//:org_jetbrains_kotlin_kotlin_test",
            "@maven//:org_jetbrains_kotlin_kotlin_test_junit5",
        ],
        runtime_deps = runtime_deps + ["@maven//:org_junit_jupiter_junit_jupiter_engine", "@maven//:org_junit_platform_junit_platform_console"],
        **kwargs
    )
