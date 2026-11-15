"""Exercise real ESLint status capture with the pinned rules_js launcher."""

load("@rules_lint//lint:eslint.bzl", "lint_eslint_aspect")
load("@rules_lint//lint:lint_test.bzl", "lint_test")

_ARGS = [
    "--no-config-lookup",
    "--config",
    "3rdparty/rules_lint/tests/eslint.config.cjs",
]

_capture = lint_eslint_aspect(
    binary = Label("//tools/lint:eslint"),
    configs = [Label(":config")],
    rule_kinds = ["filegroup"],
    args = _ARGS,
)

_tool_error = lint_eslint_aspect(
    binary = Label("//tools/lint:eslint"),
    configs = [Label(":config")],
    rule_kinds = ["filegroup"],
    args = _ARGS + ["--invalid-capture-test-option"],
)

eslint_capture_test = lint_test(aspect = _capture)
eslint_tool_error_test = lint_test(aspect = _tool_error)
