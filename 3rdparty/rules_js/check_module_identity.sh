#!/usr/bin/env bash
# Check the module identity and transitive aliases in a reduced Bazel graph.
set -euo pipefail

if [[ $# != 2 ]]; then
	printf 'Usage: %s REPOSITORY EVIDENCE_DIRECTORY\n' "$0" >&2
	exit 2
fi
repository=$(cd "$1" && pwd)
mkdir -p "$2"
evidence=$(cd "$2" && pwd)
# Do not overwrite evidence from another execution.
fixture="$evidence/fixture"
mkdir "$fixture"
bazel_command=${BAZEL:-bazel}

printf 'module(name = "rules_js_identity_probe")\n' >"$fixture/MODULE.bazel"
for module in rules_js rules_lint rules_lint_rust aspect_rules_esbuild aspect_rules_swc aspect_rules_ts rules_angular rules_nodejs; do
	# Select complete declarations from the current root, not copied dependency pins.
	awk -v module="$module" '
    /^bazel_dep\(/ && index($0, "name = \"" module "\"") { print; dependency++ }
    /^git_override\(/ { block = $0 "\n"; collecting = 1; next }
    collecting {
      block = block $0 "\n"
      if ($0 == ")") {
        if (index(block, "module_name = \"" module "\"")) {
          printf "%s", block
          override++
        }
        collecting = 0
      }
    }
    END { if (dependency != 1 || override != 1) exit 1 }
  ' "$repository/MODULE.bazel" >>"$fixture/MODULE.bazel"
done

# Keep the production typing patch in the graph as well as the identity patches.
mkdir -p "$fixture/tools/lint"
cp "$repository/tools/lint/python_typing.patch" "$fixture/tools/lint/"
printf 'exports_files(["python_typing.patch"])\n' >"$fixture/tools/lint/BUILD.bazel"
for package in rules_lint rules_esbuild rules_swc rules_ts rules_angular; do
	mkdir -p "$fixture/3rdparty/$package"
	cp "$repository/3rdparty/$package/rules_js_module.patch" "$fixture/3rdparty/$package/"
	printf 'exports_files(glob(["*.patch"]))\n' >"$fixture/3rdparty/$package/BUILD.bazel"
done
cp "$repository/3rdparty/rules_lint/rust_rules_js_module.patch" "$fixture/3rdparty/rules_lint/"
cp "$repository/3rdparty/rules_angular/fix.patch" "$fixture/3rdparty/rules_angular/"
mkdir -p "$fixture/tools/coverage"
cp "$repository/tools/coverage/rules_js_runfiles.patch" "$fixture/tools/coverage/"
printf 'exports_files(["rules_js_runfiles.patch"])\n' >"$fixture/tools/coverage/BUILD.bazel"
cp "$fixture/MODULE.bazel" "$evidence/module.good"

mapping() {
	(cd "$fixture" && "$bazel_command" --batch --ignore_all_rc_files mod dump_repo_mapping "$@")
}
assert_aliases() {
	local line count=0
	while IFS= read -r line; do
		case "$line" in
		\{*)
			case "$line" in *'"aspect_rules_js":"rules_js+"'*) count=$((count + 1)) ;; *) return 1 ;; esac
			case "$line" in *'"aspect_rules_js+'*) return 1 ;; esac
			;;
		esac
	done <"$1"
	[[ "$count" == 7 ]]
}

# The old root declaration must trigger upstream module-name validation.
awk '
  { gsub(/bazel_dep\(name = "rules_js", repo_name = "aspect_rules_js"\)/, "bazel_dep(name = \"aspect_rules_js\")")
    gsub(/module_name = "rules_js"/, "module_name = \"aspect_rules_js\"")
    print }
' "$evidence/module.good" >"$fixture/MODULE.bazel"
status=0
mapping '' >"$evidence/identity-red.log" 2>&1 || status=$?
[[ "$status" == 37 ]]
grep -Fq 'declares a different name (rules_js)' "$evidence/identity-red.log"
printf 'Old identity rejected: exit %s\n' "$status"

cp "$evidence/module.good" "$fixture/MODULE.bazel"
mapping '' rules_lint+ rules_lint_rust+ aspect_rules_esbuild+ aspect_rules_swc+ aspect_rules_ts+ rules_angular+ >"$evidence/mapping-green.log" 2>&1
assert_aliases "$evidence/mapping-green.log"
printf 'Root and six integrations resolve to rules_js+.\n'

# Prove the assertion detects a second, legacy provider implementation.
awk '!index($0, "//3rdparty/rules_ts:rules_js_module.patch")' "$evidence/module.good" >"$fixture/MODULE.bazel"
mapping '' rules_lint+ rules_lint_rust+ aspect_rules_esbuild+ aspect_rules_swc+ aspect_rules_ts+ rules_angular+ >"$evidence/transitive-red.log" 2>&1
if assert_aliases "$evidence/transitive-red.log"; then
	printf 'The check accepted the unpatched TS dependency.\n' >&2
	exit 1
fi
grep -Fq '"aspect_rules_js":"aspect_rules_js+"' "$evidence/transitive-red.log"
printf 'Unpatched TS alias rejected by the shared-identity check.\n'

cp "$evidence/module.good" "$fixture/MODULE.bazel"
mapping '' rules_lint+ rules_lint_rust+ aspect_rules_esbuild+ aspect_rules_swc+ aspect_rules_ts+ rules_angular+ >"$evidence/mapping-final.log" 2>&1
assert_aliases "$evidence/mapping-final.log"
printf 'PASS: reduced-graph identity checks. Full repository acceptance is separate.\n'
