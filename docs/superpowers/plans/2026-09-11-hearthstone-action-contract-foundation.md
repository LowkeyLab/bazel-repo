# Hearthstone Action-Contract Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make supported action declarations, deterministic legal-action enumeration, explicit targeting, and guarded continuations obey one tested contract.

**Architecture:** Store targeting as serializable card-form data. A read-only validator returns normalized actions and is shared by submission and enumeration. Extend the existing one-shot LIFO resolver with guarded sequence operations; preserve its execution, choice, and checkpoint model.

**Tech Stack:** Rust 2024, Bevy 0.19 ECS, serde, googletest, Bazel/Gazelle, Aspect.

**Spec:** [Approved design](../specs/2026-09-11-hearthstone-action-contract-foundation-design.md). Read it before executing this plan; it defines semantics and exclusions.

## Global Constraints

- This is an engine API foundation under the existing `AdvancedRulebook2026_06_26` profile.
- Only minion and spell plays are included. Unsupported hand card kinds are excluded.
- Card constructors default to None.
- All nonempty PlayCard.choice inputs are rejected as unsupported.
- Preserve current active-player-only EndTurn and Concede validation.
- Enumeration itself changes neither state nor trace.
- A rejected declaration preserves gameplay state, RNG, counters, and pending resolution work. The existing ActionRejected trace entry is permitted.
- Do not introduce a general predicate language, transaction framework, or replacement resolver.
- Keyword-specific targeting and combat legality remain explicit gaps.
- Use Bazel/Aspect for all build, test, formatting, and dependency operations; add no dependencies.
- Run `bazel run //:gazelle` immediately after each source-edit batch, including test edits, before any formatting or test command. Do not manually edit BUILD files before that run.
- Before every commit, run `aspect format --scope=all` and `aspect build //...`, plus that task's tests. Commit only task-owned files using conventional commits.
- Keep later Milestone 8 sequences and Milestone 9 policies incomplete in progress documentation.

## File and interface map

Create `core/targeting.rs` for serializable declaration filters. Extend `core/model.rs`, `core/card_definition.rs`, and `core/checkpoint.rs` for metadata. Export types in `core/lib.rs` and the simulator's explicit imports in `simulator/lib.rs`.

Create `simulator/simulation_action_validation.rs`, registered as sibling `action_validation` through `#[path = "simulation_action_validation.rs"]` in `simulation.rs`. Move the existing validation functions there and add target validation. Keep submission, candidate enumeration, action compilation, and sequence execution in `simulation_action.rs`.

Keep tests in existing `simulation_tests_actions.rs`, `simulation_tests_effects.rs`, `simulation_tests_movement.rs`, and `simulation_tests_api.rs`; their parent test module already exposes fixture helpers and internal world access. Do not split unrelated modules.

Extend `core/resolver.rs`, `core/trace.rs`, `simulation_action.rs`, `simulation_event_resolver.rs`, and `simulation_checkpoint.rs` for guarded steps.

The actual focused test targets are `//hearthstone_simulator/core:core_test` and `//hearthstone_simulator/simulator:simulator_test`. The simulator tests are not in the core target.

## Task 1: Explicit targeting data survives every card-form path

**Files:** Create `core/targeting.rs`; modify `core/{model,card_definition,checkpoint,lib}.rs`, `simulator/{lib,simulation_card_runtime,simulation_effect_executor,simulation_checkpoint}.rs`; test in core targeting/card-definition tests and `simulator/simulation_tests_api.rs`, `simulation_tests_effects.rs`, `simulation_tests_movement.rs`. Inspect `simulator/zone.rs:reset_runtime_state`; modify it only if preservation requires a change.

**Interfaces produced:**

```rust
// All four types derive Clone, Copy, Debug, Eq, PartialEq,
// serde::Deserialize, serde::Serialize.
pub enum TargetAudience { Friendly, Enemy, Either }
pub enum TargetKind { Minion, Hero, Character }
pub struct TargetFilter {
    pub audience: TargetAudience,
    pub kind: TargetKind,
}
pub enum TargetRequirement {
    None,
    Required(TargetFilter),
    Optional(TargetFilter),
    RequiredIfAvailable(TargetFilter),
}
// Field on Card, CardDefinition, CardRuntime, CardRuntimeCheckpoint:
// targeting: TargetRequirement
// Card method:
// pub fn with_targeting(self, targeting: TargetRequirement) -> Self
```

- [ ] Add a core regression exercising constructor default, builder, definition conversion, and serde. Use this concrete case in `card_definition.rs` tests, importing the new types explicitly:

```rust
let targeting = TargetRequirement::Required(TargetFilter {
    audience: TargetAudience::Enemy,
    kind: TargetKind::Character,
});
assert_eq!(Card::spell("Plain", 0).targeting, TargetRequirement::None);
let card = Card::spell("Bolt", 0).with_targeting(targeting);
let json = serde_json::to_string(&card).unwrap();
assert_eq!(serde_json::from_str::<Card>(&json).unwrap(), card);
assert_eq!(CardDefinition::from(card).targeting, targeting);
```

- [ ] Run Gazelle, then `aspect test //hearthstone_simulator/core:core_test`. Expect missing-type/field failures before implementation.
- [ ] Implement the four types, exports, field, and builder. Constructors explicitly initialize None. The builder sets the field and returns self. Do not add serde defaults that accept old checkpoint payloads as the new schema.
- [ ] Thread metadata through each reconstruction point using the source appropriate to that path:

```rust
// CardDefinition::from and spawn_validated_card:
targeting: card.targeting,
// copy_card_data:
targeting: runtime.targeting,
// transform_entity's replacement CardRuntime:
targeting: card.targeting,
// build_checkpoint:
targeting: runtime.targeting,
// restore_checkpoint:
targeting: value.targeting,
```

Backward movement currently resets stats/keywords and leaves CardRuntime intact. Preserve the current form's targeting through that path; do not introduce a definition registry lookup. Include all explicit Card and CardRuntime initializers located with `rg -n 'Card \{|CardRuntime \{|CardDefinition \{' hearthstone_simulator`.

- [ ] Bump checkpoint schema 7 to 8 for metadata and update version-rejection assertions. Add a runtime round trip using the existing API:

```rust
let simulation = Simulation::new([
    PlayerConfig::new("Jaina", vec![Card::spell("Bolt", 0).with_targeting(
        TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        }),
    )]),
    PlayerConfig::new("Rexxar", Vec::new()),
]);
let checkpoint = simulation.checkpoint().unwrap();
let json = checkpoint.to_json().unwrap();
let restored = Simulation::from_checkpoint(
    SimulationCheckpoint::from_json(&json).unwrap(),
).unwrap();
assert_eq!(restored.checkpoint().unwrap(), checkpoint);
```

- [ ] Extend existing copy/transform/movement regressions: give source and replacement distinct targeting values, inspect reconstructed `copy_card_data(...).unwrap().targeting`, and assert source metadata on copies, replacement metadata after transformation, and unchanged current-form metadata after backward movement. Exercise both Play and non-Play copy destinations already covered by those fixtures.
- [ ] Run Gazelle after edits, `aspect test //hearthstone_simulator/...`, repository formatting, and full build. Commit `feat(hearthstone): persist explicit card targeting contracts`.

## Task 2: Shared validator enforces targeting and normalizes declarations

**Files:** Create `simulator/simulation_action_validation.rs`; modify `simulation.rs`, `simulation_action.rs`, `core/error.rs`; test `simulation_tests_actions.rs`. Migrate targeted action fixtures in existing simulator test modules.

**Interfaces consumed:** Task 1 targeting types; existing `CardRuntime`, `ZoneIndex`, `player`, `game_entity`, and effect-program validators.

**Interfaces produced in action_validation:**

```rust
pub(super) fn validate_action(
    world: &World, action: &GameAction,
) -> Result<GameAction, SimulationError>;
pub(super) fn eligible_targets(
    world: &World, player: PlayerId, filter: TargetFilter,
) -> Vec<GameEntityId>;
pub(super) fn target_options(
    world: &World, player: PlayerId, requirement: TargetRequirement,
) -> Vec<Option<GameEntityId>>;
```

`validate_action` returns a cloned action with minion append normalized to an explicit index. It is the only entry point for declaration legality. `eligible_targets` returns sorted, deduplicated in-Play candidates matching audience and character kind. `target_options` implements this exact truth table, reusing `eligible_targets`:

```rust
match requirement {
    TargetRequirement::None => vec![None],
    TargetRequirement::Required(filter) => eligible_targets(world, player, filter)
        .into_iter().map(Some).collect(),
    TargetRequirement::Optional(filter) => std::iter::once(None)
        .chain(eligible_targets(world, player, filter).into_iter().map(Some)).collect(),
    TargetRequirement::RequiredIfAvailable(filter) => {
        let targets = eligible_targets(world, player, filter);
        if targets.is_empty() { vec![None] }
        else { targets.into_iter().map(Some).collect() }
    }
}
```

- [ ] Add a regression for ignored target input before moving validation code:

```rust
#[googletest::test]
fn untargeted_play_rejects_a_supplied_target() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);
    assert!(simulation.apply(GameAction::PlayCard {
        player: PlayerId::One, card, target: Some(target),
        board_index: None, choice: None,
    }).is_err());
}
```

Run Gazelle and the simulator test target. Expect this new assertion to fail under current behavior.

- [ ] Move `validate_action`, `validate_play_card`, and `validate_attack` into the sibling module, retaining existing validation checks. Add these structured errors with thiserror messages: `MissingTarget(GameEntityId)`, `UnexpectedTarget(GameEntityId)`, `InvalidTarget { card: GameEntityId, target: GameEntityId }`, `UnsupportedActionChoice(ChoiceId)`, and `UnexpectedBoardPosition(GameEntityId)`. Reuse the existing zone error for out-of-range minion positions.
- [ ] Match the requirement table before resolution. For a supplied target, reject None requirements as UnexpectedTarget and filter failures as InvalidTarget. For an absent target, reject Required and nonempty RequiredIfAvailable as MissingTarget. Reject nonempty choice and any spell board_index. Normalize minion None to `Some(crate::zone::board_entities(world, player).len())` only after validating the submitted position. Use `crate::zone::board_entities(world, player).len()` for board-row length in enumeration too; counting all Play entities would incorrectly include Heroes and Hero Powers.
- [ ] Restrict both combat entities to `EntityKind::Hero | EntityKind::Minion` in addition to existing readiness/zone/controller checks. Do not add keyword checks in this task.
- [ ] Change action submission to consume the normalized declaration before changing status:

```rust
let action = super::action_validation::validate_action(world, action)?;
// Only after this succeeds: set Resolving, begin_sequence, compile &action.
```

Captured target and normalized placement are copied directly to SequenceStep. Keep ActionAccepted/Rejected logging and post-acceptance error semantics unchanged.

- [ ] Add table-driven cases covering four requirements with zero and multiple candidates, all audiences and kinds, missing/stale/wrong-zone targets, both players' Heroes, and non-character in-Play entities. Each case asserts the specific error or normalized action. Migrate fixtures submitting targets to explicit metadata, including any fixture intentionally supplying an otherwise unused target.
- [ ] Add rejection atomicity checks around each invalid declaration. Compare checkpoints after removing only the newly appended ActionRejected entry from the actual checkpoint's trace; compare remaining fields exactly. Check wrong turn, busy, complete, unsupported kind/choice, mana, capacity, and positions.
- [ ] Run Gazelle, `aspect test //hearthstone_simulator/...`, formatting, and full build. Commit `feat(hearthstone): validate and normalize action declarations`.

## Task 3: Enumerate every canonical supported action

**Files:** Modify `simulator/simulation_action.rs`; test `simulation_tests_actions.rs` and update existing exact-list expectations in that file.

**Consumes:** `validate_action`, `target_options` from Task 2.

**Produces:** Existing `legal_actions(world: &mut World) -> Vec<GameAction>` with the approved deterministic contract; no public signature change.

- [ ] Add a regression with an empty friendly board and a targeted zero-cost minion. Both opposing Hero and a spawned enemy minion are eligible. Expected plays are the two target IDs in ascending order at position Some(0), surrounded by EndTurn and Concede. This currently fails because enumeration emits target=None and omits Concede.
- [ ] Add a candidate through the shared validator using this local closure pattern:

```rust
let mut actions = Vec::new();
let mut offer = |candidate: GameAction| {
    if let Ok(normalized) = super::action_validation::validate_action(world, &candidate) {
        actions.push(normalized);
    }
};
```

Read world state through shared references while this closure exists. Return early unless AwaitingAction and outcome absent. Offer EndTurn first, then plays, then attacks, then Concede.

- [ ] Sort and deduplicate hand IDs; ignore stale IDs lacking entity/runtime data. For each supported card, iterate `target_options`, then explicit minion positions `0..=board_len`; spells use None. Offer every candidate with choice=None. Never offer an additional minion append alias.
- [ ] Sort and deduplicate the active player's and opponent's Play IDs. Generate character attacker/defender pairs, offering each through validation. Readiness remains owned by the validator. Preserve grouping order: card ID, optional target order, position; then attacker ID, defender ID.
- [ ] Add the observational purity and soundness regression:

```rust
let before = simulation.checkpoint().unwrap();
let first = simulation.legal_actions();
assert_eq!(first, simulation.legal_actions());
assert_eq!(simulation.checkpoint().unwrap(), before);
for action in &first {
    assert_eq!(
        super::action_validation::validate_action(simulation.app.world(), action).unwrap(),
        *action,
    );
}
```

In the nested test module, access `action_validation` through the parent module's path (for example `super::action_validation`); do not make it public outside the simulator.

- [ ] Add an independently generated exhaustive small-fixture candidate loop. Iterate every hand ID including unsupported kinds; targets None, every known entity, and a stale ID; positions None and zero through board length plus one; choices None and Some(ChoiceId(999)). Normalize each successful validation and assert it is contained in the list. Separately enumerate all entity pairs for attacks and both players for EndTurn/Concede. This test must not call target_options to generate its oracle candidates.
- [ ] Execute every canonical action on a fresh fork of the same well-formed fixture. Add separate fixtures for exhausted/zero-attack characters, full board, unsupported hand cards, negative costs, and each status restriction. Assert minion None normalizes to exactly the final explicit position action.
- [ ] Run Gazelle, simulator tests, formatting, and full build. Commit `feat(hearthstone): enumerate canonical legal actions`.

## Task 4: Guarded one-shot sequence operations

**Files:** Modify `core/{resolver,trace,lib,checkpoint}.rs`, `simulator/{lib,simulation_action,simulation_event_resolver,simulation_checkpoint}.rs`; test `simulation_tests_actions.rs` and core resolver serialization tests.

**Consumes:** Existing SequenceStep dispatch and normalized action compiler.

**Produces:**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SubjectGuard {
    pub subject: GameEntityId,
    pub required_zone: Zone,
}
// ResolutionOp addition:
// RunGuardedSequenceStep { guards: Vec<SubjectGuard>, step: SequenceStep }
// TraceEntry addition:
// SequenceStepSkipped {
//     step: SequenceStep,
//     subject: GameEntityId,
//     expected_zone: Zone,
//     actual_zone: Option<Zone>,
// }
// simulation_action.rs:
// pub(super) fn run_guarded_sequence_step(
//     world: &mut World, guards: &[SubjectGuard], step: &SequenceStep,
// ) -> Result<(), SimulationError>
```

The trace carries a typed step, avoiding a second string-based sequence naming contract. Missing entity or missing zone records actual_zone=None.

- [ ] Add a test that queues a guarded Concede step requiring a hand card to be in Play. Queue an unguarded EndTurn after it, drive resolution, and assert no concede outcome, one skip trace, and that EndTurn still runs. This synthetic use proves guard behavior independently of full card-sequence rules.
- [ ] Run Gazelle and simulator tests; expect the new type/variant to be missing. Then implement the first-failing-guard algorithm:

```rust
for guard in guards {
    let actual_zone = game_entity(world, guard.subject)
        .and_then(|entity| world.get::<Zone>(entity).copied());
    if actual_zone != Some(guard.required_zone) {
        world.resource_mut::<CanonicalTrace>().entries.push(
            TraceEntry::SequenceStepSkipped {
                step: step.clone(), subject: guard.subject,
                expected_zone: guard.required_zone, actual_zone,
            },
        );
        return Ok(());
    }
}
run_sequence_step(world, step)
```

- [ ] Add dispatch and `ResolutionOp::kind()` handling. The guarded operation executes or skips the step directly; it never invokes the resolution driver or leaves a partially consumed operation. Count the popped guard operation once under the existing budget mechanism.
- [ ] Compile accepted PlayCard as guarded with card/Hand, and Attack with attacker/Play then defender/Play. Keep EndTurn and Concede unguarded. Keep the ordinary boundary and CheckOutcome outside the guarded operation. Do not wrap FinishAttack, effect programs, or played-self-transform completion.
- [ ] Add checkpoint-reference validation for each guard subject and the nested SequenceStep using existing validators. Existing subjects in wrong zones are valid restoration input. Unknown logical IDs remain invalid checkpoint references even though runtime guard dispatch handles missing entities defensively.
- [ ] Bump schema 8 to 9 because this independently committed task adds another serialized operation contract. Update schema tests. Both schema changes are intentional: the final implementation accepts version 9 and rejects intermediate versions 7/8.
- [ ] Add success, empty-guards, missing subject, wrong zone, first-of-two failure, and no-RNG-change cases. Use a queued Move effect before the guarded step to prove execution-time evaluation. Exercise move-out-and-back and stable-ID transformation passing the guard. Assert siblings and CheckOutcome still run, with no orphaned resolver work.
- [ ] Run Gazelle, Hearthstone tests, formatting, and full build. Commit `feat(hearthstone): guard deferred sequence subjects`.

## Task 5: Captured targets and suspended guards survive continuation

**Files:** Test `simulator/simulation_tests_actions.rs`, `simulation_tests_api.rs`; modify `simulation_checkpoint.rs` only for defects revealed by these integration cases.

**Consumes:** Task 4 guarded operations and Task 2 captured declarations; existing `retain_operation` helper in API tests, ChoiceRequest, ChoiceOption, and Simulation checkpoint/fork API.

**Produces:** Behavioral evidence for target capture and checkpoint-exact guarded continuation.

- [ ] Build a required-enemy-character spell targeting a spawned enemy minion, with a CardPlayed trigger that changes that minion's controller before the spell's damage effect executes. The action is accepted based on initial enemy ownership; damage still refers to the same captured ID. Use `Effect::Move { targets: Selector::Entity(target), player: PlayerSelector::Player(PlayerId::One), zone: Zone::Play, kind: ZoneMovementKind::Normal }` to change the minion's control while remaining in Play. Give both players free board capacity; do not change Hero ownership. Assert damage hits that ID and no retarget event/RNG draw occurs.
- [ ] Add a complementary invalid-before-declaration case using the same fixture: change control before apply, then expect InvalidTarget with unchanged gameplay state. Together these distinguish declaration validation from later effect resolution.
- [ ] Construct suspended work using the existing API-test `retain_operation` helper. Put a guarded Concede requiring source/Play below RequestChoice. Give two choice options: no operations, or a Move operation sending source to Hand. Include a following ordinary boundary and CheckOutcome outside the guard. Use real allocated IDs for the subject and the existing ChoiceId allocator/counters in the fixture.
- [ ] Round-trip the suspended checkpoint and fork it using this comparison pattern, once for each option:

```rust
let checkpoint = original.checkpoint().unwrap();
let mut restored = Simulation::from_checkpoint(
    SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
).unwrap();
let mut fork = original.fork().unwrap();
original.choose(option).unwrap();
restored.choose(option).unwrap();
fork.choose(option).unwrap();
assert_eq!(original.snapshot(), restored.snapshot());
assert_eq!(original.snapshot(), fork.snapshot());
assert_eq!(original.trace(), restored.trace());
assert_eq!(original.trace(), fork.trace());
original.assert_invariants().unwrap();
restored.assert_invariants().unwrap();
fork.assert_invariants().unwrap();
```

`option` is the ChoiceId from the chosen option in that fixture; use a fresh original simulation for each branch.

- [ ] Assert successful guard execution for the unchanged-zone branch and exactly one skip for the moved branch. Assert empty pending choice/stack/event slots at completion. Add checkpoint tampering tests: unknown guard ID fails restoration; a known wrong-zone subject restores and subsequently skips; old schema versions fail.
- [ ] Run Gazelle after edits, Hearthstone tests, formatting, and full build. Commit `test(hearthstone): verify action continuation contracts`.

## Task 6: Publish foundation coverage and verify the implementation

**Files:** Modify `hearthstone_simulator/{README,IMPLEMENTATION_PROGRESS,RULEBOOK_CONFORMANCE}.md`; update this plan's completed checkboxes only for verified tasks.

**Consumes:** Passing tasks 1–5 and actual fresh verification results.

**Produces:** Documentation accurately describing the foundation and remaining scope.

- [ ] Update README's action API description with explicit targeting, canonical placement enumeration, attacks, and supported declaration restrictions. Use this example, adding the public core import matching the documentation's surrounding style:

```rust
let bolt = Card::spell("Bolt", 1)
    .with_targeting(TargetRequirement::Required(TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    }))
    .with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(2),
    }]);
```

- [ ] Add checked Milestone 8 subitems for explicit action contracts, canonical enumeration, and narrow serializable subject guards. Leave weapons, Hero cards, locations, Hero Powers, combat redirection, full phase guards, and keyword-specific legality unchecked. Do not relabel the whole milestone complete.
- [ ] Add conformance rows classified Engine policy for enumeration/normalization and guarded checkpoint mechanics. Describe targeting as the implemented filter foundation; retain explicit gaps for complete rulebook legality. Document schema 9 and rejection of older checkpoints.
- [ ] Run `aspect format --scope=all`, `aspect test //hearthstone_simulator/...`, `aspect lint`, and `aspect build //...`. Inspect exit codes and record actual results in the progress verification log. No Gazelle run is needed for Markdown-only edits.
- [ ] Run `git diff --check` and inspect the full diff for unrelated changes, broad exports, silent defaults, accidental target revalidation, duplicated legality checks, and changes to later-milestone semantics.
- [ ] Commit `docs(hearthstone): record action-contract foundation coverage`. Report completed behavior, verification evidence, final checkpoint schema, and remaining Milestone 8 scope.

## Plan self-review coverage

| Spec requirement                                                               | Task    |
| ------------------------------------------------------------------------------ | ------- |
| Four targeting modes and card-form propagation                                 | 1, 2    |
| Read-only validation, structured errors, positions, choices, character attacks | 2       |
| Rejection atomicity and normalized captured declarations                       | 2, 5    |
| Complete canonical enumeration, deterministic order, no mutation               | 3       |
| Narrow one-shot guards and first-failure diagnostics                           | 4       |
| Zone continuity semantics and unaffected cleanup                               | 4       |
| Runtime metadata and guarded checkpoint validation/versioning                  | 1, 4, 5 |
| Captured targets, suspended choices, JSON/fork equivalence                     | 5       |
| Honest partial-milestone documentation and required verification               | 6       |

## Execution handoff

This document is the implementation plan, not evidence that code or tests have changed. Choose inline execution with `superpowers:executing-plans`, or explicitly select subagent-driven execution with task reviews. Establish an isolated worktree at execution time if the current checkout needs isolation. Execute tasks in order; they share contracts and are not independent parallel work.
