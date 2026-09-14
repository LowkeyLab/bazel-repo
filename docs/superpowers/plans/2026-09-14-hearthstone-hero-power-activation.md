# Hero Power Activation Implementation Checklist

Design: [Hero Power Activation](../specs/2026-09-14-hearthstone-hero-power-activation-design.md).

Status: implemented and verified on 2026-09-14. Pinned-rulebook conformance remains unverified; the design records the selected engine policy. Tests pass (9 core, 258 simulator), full build passes (351 targets), and formatting passes. Lint reports no filtered findings; unrelated website ESLint formatter errors for missing `chalk` limit the repository-wide check.

## 1. Settle sequence semantics

- [x] Verify phase-boundary placement and replacement-during-activation behavior against the pinned rulebook or record an explicitly named, unverified engine policy.
- [x] Specify exactly which original-power state completion updates after replacement.
- [x] Confirm existing trigger seed capture evaluates the required sequence-start eligibility, then retains ordinary later eligibility checks.
- [x] Express those decisions as focused ordering and replacement fixtures alongside the resolver path.

## 2. Extend the action contract

Files: `core/action.rs`, `core/error.rs`, `simulator/simulation_action_validation.rs`, `simulator/simulation_action.rs`.

- [x] Add the stable-ID `UseHeroPower` declaration and structured activation errors.
- [x] Reuse status, ownership, zone, targeting, program, and resource validation.
- [x] Add deterministic target enumeration after attacks and before concession.
- [x] Test atomic rejection, all targeting requirement branches, canonical completeness, and enumeration purity.

## 3. Add durable activation and completion steps

Files: `core/resolver.rs`, `core/event.rs`, `simulator/simulation_action.rs`. Reuse the existing event preparation API in `simulator/simulation_event_resolver.rs` unchanged.

- [x] Capture original source, target, effect program, and after-use trigger seeds at the selected timing.
- [x] Pay resources, run effects with Hero Power origin, complete exhaustion/usage, and resolve after-use events in the documented engine-policy order.
- [x] Use the existing operation stack, prepared events, boundaries, and outcome checks.
- [x] Test targeted damage modifiers, untargeted resource ordering, repeated use, turn refresh, replacement, after-use eligibility, and lethal outcomes.

## 4. Preserve activation through checkpoints

Files: `core/checkpoint.rs`, `simulator/simulation_checkpoint.rs`, focused simulator API tests.

- [x] Bump schema to 10 and update old-version rejection tests.
- [x] Validate new step references and captured seeds without rejecting legitimate moved subjects.
- [x] Test suspended activation through JSON restore and fork, including replacement and a moved declared target.
- [x] Compare complete final checkpoints and canonical traces; test invalid logical references.

## 5. Document and verify

- [x] Update README with a public activation example using a synthetic power installed by Hero replacement.
- [x] Update IMPLEMENTATION_PROGRESS and RULEBOOK_CONFORMANCE with implemented behavior and retained gaps; leave Milestone 8 incomplete.
- [x] Run `bazel run //:gazelle` immediately after every source-edit batch, before formatting or manual BUILD changes.
- [x] Run `aspect format --scope=all`.
- [x] Run `aspect test //hearthstone_simulator/...`.
- [x] Run `aspect build //...`.
- [x] Run `aspect lint`.
- [x] Record actual verification results; do not turn planned coverage into a completion claim.

Keep tests in the existing action, Hero, and API suites unless a dedicated Hero Power suite materially improves readability. No dependency additions or unrelated refactoring are expected.
