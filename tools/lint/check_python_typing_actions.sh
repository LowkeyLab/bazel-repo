#!/usr/bin/env bash
# Check real first-party lint action arguments and declared dependency inputs.
set -euo pipefail

if [[ $# != 4 ]]; then
	printf 'Usage: %s REPOSITORY NEW_EVIDENCE_DIRECTORY VERSION OTHER_VERSION\n' "$0" >&2
	exit 2
fi
repository=$(cd "$1" && pwd)
evidence=$2
version=$3
other_version=$4
[[ "$version" =~ ^[0-9]+\.[0-9]+$ && "$other_version" =~ ^[0-9]+\.[0-9]+$ ]]
[[ "$version" != "$other_version" ]]
# Stop rather than overwrite an earlier run, and keep action output private.
(
	umask 077
	mkdir "$evidence"
)
evidence=$(cd "$evidence" && pwd)
bazel_command=${BAZEL:-bazel}

check_actions() {
	local selected=$1 output=$2
	awk -v selected="$selected" -v ruff="py${selected//./}" '
    $1 == "ActionKey:" { keys++ }
    $1 == "Inputs:" {
      inputs++
      if (!index($0, "python_sample/cli.py") || !index($0, "python_sample/versions.py")) exit 1
      if (!index($0, "whl_install_") || !index($0, "packaging_")) exit 1
    }
    expect_ty { if ($1 != selected) exit 1; expect_ty = 0 }
    expect_ruff { if ($1 != ruff) exit 1; expect_ruff = 0 }
    $1 == "--python-version" { ty++; expect_ty = 1 }
    $1 == "--target-version" { ruff_count++; expect_ruff = 1 }
    $1 == "python_sample" { own_imports++ }
    END {
      if (keys != 4 || inputs != 4 || ty != 2 || ruff_count != 2 || own_imports != 2 || expect_ty || expect_ruff) exit 1
    }
  ' "$output"
}

for selected in "$version" "$other_version"; do
	output="$evidence/actions-$selected.txt"
	(
		cd "$repository"
		# bazel run can retain its output-base lock while this command runs.
		"$bazel_command" --batch "--output_base=$evidence/bazel-output" aquery \
			'mnemonic("AspectRulesLint(Ruff|Ty)", //python_sample:python_sample)' \
			--aspects=//tools/lint:linters.bzl%ruff,//tools/lint:linters.bzl%ty \
			--output_groups=rules_lint_human,rules_lint_machine \
			--output=text \
			"--@rules_python//python/config_settings:python_version=$selected" \
			--remote_executor= --remote_cache= --bes_backend=
	) >"$output" 2>"$evidence/actions-$selected.log"
	check_actions "$selected" "$output"
	awk '$1 == "ActionKey:" { print $2 }' "$output" | LC_ALL=C sort -u >"$evidence/keys-$selected.txt"
	[[ $(wc -l <"$evidence/keys-$selected.txt") == 4 ]]
done

# The assertion must reject arguments for a selector that was not used.
if check_actions 0.0 "$evidence/actions-$version.txt"; then
	printf 'The action check accepted an incorrect Python selector.\n' >&2
	exit 1
fi
LC_ALL=C comm -12 "$evidence/keys-$version.txt" "$evidence/keys-$other_version.txt" >"$evidence/shared-keys.txt"
[[ ! -s "$evidence/shared-keys.txt" ]]
printf 'PASS: both actions per checker use their selector, retain source/wheel inputs, and change action keys.\n'
printf 'Checker execution and Aspect strategy enforcement require separate validation.\n'
