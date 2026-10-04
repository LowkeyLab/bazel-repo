# Gazelle Rust Local Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the locally maintained gazelle_rust baseline required by #1956, preserving application generation behavior before #1957.

**Architecture:** Vendor the pinned upstream as a nested Bazel module, incorporating the existing compatibility patch. Connect it through local_path_override while excluding its source and fixtures from main-repository traversal. Compare old and new generation against identical inputs and retain upstream parser and generation tests.

**Tech Stack:** Bazel 9.2.0, Aspect, Nix development shell, Go/Gazelle, Rust parser, protobuf, rules_rust internals and rules_rs generated application loads.

**Spec:** [Approved stage-1 design](../specs/2026-10-04-gazelle-rust-local-baseline-design.md).

## Global Constraints

- Pin: `747482bfad721b0fe7198c94491d76c32b263ac0`, upstream `https://github.com/Calsign/gazelle_rust.git`; recheck for intervening changes.
- Destination: `3rdparty/gazelle_rust/upstream`; keep licence, package structure, Go imports, protobuf generation, and parser runfiles.
- Preserve both adaptations in `3rdparty/gazelle_rust/rules_rs_loads.patch`, including parser library edition `2024`.
- Application generation behavior remains identical to the currently patched upstream baseline.
- Preserve existing application exclusions; do not migrate targets, remove modes, or implement #1957.
- Use Bazel/Aspect through `nix develop --command`; never invoke cargo, npm, or go directly.
- Run `bazel run //:gazelle` immediately after source edits and before formatting. Run it before manual BUILD changes.
- Do not disable upstream tests to obtain a green result. Report environmental blockers with commands and evidence.
- Keep baseline implementation separately reviewable. Do not push, create a PR, or merge.

## Review Focus

1. Existing generator drift: distinguish output already changed by the old generator from differences caused by vendoring (Task 1 comparison).
2. Nested fixture workspaces: root Gazelle and root package queries must leave vendored content isolated (Task 1 isolation check).
3. External-repository execution: parser runfiles and protobuf dependencies must work under the main repository's module mapping (Tasks 1 and 2 tests).
4. Fixture load compatibility: preserve non-load expectations and dependency-resolution coverage when adapting rules_rs goldens (Task 2 fixture comparison).
5. Source provenance: a warmed Bazel cache may contain patched files and metadata; compare against the exact upstream tracked tree instead of treating the cache as pristine (Task 1 provenance check).

## File Map

- Create `3rdparty/gazelle_rust/upstream/`: tracked upstream tree, retaining `MODULE.bazel`, lockfiles, `go.mod`, `LICENSE`, `rust_language/`, `rust_parser/`, `gazelle_rust_parser/`, `proto/`, `generation_tests/`, and supporting packages.
- Modify copied `rust_language/lang.go` and `gazelle_rust_parser/src/BUILD.bazel`: incorporate current patch only.
- Modify root `MODULE.bazel`, `BUILD.bazel`, and `.bazelignore`: local override and traversal isolation; update `MODULE.bazel.lock` only if Bazel legitimately changes it.
- Create `3rdparty/gazelle_rust/README.md`: provenance, adaptations, maintenance commands, validation evidence.
- Remove `3rdparty/gazelle_rust/rules_rs_loads.patch`; update its parent `BUILD.bazel` to export the README after generation has run.
- Modify copied `generation_tests/**/BUILD.out` and any other actual golden output files only where expected loads require the existing compatibility adaptation; enumerate exact files from failed fixture diffs.
- Modify copied test/build wiring only for demonstrated integration failures; record every such departure from upstream.

## Task 1: Integrate the local module and prove unchanged application output

**Interfaces:** Consumes the pinned upstream tree, current patch, and clean repository inputs. Produces the `@gazelle_rust` local module, unchanged `@gazelle_rust//rust_language` integration, and saved baseline comparison evidence for Task 2. No new product API.

- [ ] **1. Establish the checkpoint.** Read applicable AGENTS.md files, inspect `git status --short`, confirm the pin and patch, and record the starting commit. Reuse this isolated worktree. Keep temporary validation snapshots outside the checkout and never overwrite user edits.
- [ ] **2. Capture old generation behavior.** Inventory application BUILD/BUILD.bazel files and their bytes, excluding existing ignored directories. Run `nix develop --command bazel run //:gazelle`; save the resulting file inventory and content. Record tracked diffs and new files. Restore only changes attributable to this run from the saved pre-run inputs so the local generator can later receive identical inputs. Do not use a blanket reset/clean.
- [ ] **3. Verify upstream provenance.** Obtain the exact commit's tracked archive through Git, compare the existing patch against it, and record its source and revision. A cached Bazel tree may help inspection but is not the authority for pristine contents. Verify the current patch applies exactly once.
- [ ] **4. Prepare isolation and vendor.** Add `3rdparty/gazelle_rust/upstream` to `.bazelignore`. Having run Gazelle in step 2, add the matching root `gazelle:exclude` directive while preserving existing exclusions. Copy the tracked archive to the destination and immediately run root Gazelle as required; verify the new nested tree is unchanged. Apply the lang.go patch and immediately run Gazelle again. Incorporate the parser BUILD edition change, then replace only the gazelle_rust git_override with `local_path_override(module_name = "gazelle_rust", path = "3rdparty/gazelle_rust/upstream")`. Keep the existing root language entry.
- [ ] **5. Prove integration works.** Run `nix develop --command aspect build //:gazelle_bin @gazelle_rust//rust_parser:rust_parser @gazelle_rust//proto:messages_go @gazelle_rust//proto:messages_rust_proto`. Expected: success with the local module. Use `nix develop --command bazel mod show_repo gazelle_rust` to inspect repository provenance. Resolve only demonstrated integration failures.
- [ ] **6. Compare application generation.** Restore the original application inputs if preparatory Gazelle runs changed them, preserving intentional root integration edits. Run `nix develop --command bazel run //:gazelle`. Compare application BUILD file membership and bytes to step 2's old-generator output; expected zero differences. Assess root BUILD separately, allowing only the planned exclusion. Save the full post-run inventory, including the vendored tree, run Gazelle again, and require identical content and file membership. Restore unrelated pre-existing generator drift after recording it, so it is not shipped as this change.
- [ ] **7. Verify package isolation.** Run `nix develop --command bazel query //... --output=label`; assert there are no labels beginning `//3rdparty/gazelle_rust/upstream`. The vendored file inventory from step 6 must show no root-generation changes. If either check fails, repair exclusions and repeat the affected checks.
- [ ] **8. Document and retire the patch.** Write README provenance with exact upstream revision and the two adaptations, remove the old patch, and replace its parent BUILD export with the README. Do not remove the upstream licence. Compare the vendored tree with upstream: only the documented compatibility and justified integration edits may differ.
- [ ] **9. Validate and commit the integration.** Run `nix develop --command aspect format --scope=all`, inspect all changes including upstream fixtures, and run `git diff --check`. Verify formatting did not introduce an application behavior change. Stage only the intended integration files and commit as `build: maintain gazelle Rust as a local module`. Retain baseline comparison evidence for the final report.

## Task 2: Preserve upstream test coverage and close stage-1 acceptance

**Interfaces:** Consumes Task 1's local module and baseline evidence. Produces passing focused coverage, final repository validation evidence, and documented maintenance commands. Test entrypoints remain upstream Bazel targets.

- [ ] **1. Run the existing focused tests before modifying fixtures.** Run `nix develop --command aspect test @gazelle_rust//gazelle_rust_parser/tests:parse_test @gazelle_rust//generation_tests:all`. Record actual test counts and failures. These existing behavioral tests provide the regression oracle; do not invent an implementation-mirroring test for the override.
- [ ] **2. Repair only evidenced compatibility failures.** For failures consisting solely of rules_rust versus rules_rs generated loads, change the expected golden load statements to match the already approved adapter, preserving rule attributes, dependency labels, and all other expected output. For runfiles or module mapping failures, repair the precise test/build boundary without changing parser semantics. Run root Gazelle immediately after source edits and before formatting. Any required semantic change exceeds this baseline and must be surfaced before implementing it.
- [ ] **3. Rerun focused coverage.** Repeat step 1 and run `nix develop --command aspect test @gazelle_rust//rust_language:gofmt_test`. Expected: all selected tests pass, including fixtures for local/external dependencies, aliases, and procedural macros. Confirm `generation_tests/BUILD.bazel` still uses `disabled_tests = []`. Check fixture diffs contain only explained compatibility adjustments.
- [ ] **4. Run repository acceptance checks.** Run root Gazelle, `nix develop --command aspect format --scope=all`, `nix develop --command aspect build //...`, `nix develop --command aspect test //...`, and `nix develop --command aspect lint`. Inspect logs even on successful exit. Keep unrelated generated drift out of the delivered diff. Report failed or blocked checks separately; do not declare stage 1 verified while required checks remain unresolved.
- [ ] **5. Recheck final generation stability.** Repeat Task 1's application comparison and whole-tree second-run inventory after all fixture/integration changes. Require unchanged application output relative to the old plugin, no vendored rewriting, and no second-run diff. Run `git diff --check`.
- [ ] **6. Finish the maintenance guide.** Add the exact passing focused commands from this task, root generation command, nested-module boundary explanation, and any additional invocation required by upstream dependencies. Document validation results and remaining blockers without claiming implementation of crate discovery.
- [ ] **7. Commit and report.** Format documentation, inspect the final diff, and commit only justified fixture/wiring/documentation changes as `test: validate local gazelle Rust baseline`. Report upstream provenance, generation equivalence, idempotence, focused test counts, repository checks, and blockers. Stop before #1957 design or implementation; it remains the next separately reviewable stage of #1955.

## Execution Handoff

This is a sequential integration with two tightly related tasks. Native execution is recommended: one implementer can retain the before/after comparison context, followed by an independent whole-branch review. Subagent-driven execution remains available if preferred. The user must review this plan and choose an execution method before implementation.
