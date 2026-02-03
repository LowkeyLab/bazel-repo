"Define linter aspects"

load("@aspect_rules_lint//lint:buf.bzl", "lint_buf_aspect")
load("@aspect_rules_lint//lint:checkstyle.bzl", "lint_checkstyle_aspect")
load("@aspect_rules_lint//lint:clippy.bzl", "lint_clippy_aspect")
load("@aspect_rules_lint//lint:eslint.bzl", "lint_eslint_aspect")
load("@aspect_rules_lint//lint:keep_sorted.bzl", "lint_keep_sorted_aspect")
load("@aspect_rules_lint//lint:ktlint.bzl", "lint_ktlint_aspect")
load("@aspect_rules_lint//lint:lint_test.bzl", "lint_test")
load("@aspect_rules_lint//lint:pmd.bzl", "lint_pmd_aspect")
load("@aspect_rules_lint//lint:ruff.bzl", "lint_ruff_aspect")
load("@aspect_rules_lint//lint:shellcheck.bzl", "lint_shellcheck_aspect")
load("@aspect_rules_lint//lint:stylelint.bzl", "lint_stylelint_aspect")

# Check proto_library sources, see https://buf.build/docs/lint/overview
buf = lint_buf_aspect(
    config = Label("//:buf.yaml"),
)

eslint = lint_eslint_aspect(
    binary = Label(":eslint"),
    # We trust that eslint will locate the correct configuration file for a given source file.
    # See https://eslint.org/docs/latest/use/configure/configuration-files#cascading-and-hierarchy
    configs = [
        Label("//:eslintrc"),
        # if the repository has nested eslintrc files, they must be added here as well
        Label("//personal_website:eslintrc"),
    ],
)

eslint_test = lint_test(aspect = eslint)

ruff = lint_ruff_aspect(
    binary = "@multitool//tools/ruff",
    configs = [Label("//:.ruff.toml")],
)

shellcheck = lint_shellcheck_aspect(
    binary = "@multitool//tools/shellcheck",
    config = Label("//:.shellcheckrc"),
)

pmd = lint_pmd_aspect(
    binary = Label("//tools/lint:pmd"),
    rulesets = [Label("//:pmd.xml")],
)

checkstyle = lint_checkstyle_aspect(
    binary = Label("//tools/lint:checkstyle"),
    config = Label("//:checkstyle.xml"),
)

ktlint = lint_ktlint_aspect(
    binary = Label("@pinterest_ktlint//file"),
    editorconfig = Label("//:.editorconfig"),
    baseline_file = Label("//:.ktlint-baseline.xml"),
)

keep_sorted = lint_keep_sorted_aspect(
    binary = Label("@com_github_google_keep_sorted//:keep-sorted"),
)

stylelint = lint_stylelint_aspect(
    binary = Label("//tools/lint:stylelint"),
    config = Label("//:stylelintrc"),
)

clippy = lint_clippy_aspect(
    config = Label("//:.clippy.toml"),
)
