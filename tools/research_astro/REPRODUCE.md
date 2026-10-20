# Reproduce the bounded experiment

Run these commands from the repository root.
Choose an absolute writable artifact directory.

```bash
artifact_dir=/absolute/path/to/astro-artifacts
limits=(--config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048)
nix develop --command bazel --host_jvm_args=-Xmx1536m run //tools/research_astro:server "${limits[@]}" -- "$artifact_dir/server"
nix develop --impure --file tools/research_astro/browser-deps.nix --command bazel --host_jvm_args=-Xmx1536m test //tools/research_astro:browser_test "${limits[@]}" --test_output=errors
cp -R bazel-testlogs/tools/research_astro/browser_test/test.outputs "$artifact_dir/browser"
nix develop --command bazel --host_jvm_args=-Xmx1536m run //tools/research_astro:merge "${limits[@]}" -- "$artifact_dir/server" "$artifact_dir/browser" "$artifact_dir/merge"
nix develop --command bazel --host_jvm_args=-Xmx1536m run //tools/research_astro:controls "${limits[@]}" -- "$PWD/bazel-bin/tools/research_astro/merge_/merge" "$artifact_dir/server" "$artifact_dir/browser" "$artifact_dir/controls"
```

The browser test declares its complete raw outputs.
The other probes write to the selected artifact directory.
The canonical LCOV is a partial candidate.
It excludes unresolved virtual-script coverage.
It retains observed mapping defects for review.
