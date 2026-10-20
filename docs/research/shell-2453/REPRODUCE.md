# Reproduce the bounded Shell collector experiment

Use the recorded repository base and the pinned collector archive in this package.
The dependency expression uses the repository's locked Nixpkgs.
The configured BuildBuddy services handle supported Bazel work.
The collector genrule carries `local` and `no-remote` tags.

```bash
umask 077
research_output=$(mktemp -d)

nix develop --impure --file docs/research/shell-2453/build-deps.nix \
  --command bazel --host_jvm_args=-Xmx1536m run \
  --config=ci-remote --jobs=8 \
  --local_resources=cpu=2 --local_resources=memory=2048 \
  --action_env=PATH --action_env=NIX_CFLAGS_COMPILE \
  --action_env=NIX_LDFLAGS --action_env=PKG_CONFIG_PATH \
  --action_env=CC --action_env=CXX \
  //docs/research/shell-2453:probe -- "$research_output/run"
```

The probe uses kcov's actual `--bash-parser` and `--bash-parse-files-in-dirs` options.
It records each command, native XML report, functional output, and return status.
The script executes the following bounded controls:

1. Collect a zero baseline with a separate no-op driver.
2. Execute the standalone script, sourced helper, child, and multiline condition.
3. Disable child xtrace while the functional assertions still pass.
4. Execute the child with its own report. Remove its required XML. Repeat the check with corrupt XML.
5. Execute one extracted workflow command and one extracted launcher with controlled external tools.
6. Convert native line records through the explicit original-source mappings.

The native missing and corrupt checks occur after kcov exits.
The adapter raises on each invalid required report.
The validation probe catches those expected errors and records them.
It does not claim that native kcov rejected those faults.

Merge the independently collected baseline with the converted measured report through Bazel's pinned LCOV merger.

```bash
printf '%s\n' "$research_output/run/baseline.lcov" \
  "$research_output/run/combined.lcov" > "$research_output/forward.txt"
printf '%s\n' "$research_output/run/combined.lcov" \
  "$research_output/run/baseline.lcov" > "$research_output/reverse.txt"

for merge_order in forward reverse; do
  nix develop --command bazel --host_jvm_args=-Xmx1536m run \
    --config=ci-remote --jobs=8 \
    --local_resources=cpu=2 --local_resources=memory=2048 \
    @bazel_tools//tools/test:coverage_report_generator -- \
    --reports_file="$research_output/$merge_order.txt" \
    --output_file="$research_output/$merge_order.lcov"
done
cmp "$research_output/forward.lcov" "$research_output/reverse.lcov"
```

The published XML copies replace the private fixture root with `[FIXTURE_ROOT]`.
They also normalize timestamps to zero.
The observation and command records replace private output paths with `[OUTPUT_ROOT]` and `[COLLECTOR]`.
These changes preserve source names, original offsets, line numbers, hit counts, and fault diagnostics.
Private raw receipts preserve the original paths and unmodified outputs.
No credential headers or private worker identities occur in the published evidence.
