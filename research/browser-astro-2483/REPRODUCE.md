# Reproduce the bounded local checks

Use the repository Nix shell and the research branch.
Choose an absolute artifact directory outside the checkout.

```bash
artifact_dir=/absolute/path/to/browser-astro-artifacts
local_flags=(--config=ci-remote --remote_executor= --remote_cache= --bes_backend= --remote_upload_local_results=false --jobs=4 --local_resources=cpu=2 --local_resources=memory=2048)
local_bazel() {
  local command="$1"
  shift
  nix develop --command bazel --host_jvm_args=-Xmx1536m "$command" "${local_flags[@]}" "$@"
}

local_bazel run //:gazelle
local_bazel run //tools/research_astro:server -- "$artifact_dir/server"
local_bazel build //personal_website:build //tools/research_astro:site_build @research_astro_chromium//:chrome-headless-shell
local_bazel cquery @research_astro_chromium//:chrome-headless-shell --output=files
```

Resolve the returned Chromium path under Bazel's execution root.
The recorded run used this executable:

```text
/home/tacascer/.cache/bazel/_bazel_tacascer/40e4681fd78d9a4fcb24254f7d554ad5/external/+http_archive+research_astro_chromium/chrome-headless-shell
```

Run the Node driver and its Chromium child locally:

```bash
browser_executable=/absolute/path/to/the/pinned/chrome-headless-shell
TEST_UNDECLARED_OUTPUTS_DIR="$artifact_dir/browser" \
  nix develop --impure --file tools/research_astro/browser-deps.nix \
  --command bash -c \
  'export LD_LIBRARY_PATH="$NIX_LD_LIBRARY_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"; exec bazel "$@"' \
  browser-local --host_jvm_args=-Xmx1536m run "${local_flags[@]}" \
  //tools/research_astro:browser -- \
  "$PWD/bazel-bin/tools/research_astro/dist" \
  "$browser_executable" "$PWD/book_smartz/web/submitting.js" \
  "$PWD/bazel-bin/personal_website/dist"

local_bazel run //tools/research_astro:merge -- \
  "$artifact_dir/server" "$artifact_dir/browser" "$artifact_dir/merge"
local_bazel run //tools/research_astro:chain -- \
  "$artifact_dir/server" "$artifact_dir/merge" "$artifact_dir/chain"
```

The maintained route uses the existing production build artifact.
The browser interval performs no Astro compilation.
The server probe provides separate fresh template execution.
The chain probe analyzes saved records without executing components.
The mapper requires `browser` beside `merge` in the artifact directory.

For each negative control, copy `server`, `browser`, and `merge` into a new private directory.
Preserve `browser/maintained-functional.json` with `passed=true`.
Change exactly one required artifact:

- Remove `browser/maintained-profile.json`.
- Replace that profile with invalid JSON.
- Remove `browser/maintained-completion.json`.
- Remove `browser/maintained-script-6.js`.
- Remove the fixture's `.map` file from `browser`.

Run the chain probe against each changed directory.
Require exit one for every control.
The compiler-only semantic mutation control resides inside the chain probe.
It must reject the changed virtual script structure.

The exported evidence adds `.txt` to raw `.json`, `.js`, `.mjs`, `.map`, and `.html` filenames.
Restore the original names before replaying saved artifacts.
The final manifests identify source and artifact hashes separately.
