"""
Load common test dependencies for Mindreadr app tests.
"""

load("//bzl:kotlin.bzl", "kt_junit5_test")

def http_test(name):
    kt_junit5_test(
        name = name,
        size = "small",
        srcs = ["{}.kt".format(name)],
        test_package = "io.lowkeylab.mindreadr.app",
        runtime_deps = ["//mindreadr/src/test:test_runtime_deps"],
        deps = [
            "//mindreadr/src/main/io/lowkeylab/mindreadr",
            "//mindreadr/src/main/io/lowkeylab/mindreadr/game",
            "@maven//:io_ktor_ktor_client_content_negotiation_jvm",
            "@maven//:io_ktor_ktor_client_websockets_jvm",
            "@maven//:io_ktor_ktor_server_test_host_jvm",
        ],
    )
