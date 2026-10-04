# Locally maintained Gazelle Rust baseline

Date: 2026-10-04
Status: Design approved in chat; written specification awaiting review.
Tracking: [#1956](https://github.com/LowkeyLab/bazel-repo/issues/1956), stage 1 of [#1955](https://github.com/LowkeyLab/bazel-repo/issues/1955).

## Intent and success criteria

Maintain gazelle_rust inside this repository so the subsequent stage can replace its generation modes with one Cargo-style crate discovery pipeline. This stage establishes a reproducible local upstream baseline with unchanged application BUILD generation. Bazel remains the build system.

The user approved the stage-1 design in chat. This specification covers that stage only. Unified discovery, parser protocol extensions, source ownership, target migration, and removal of rust_mode belong to #1957 and require a separate design after this baseline is established. Do not publish or merge either stage without a separate instruction.

## Verified starting point

At inspection, the checkout was clean at 85f2954a. MODULE.bazel uses a git_override for gazelle_rust at upstream commit 747482bfad721b0fe7198c94491d76c32b263ac0 from https://github.com/Calsign/gazelle_rust.git. The root gazelle_bin includes @gazelle_rust//rust_language.

The existing 3rdparty/gazelle_rust/rules_rs_loads.patch changes generated Rust and cargo_build_script loads to rules_rs, retaining rules_rust loads for the other supported rule families, and sets the parser library edition to 2024. Preserve both adaptations. Recheck these inputs before implementation and preserve intervening changes.

Existing root Gazelle exclusions protect several application directories. Preserve those exclusions. No application target migration is authorized by this stage.

## Approach and alternatives

Copy the pinned upstream tree into 3rdparty/gazelle_rust/upstream as a nested Bazel module and replace the remote override with local_path_override. This retains upstream package boundaries and makes later changes directly reviewable.

A remote patch stack would keep maintenance dependent on patch application rather than establish the requested local source. Flattening upstream packages into the main module would unnecessarily change labels and imports. Neither alternative meets the intended maintenance boundary as directly.

## Source and integration boundaries

Retain upstream source, licence, module metadata, test fixtures, and files required by parser, protobuf, and generation builds. Do not copy Git metadata or generated build caches. Record the upstream URL, exact commit, and local adaptations in a provenance document next to the vendored module.

Apply the existing compatibility patch directly to the copied source. Remove the obsolete remote patch configuration and patch artifact once its complete contents are accounted for in the local source and provenance. Preserve the parent BUILD package if documentation or other retained files still require it.

Keep Go import paths, internal package labels, protobuf generation, parser service packaging, and parser runfiles intact. Continue referencing @gazelle_rust//rust_language from the root Gazelle binary. Retain the plugin's internal build toolchain choices; changing generated application loads to rules_rs does not authorize a wholesale conversion of upstream internals.

Add the nested source directory to root Gazelle exclusions and .bazelignore. Root generation must not rewrite the maintained plugin or its golden fixtures, and root recursive target patterns must not treat nested-module packages as main-repository packages. Build and test the plugin explicitly through its external module labels. Document the explicit targets and any additional invocation needed for tests that rely on upstream development dependencies.

## Behavior and failure handling

Application generation behavior remains identical to the currently patched upstream baseline. Do not change discovery modes, naming, module traversal, dependency resolution, source ownership, or existing application exclusions.

If local-module integration exposes missing dependency mappings, protobuf inputs, or runfiles, fix the integration boundary while preserving parser and generator semantics. Keep parser and applicable generation tests runnable through Bazel. If fixture expectations conflict with the existing rules_rs load adaptation, update only those expected load statements and explain the difference; do not rewrite fixtures broadly or disable failing behavior coverage.

If a build or test is blocked by infrastructure or an upstream incompatibility, record the command, failure, and affected acceptance criteria. Do not label blocked checks as passed or proceed to crate-discovery work with an unverified baseline.

## Baseline comparison

Before switching the override, run the existing patched generator and record the resulting application BUILD state, including tracked changes and newly generated files. Treat pre-existing generation drift separately from the local-module change; do not silently adopt unrelated drift.

Run the locally maintained generator against the same application inputs and compare the generated application BUILD files with that recorded baseline. Require matching content and file membership. Account separately for intentional root integration edits and the vendored module itself.

Run Gazelle a second time and require no additional changes. Verify the vendored tree and its fixtures remain unchanged by ordinary root generation. These comparisons must cover the repository's current generation scope without relaxing exclusions to manufacture coverage.

## Validation and delivery

Use the repository's Bazel/Aspect workflow, entering the Nix development environment when required. Run Gazelle immediately after source edits and before formatting. Determine exact upstream test labels from the copied BUILD files rather than assume target names.

Required evidence for stage 1:

- Root Gazelle binary builds and runs using the local module.
- Parser and applicable generator/resolver fixture tests pass through Bazel, with existing coverage retained.
- Before/after application generation comparison passes.
- A second generation run introduces no changes and does not modify the maintained plugin.
- aspect format --scope=all, aspect build //..., aspect test //..., and aspect lint run; failures and environmental blockers are reported accurately.
- git diff --check passes and the final diff contains only justified stage-1 changes.

Document provenance, local module boundaries, the root generation command, focused build/test commands, and how to maintain the plugin without root Gazelle rewriting it. Deliver stage 1 as its own reviewable implementation commit or commits before stage 2 changes. No push, pull request, or merge is part of this authorization.

## Review and handoff

Self-review this specification for missing requirements, contradictions, unclear scope, and unresolved placeholders. The user reviews the saved specification next. Only after written-spec approval should writing-plans create the implementation plan; the user then reviews that plan and selects its execution method before implementation begins.
