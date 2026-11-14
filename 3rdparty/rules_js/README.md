# JavaScript module identity

The pinned upstream module declares `rules_js`.
The root uses `aspect_rules_js` as its apparent repository name.
The module patches for rules_lint, rules_lint_rust, esbuild, swc, TypeScript, and Angular retain that alias.
All seven aliases must resolve to the same canonical `rules_js+` repository.
This keeps existing loads and provider identities consistent without a dependency update.

## Focused regression check

From the repository root, use a new absolute evidence directory:

```bash
nix develop --command bazel run //3rdparty/rules_js:check_module_identity -- \
  "$PWD" /absolute/path/to/new-evidence
```

The command uses the current root declarations and local patches.
It creates a separate module in the evidence directory.
It does not change the root configuration.
It rejects an evidence directory that already contains a `fixture` directory.
Set `BAZEL` to select a different Bazel executable when necessary.

The check verifies these conditions:

- The old root module identity fails upstream name validation.
- The root and six integrations resolve their aliases to `rules_js+`.
- Removing the TypeScript module patch makes the shared-identity assertion fail.
- Restoring the patch makes the assertion pass again.

The fixture contains rules_js, rules_lint, rules_lint_rust, esbuild, swc, TypeScript, Angular,
rules_nodejs, and their transitive module dependencies.
It retains the root rules_lint Python typing patch as well as the module identity patches.
The combined check covers patch application for both #3125 and #3126.
It does not exercise Python typing policy.
The command saves complete mapping logs for the positive and negative checks.

## Limits

This is a reduced-graph check, not full repository acceptance.
It uses `--ignore_all_rc_files` to keep the probe independent of root build settings.
It does not validate first-party application builds, npm package builds, adapter
behavior, source membership, or combined coverage reconciliation.
Run the existing native collector and coverage acceptance commands separately.
Keep `tools/coverage/rules_js_runfiles.patch` and the existing coverage contracts.

Before merge, regenerate `MODULE.bazel.lock` and run full mapping, formatting,
lint, tests, build, and coverage through the
required Nix development workflow on the same final revision.
