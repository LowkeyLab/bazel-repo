# Trigger Enchantments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class permanent and timed trigger enchantments with correct Play-zone identity, attachment context, ordering, expiration, removal, and checkpoint behavior.

**Architecture:** Every enchantment is a durable Play-zone ECS entity with an explicit `EnchantmentDuration`; removed enchantments retain their payload and duration in `RemovedFromGame`. Trigger enchantments reuse `RuntimeTriggers` and the existing immutable candidate pipeline, with attachment-aware selectors and conditions rather than a parallel trigger registry.

**Tech Stack:** Rust 2024, Bevy 0.19 ECS, serde, thiserror, googletest, Bazel/Aspect.

**Spec:** `docs/superpowers/specs/2026-09-03-trigger-enchantments-design.md`

## Global Constraints

- Ruleset remains `AdvancedRulebook2026_06_26`, pinned to advanced rulebook revision `913067` dated `2026-06-26`.
- Every `EntityKind::Enchantment` has an explicit `EnchantmentDuration`; missing duration never means permanent.
- Every attached enchantment is in `Zone::Play`; enchantments do not consume the seven-slot board row.
- The enchantment is the trigger source and owns controller/order semantics; `AttachedEntity` explicitly refers to its host.
- Added `EventKind::Death` trigger enchantments remain deferred to Milestone 9.
- No backward migration for checkpoint schema version 5; version 6 rejects older checkpoints.
- Do not add a trigger-grant registry, executable queue, cursor, or recursive resolver path.
- Use Bazel/Aspect only. Run `bazel run //:gazelle` immediately after every Rust source edit and before formatting.
- Finish with `aspect format --scope=all`, simulator tests, coverage, lint, and `aspect build //...`.

## File Map

- `hearthstone_simulator/core/enchantment.rs`: define `EnchantmentDuration`.
- `hearthstone_simulator/core/effect.rs`: normalize attachment effect payloads and add `AttachTriggerEnchantment` plus `Selector::AttachedEntity`.
- `hearthstone_simulator/core/trigger.rs`: add relative event-controller and attached-target conditions.
- `hearthstone_simulator/core/checkpoint.rs`: persist the renamed duration field and schema version 6.
- `hearthstone_simulator/core/error.rs`: report invalid trigger-enchantment definitions.
- `hearthstone_simulator/core/lib.rs`: export new and renamed public types.
- `hearthstone_simulator/simulator/enchantment.rs`: validate runtime enchantment invariants.
- `hearthstone_simulator/simulator/simulation_effect_executor.rs`: create all enchantments through one base helper, execute trigger attachment, validate payloads, and resolve attachment selectors.
- `hearthstone_simulator/simulator/trigger.rs`: evaluate attachment-aware and relative event-controller conditions.
- `hearthstone_simulator/simulator/simulation_action.rs`: expire only active timed enchantments while preserving `Permanent`.
- `hearthstone_simulator/simulator/simulation_checkpoint.rs`: serialize, restore, and validate version 6 duration/zone invariants.
- `hearthstone_simulator/simulator/simulation.rs`: include enchantment invariants in the public invariant check.
- `hearthstone_simulator/simulator/zone.rs`: preserve duration when detaching and keep board capacity independent from enchantment Play membership.
- `hearthstone_simulator/simulator/lib.rs`: update internal re-exports.
- `hearthstone_simulator/simulator/simulation_tests_temporal.rs`: duration, turn-series, and trigger-expiration conformance.
- `hearthstone_simulator/simulator/simulation_tests_events.rs`: trigger source, host context, controller grouping, ordering, and abort behavior.
- `hearthstone_simulator/simulator/simulation_tests_api.rs`: checkpoint schema and invariant coverage.
- `hearthstone_simulator/simulator/simulation_tests_movement.rs`: Play-zone enchantment and detachment coverage.
- `hearthstone_simulator/simulator/simulation_tests_effects.rs`, `simulation_tests_auras.rs`, and `simulation_tests_hero.rs`: migrate existing attachment fixtures to explicit duration.
- `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`, `RULEBOOK_CONFORMANCE.md`, and `README.md`: publish completed behavior and retained gaps.

---

### Task 1: Normalize Enchantment Duration And Zone Semantics

**Files:**

- Modify: `hearthstone_simulator/core/enchantment.rs`
- Modify: `hearthstone_simulator/core/effect.rs`
- Modify: `hearthstone_simulator/core/checkpoint.rs`
- Modify: `hearthstone_simulator/core/lib.rs`
- Modify: `hearthstone_simulator/simulator/lib.rs`
- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_action.rs`
- Modify: `hearthstone_simulator/simulator/simulation_checkpoint.rs`
- Modify: `hearthstone_simulator/simulator/zone.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_temporal.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_movement.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_effects.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_auras.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_hero.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_api.rs`

**Interfaces:**

- Consumes: Existing `AttachedTo`, modifier components, `ZoneIndex`, and turn scheduling.
- Produces: `EnchantmentDuration`; duration-bearing attachment effect variants; `spawn_attached_enchantment(...) -> Result<(GameEntityId, Entity), SimulationError>` for Task 3.

- [ ] **Step 1: Write failing tests for explicit permanence and Play-zone membership**

Add a temporal test that creates a permanent stat enchantment, ends a full turn, and verifies both the modifier and duration remain:

```rust
fn play_card(
    simulation: &mut Simulation,
    player: PlayerId,
    card: GameEntityId,
    target: Option<GameEntityId>,
) {
    simulation.apply(GameAction::PlayCard {
        player,
        card,
        target,
        board_index: None,
        choice: None,
    }).unwrap();
}

#[googletest::test]
fn permanent_enchantment_has_explicit_duration_and_survives_turn_cleanup() {
    let buff = Card::spell("Permanent Strength", 0).with_effects(vec![
        Effect::AttachStatModifier {
            targets: Selector::DeclaredTarget,
            modifier: StatModifier {
                attack: 2,
                health: 0,
                silence_removable: true,
            },
            duration: EnchantmentDuration::Permanent,
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Target", 0, 1, 2), buff]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let target = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, target, None);
    let buff = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, buff, Some(target));

    simulation.apply(GameAction::EndTurn { player: PlayerId::One }).unwrap();

    let enchantment = simulation.app.world().iter_entities()
        .find(|entity| entity.get::<StatModifier>().is_some())
        .unwrap();
    assert_that!(enchantment.get::<Zone>(), eq(Some(&Zone::Play)));
    assert_that!(
        enchantment.get::<EnchantmentDuration>(),
        eq(Some(&EnchantmentDuration::Permanent)),
    );
    assert_that!(object(&mut simulation, target).attack, eq(Some(3)));
}
```

Add a movement test that fills all seven board slots, attaches a permanent keyword enchantment, and distinguishes board-row capacity from total Play-zone membership:

```rust
#[googletest::test]
fn play_zone_enchantments_do_not_consume_board_capacity() {
    let cards = (0..7)
        .map(|index| Card::minion(format!("Minion {index}"), 0, 1, 1))
        .collect::<Vec<_>>();
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", cards),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..7 {
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }).unwrap();
    }
    let target = simulation.snapshot().players[0].board[0];
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachKeywordModifier {
            targets: Selector::Entity(target),
            modifier: KeywordModifier {
                keyword: Keyword::Taunt,
                granted: true,
                silence_removable: true,
            },
            duration: EnchantmentDuration::Permanent,
        },
    ).unwrap();

    assert_that!(simulation.snapshot().players[0].board.len(), eq(7));
    assert_that!(
        simulation.snapshot().objects.iter().filter(|object| {
            object.controller == PlayerId::One && object.zone == Zone::Play
        }).count(),
        gt(7),
    );
}
```

- [ ] **Step 2: Regenerate BUILD metadata and verify the tests fail**

Run:

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: compilation fails because `EnchantmentDuration` and required duration fields do not exist yet.

- [ ] **Step 3: Replace optional temporary duration with the explicit enum**

In `core/enchantment.rs`, replace `TemporaryDuration` exactly with:

```rust
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum EnchantmentDuration {
    Permanent,
    EndOfTurn(PlayerId),
    EndOfTurnSeries(PlayerId),
}
```

In `core/effect.rs`, make duration mandatory and collapse the duplicate stat variants:

```rust
AttachStatModifier {
    targets: Selector,
    modifier: StatModifier,
    duration: EnchantmentDuration,
},
AttachKeywordModifier {
    targets: Selector,
    modifier: KeywordModifier,
    duration: EnchantmentDuration,
},
AttachCostModifier {
    targets: Selector,
    modifier: CostModifier,
    duration: EnchantmentDuration,
},
AttachContinuousEffect {
    targets: Selector,
    effect: ContinuousEffectDefinition,
    silence_removable: bool,
    duration: EnchantmentDuration,
},
```

Remove `AttachTemporaryStatModifier`. Export `EnchantmentDuration` from both crate facades.

- [ ] **Step 4: Regenerate immediately, then centralize base enchantment creation**

Run `bazel run //:gazelle`, then add this private helper in `simulation_effect_executor.rs`:

```rust
fn spawn_attached_enchantment(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    definition_id: &str,
    display_name: &str,
    duration: EnchantmentDuration,
    silence_removable: bool,
) -> Result<(GameEntityId, Entity), SimulationError>
```

The helper must resolve the target before allocating IDs, spawn `DefinitionId`, `EntityKind::Enchantment`, `Controller`, `DisplayName`, `PlayOrder`, `duration`, and `AttachedTo`, conditionally insert `SilenceRemovable`, then call:

```rust
insert_into_zone(world, id, controller, Zone::Play, None)
    .expect("an attached enchantment must fit in the unbounded Play zone");
```

Refactor `attach_stat_modifier`, `attach_keyword_modifier`, `attach_cost_modifier`, and `attach_continuous_effect` to use the helper and insert only their payload-specific component afterward.

- [ ] **Step 5: Regenerate immediately, then migrate all existing effect construction**

Run `bazel run //:gazelle`. Apply these exact call-site rewrites across simulator source and tests:

```rust
Effect::AttachTemporaryStatModifier { ..., duration: old }
// becomes
Effect::AttachStatModifier { ..., duration: old }

duration: None
// becomes
duration: EnchantmentDuration::Permanent

duration: Some(TemporaryDuration::EndOfTurn(player))
// becomes
duration: EnchantmentDuration::EndOfTurn(player)

duration: Some(TemporaryDuration::EndOfTurnSeries(player))
// becomes
duration: EnchantmentDuration::EndOfTurnSeries(player)
```

Add `duration: EnchantmentDuration::Permanent` to every existing `AttachStatModifier` and `AttachContinuousEffect`. Rename imports and direct component lookups from `TemporaryDuration` to `EnchantmentDuration`.

To keep this task compiling before the schema rename in Task 2, change the existing checkpoint field's type to `Option<EnchantmentDuration>` but retain its current `temporary_duration` field name. Update checkpoint construction/restoration to use `EnchantmentDuration`; Task 2 performs the persisted field rename and schema increment as one atomic change.

- [ ] **Step 6: Regenerate immediately, then update expiration and detachment**

Run `bazel run //:gazelle`. In `expire_temporary_effects`, select only entities that have `AttachedTo` and `Zone::Play`, and match:

```rust
let expires = match duration {
    EnchantmentDuration::Permanent => false,
    EnchantmentDuration::EndOfTurn(player) => player == ending_player,
    EnchantmentDuration::EndOfTurnSeries(player) => {
        player == ending_player && next_player != ending_player
    }
};
```

In `zone::apply_movement_state_policy`, delete the `DetachEnchantment` branch that removes the duration component. Update movement assertions from `Zone::SetAside` to `Zone::Play` for attached enchantments and retain `Zone::RemovedFromGame` after detachment.

Extend `backward_movement_resets_runtime_tags_and_detaches_enchantments` with the durable-duration assertion:

```rust
let detached_entity = game_entity(simulation.app.world(), detached.id).unwrap();
assert_that!(
    simulation.app.world().get::<EnchantmentDuration>(detached_entity),
    eq(Some(&EnchantmentDuration::Permanent)),
);
```

- [ ] **Step 7: Regenerate and run focused tests**

Run:

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: all simulator tests pass with explicit durations and Play-zone enchantments.

- [ ] **Step 8: Commit the normalized model**

```bash
git add hearthstone_simulator/core hearthstone_simulator/simulator
git commit -m "refactor(hearthstone): make enchantment duration explicit" \
  -m "Represent permanent and timed enchantments uniformly and keep attached enchantments in Play without consuming board capacity." \
  -m "Constraint: Every enchantment carries EnchantmentDuration" \
  -m "Confidence: high" \
  -m "Scope-risk: moderate"
```

---

### Task 2: Version And Enforce Enchantment Persistence Invariants

**Files:**

- Modify: `hearthstone_simulator/core/checkpoint.rs`
- Modify: `hearthstone_simulator/simulator/enchantment.rs`
- Modify: `hearthstone_simulator/simulator/simulation_checkpoint.rs`
- Modify: `hearthstone_simulator/simulator/simulation.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_api.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_movement.rs`

**Interfaces:**

- Consumes: `EnchantmentDuration` and Play-zone creation from Task 1.
- Produces: Checkpoint schema version 6 and `assert_enchantment_invariants(&World) -> Result<(), String>` used by normal and restored simulations.

- [ ] **Step 1: Write failing checkpoint and runtime invariant tests**

Extend the checkpoint round-trip fixture to assert `enchantment_duration == Some(EnchantmentDuration::Permanent)` for permanent payloads and the exact timed variant for temporary payloads. Add malformed-checkpoint cases:

```rust
let mut missing_duration = simulation.checkpoint().unwrap();
missing_duration.entities
    .iter_mut()
    .find(|entity| entity.kind == Some(EntityKind::Enchantment))
    .unwrap()
    .enchantment_duration = None;
assert_that!(
    Simulation::from_checkpoint(missing_duration),
    err(matches_pattern!(SimulationError::Checkpoint(contains_substring(
        "enchantment duration"
    )))),
);

let mut old_schema = simulation.checkpoint().unwrap();
old_schema.schema_version = 5;
assert_that!(
    Simulation::from_checkpoint(old_schema),
    err(matches_pattern!(SimulationError::Checkpoint(contains_substring(
        "unsupported checkpoint schema version 5"
    )))),
);
```

Add a runtime test that removes `EnchantmentDuration` from an enchantment and expects `simulation.assert_invariants()` to fail with an enchantment-duration message.

- [ ] **Step 2: Regenerate and verify failure**

Run:

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: compilation fails because the checkpoint field has not been renamed and runtime enchantment invariants are absent.

- [ ] **Step 3: Implement schema version 6 serialization and validation**

In `core/checkpoint.rs`, set `CHECKPOINT_SCHEMA_VERSION` to 6 and replace the `temporary_duration` field:

```rust
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 6;
pub enchantment_duration: Option<EnchantmentDuration>,
```

Update checkpoint construction and restoration to read/write `EnchantmentDuration`. In `validate_checkpoint_entity`, enforce:

```rust
if entity.kind == Some(EntityKind::Enchantment) && entity.enchantment_duration.is_none() {
    return Err(SimulationError::Checkpoint(format!(
        "enchantment {:?} lacks enchantment duration",
        entity.id
    )));
}
if entity.kind == Some(EntityKind::Enchantment)
    && entity.attached_to.is_some()
    && entity.zone != Some(Zone::Play)
{
    return Err(SimulationError::Checkpoint(format!(
        "attached enchantment {:?} is not in Play",
        entity.id
    )));
}
```

Do not deserialize version 5 by defaulting a missing field.

- [ ] **Step 4: Regenerate immediately, then add runtime invariants**

Run `bazel run //:gazelle`. In `simulator/enchantment.rs`, add:

```rust
pub(crate) fn assert_enchantment_invariants(world: &World) -> Result<(), String> {
    for entity in world.iter_entities().filter(|entity| {
        entity.get::<EntityKind>() == Some(&EntityKind::Enchantment)
    }) {
        let id = entity
            .get::<GameEntityId>()
            .copied()
            .ok_or_else(|| "enchantment lacks a logical ID".to_string())?;
        if entity.get::<EnchantmentDuration>().is_none() {
            return Err(format!("enchantment {id:?} lacks enchantment duration"));
        }
        if entity.get::<AttachedTo>().is_some() && entity.get::<Zone>() != Some(&Zone::Play) {
            return Err(format!("attached enchantment {id:?} is not in Play"));
        }
    }
    Ok(())
}
```

Call it from `Simulation::assert_invariants` and after checkpoint relationships are restored, before returning a restored simulation.

- [ ] **Step 5: Regenerate and run focused tests**

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: all checkpoint, movement, and simulator tests pass under schema version 6.

- [ ] **Step 6: Commit persistence invariants**

```bash
git add hearthstone_simulator/core/checkpoint.rs \
  hearthstone_simulator/simulator/enchantment.rs \
  hearthstone_simulator/simulator/simulation_checkpoint.rs \
  hearthstone_simulator/simulator/simulation.rs \
  hearthstone_simulator/simulator/simulation_tests_api.rs \
  hearthstone_simulator/simulator/simulation_tests_movement.rs
git commit -m "feat(hearthstone): persist enchantment duration invariants" \
  -m "Version checkpoints and reject enchantments without explicit duration or valid active zone state." \
  -m "Constraint: Version 5 checkpoints are intentionally unsupported" \
  -m "Confidence: high" \
  -m "Scope-risk: narrow"
```

---

### Task 3: Add The Trigger-Enchantment Primitive

**Files:**

- Modify: `hearthstone_simulator/core/effect.rs`
- Modify: `hearthstone_simulator/core/trigger.rs`
- Modify: `hearthstone_simulator/core/error.rs`
- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/trigger.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_effects.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`

**Interfaces:**

- Consumes: `spawn_attached_enchantment` and `EnchantmentDuration` from Task 1; checkpointed `RuntimeTriggers` from Task 2.
- Produces: `Effect::AttachTriggerEnchantment`, `Selector::AttachedEntity`, `TriggerCondition::EventTargetsAttachedEntity`, and `TriggerCondition::EventControllerIs(PlayerSelector)`.

- [ ] **Step 1: Write failing tests for attachment context and validation**

Add a low-level effect test that attaches this payload to a minion:

```rust
let trigger = TriggerDefinition {
    event: EventKind::TurnEnded,
    eligible_zones: vec![Zone::Play],
    conditions: vec![TimedCondition {
        timing: ConditionTiming::QueueTime,
        condition: TriggerCondition::EventControllerIs(PlayerSelector::Controller),
    }],
    source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
    priority: 0,
    wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
    effect_program: vec![Effect::DealDamage {
        targets: Selector::AttachedEntity,
        amount: ValueExpression::Constant(1),
    }],
};
let effect = Effect::AttachTriggerEnchantment {
    targets: Selector::Entity(target),
    triggers: vec![trigger.clone()],
    duration: EnchantmentDuration::Permanent,
    silence_removable: true,
};
```

Assert that execution creates one Play-zone enchantment with `AttachedTo(target)`, `RuntimeTriggers(vec![trigger])`, the context controller, explicit permanent duration, and `SilenceRemovable`.

Add table-driven action-validation cases for empty triggers, `EventKind::Death`, eligible zones other than exactly `[Zone::Play]`, `RememberedSource`, and an invalid nested native effect. For every case, assert `SimulationError::InvalidTriggerEnchantment(_)` or the existing native-effect error and assert that no trigger enchantment was created.

- [ ] **Step 2: Regenerate and verify failure**

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: compilation fails because the new effect, selector, and conditions are undefined.

- [ ] **Step 3: Add serializable core vocabulary and error reporting**

Add these variants:

```rust
Effect::AttachTriggerEnchantment {
    targets: Selector,
    triggers: Vec<TriggerDefinition>,
    duration: EnchantmentDuration,
    silence_removable: bool,
}

Selector::AttachedEntity

TriggerCondition::EventTargetsAttachedEntity
TriggerCondition::EventControllerIs(PlayerSelector)
```

Import `TriggerDefinition` into `core/effect.rs` and `PlayerSelector` into `core/trigger.rs`. Add:

```rust
#[error("invalid trigger enchantment: {0}")]
InvalidTriggerEnchantment(String),
```

to `SimulationError`.

- [ ] **Step 4: Regenerate immediately, then implement attachment-aware evaluation**

Run `bazel run //:gazelle`. In `simulation_effect_executor::select_entities`, resolve `Selector::AttachedEntity` through `context.source`, `game_entity`, `AttachedTo`, and the host's `GameEntityId`; return an empty vector if any link is absent.

In `trigger::selector_count`, apply the same relationship lookup for `Selector::AttachedEntity`. In `evaluate_condition`, add:

```rust
TriggerCondition::EventTargetsAttachedEntity => source
    .and_then(|source| world.get::<AttachedTo>(source))
    .and_then(|attached| world.get::<GameEntityId>(attached.0))
    .is_some_and(|attached| event.is_some_and(|event| event.targets.contains(attached))),
TriggerCondition::EventControllerIs(player) => event.is_some_and(|event| {
    let expected = match player {
        PlayerSelector::Controller => current_controller,
        PlayerSelector::Opponent => current_controller.opponent(),
        PlayerSelector::Player(player) => *player,
    };
    event.controller == expected
}),
```

- [ ] **Step 5: Regenerate immediately, then validate and execute trigger attachment**

Run `bazel run //:gazelle`. Add a validator used both by `validate_effect_program` and the execution arm:

```rust
fn validate_trigger_enchantment(
    world: &World,
    triggers: &[TriggerDefinition],
) -> Result<(), SimulationError> {
    if triggers.is_empty() {
        return Err(SimulationError::InvalidTriggerEnchantment(
            "at least one trigger is required".to_string(),
        ));
    }
    for trigger in triggers {
        if trigger.event == EventKind::Death {
            return Err(SimulationError::InvalidTriggerEnchantment(
                "added Deathrattles are not supported".to_string(),
            ));
        }
        if trigger.eligible_zones.as_slice() != [Zone::Play] {
            return Err(SimulationError::InvalidTriggerEnchantment(
                "eligible zones must be exactly [Play]".to_string(),
            ));
        }
        if trigger.source_eligibility != SourceEligibilityPolicy::MustRemainInEligibleZone {
            return Err(SimulationError::InvalidTriggerEnchantment(
                "source must remain in Play".to_string(),
            ));
        }
        validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
    }
    Ok(())
}
```

The execution arm validates before selecting targets, calls `spawn_attached_enchantment` once per selected target with definition ID `synthetic:trigger_enchantment` and display name `Trigger enchantment`, then inserts `RuntimeTriggers(triggers.clone())`.

- [ ] **Step 6: Regenerate and run focused tests**

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: creation, attachment-context, event-controller, and invalid-payload tests pass.

- [ ] **Step 7: Commit the trigger primitive**

```bash
git add hearthstone_simulator/core/effect.rs \
  hearthstone_simulator/core/trigger.rs \
  hearthstone_simulator/core/error.rs \
  hearthstone_simulator/simulator/simulation_effect_executor.rs \
  hearthstone_simulator/simulator/trigger.rs \
  hearthstone_simulator/simulator/simulation_tests_effects.rs \
  hearthstone_simulator/simulator/simulation_tests_events.rs
git commit -m "feat(hearthstone): add trigger enchantments" \
  -m "Attach validated ordinary triggers as first-class enchantment entities with explicit host and event-controller conditions." \
  -m "Constraint: Added Deathrattles remain unsupported" \
  -m "Rejected: Flatten granted triggers onto host entities" \
  -m "Confidence: high" \
  -m "Scope-risk: moderate"
```

---

### Task 4: Prove Trigger-Enchantment Lifecycle Conformance

**Files:**

- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs` only if conformance tests expose a reducer defect
- Modify: `hearthstone_simulator/simulator/trigger.rs` only if conformance tests expose an eligibility defect
- Modify: `hearthstone_simulator/simulator/simulation_action.rs` only if conformance tests expose an expiration-order defect
- Test: `hearthstone_simulator/simulator/simulation_tests_temporal.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`

**Interfaces:**

- Consumes: All Task 1-3 public types and runtime behavior.
- Produces: Focused evidence for end-turn ordering, extra-turn-series duration, source order/controller grouping, silence, and captured-trigger abortion.

- [ ] **Step 1: Add a shared trigger-enchantment fixture constructor**

In the test module that uses it, define:

```rust
fn turn_end_trigger(
    event_player: PlayerSelector,
    effects: Vec<Effect>,
) -> TriggerDefinition {
    TriggerDefinition {
        event: EventKind::TurnEnded,
        eligible_zones: vec![Zone::Play],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventControllerIs(event_player),
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: effects,
    }
}
```

- [ ] **Step 2: Test end-of-turn resolution before expiration**

Attach an `EndOfTurn(PlayerId::One)` enchantment controlled by Player One whose trigger uses `EventControllerIs(Controller)` and deals one damage to `Selector::AttachedEntity`. End Player One's turn and assert:

```rust
assert_that!(object(&mut simulation, target).damage, eq(1));
assert_that!(
    simulation.trace().iter().any(|entry| matches!(
        entry,
        TraceEntry::TriggerResolved { source, .. } if *source == enchantment
    )),
    is_true(),
);
assert_that!(
    object(&mut simulation, enchantment).zone,
    eq(Zone::RemovedFromGame),
);
```

End enough turns to reach Player One's next turn end and assert no second `TriggerResolved` entry for that enchantment:

```rust
let resolved_before = simulation.trace().iter().filter(|entry| matches!(
    entry,
    TraceEntry::TriggerResolved { source, .. } if *source == enchantment
)).count();
simulation.apply(GameAction::EndTurn { player: PlayerId::Two }).unwrap();
simulation.apply(GameAction::EndTurn { player: PlayerId::One }).unwrap();
let resolved_after = simulation.trace().iter().filter(|entry| matches!(
    entry,
    TraceEntry::TriggerResolved { source, .. } if *source == enchantment
)).count();
assert_that!(resolved_before, eq(1));
assert_that!(resolved_after, eq(resolved_before));
```

- [ ] **Step 3: Regenerate and run the end-turn test**

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: the end-turn trigger resolves once before its enchantment expires.

- [ ] **Step 4: Test turn-series duration across contiguous extra turns**

Schedule one `DuringNextTurnSeries` extra turn for Player Two. Attach an `EndOfTurnSeries(PlayerId::Two)` enchantment controlled by Player One using `EventControllerIs(Opponent)` and one damage to its host. Assert no damage at Player One's turn end, one damage after Player Two's first turn, two total damage after Player Two's contiguous extra turn, and expiration only when control returns to Player One.

Use this exact transition assertion after the fixture has scheduled the extra turn and attached the enchantment:

```rust
simulation.apply(GameAction::EndTurn { player: PlayerId::One }).unwrap();
assert_that!(object(&mut simulation, target).damage, eq(0));
assert_that!(object(&mut simulation, enchantment).zone, eq(Zone::Play));

simulation.apply(GameAction::EndTurn { player: PlayerId::Two }).unwrap();
assert_that!(simulation.snapshot().game.active_player, eq(PlayerId::Two));
assert_that!(object(&mut simulation, target).damage, eq(1));
assert_that!(object(&mut simulation, enchantment).zone, eq(Zone::Play));

simulation.apply(GameAction::EndTurn { player: PlayerId::Two }).unwrap();
assert_that!(simulation.snapshot().game.active_player, eq(PlayerId::One));
assert_that!(object(&mut simulation, target).damage, eq(2));
assert_that!(
    object(&mut simulation, enchantment).zone,
    eq(Zone::RemovedFromGame),
);
```

- [ ] **Step 5: Test enchantment source order and controller grouping**

Create an ordinary Play-zone trigger before attaching an enchantment trigger, fire their shared event, and assert `TraceEntry::TriggerResolved` lists the ordinary source then the enchantment source by play order. In a separate case, attach a Player One-controlled enchantment to Player Two's minion and assert it remains in Player One's dominant-player group regardless of the host controller.

Use explicit source-order assertions for both cases:

```rust
let resolved = simulation.trace().iter().filter_map(|entry| match entry {
    TraceEntry::TriggerResolved { source, .. } => Some(*source),
    _ => None,
}).collect::<Vec<_>>();
assert_that!(resolved, eq(&vec![ordinary_source, enchantment]));

let cross_controller_resolved = cross_controller.trace().iter().filter_map(|entry| match entry {
    TraceEntry::TriggerResolved { source, .. } => Some(*source),
    _ => None,
}).collect::<Vec<_>>();
assert_that!(
    cross_controller_resolved,
    eq(&vec![player_one_enchantment, player_two_source]),
);
```

- [ ] **Step 6: Test attached-target conditions and mid-queue abortion**

Create two triggers for the same event. The earlier trigger silences or transforms the host; the later trigger is the attached enchantment with `EventTargetsAttachedEntity`. Fire an event targeting the host and assert the candidate snapshot contains the enchantment, followed by:

```rust
assert_that!(
    simulation.trace().iter().any(|entry| matches!(
        entry,
        TraceEntry::TriggerAborted { source, .. } if *source == enchantment
    )),
    is_true(),
);
assert_that!(
    simulation.trace().iter().any(|entry| matches!(
        entry,
        TraceEntry::TriggerResolved { source, .. } if *source == enchantment
    )),
    is_false(),
);
```

- [ ] **Step 7: Test silence-removable and non-removable payloads**

Attach two otherwise identical permanent trigger enchantments to separate hosts. Silence both hosts. Assert the removable enchantment is detached in `RemovedFromGame`, the non-removable enchantment remains attached in Play, and only the non-removable trigger appears in the next matching candidate snapshot.

After applying both silence effects and firing the event, assert:

```rust
assert_that!(
    object(&mut simulation, removable).zone,
    eq(Zone::RemovedFromGame),
);
assert_that!(object(&mut simulation, retained).zone, eq(Zone::Play));
let candidates = simulation.trace().iter().rev().find_map(|entry| match entry {
    TraceEntry::TriggerSnapshot { candidates, .. } => Some(candidates),
    _ => None,
}).unwrap();
assert_that!(
    candidates.iter().any(|candidate| candidate.source == retained),
    is_true(),
);
assert_that!(
    candidates.iter().any(|candidate| candidate.source == removable),
    is_false(),
);
```

- [ ] **Step 8: Regenerate and run all lifecycle tests**

```bash
bazel run //:gazelle
aspect test //hearthstone_simulator/simulator:simulator_test
```

Expected: all lifecycle conformance tests pass. If a test exposes a defect, make the smallest correction in the file named in this task, run `bazel run //:gazelle` immediately, and rerun the full simulator test target.

- [ ] **Step 9: Commit conformance coverage and any required fixes**

```bash
git add hearthstone_simulator/simulator/simulation_tests_temporal.rs \
  hearthstone_simulator/simulator/simulation_tests_events.rs \
  hearthstone_simulator/simulator/simulation_effect_executor.rs \
  hearthstone_simulator/simulator/trigger.rs \
  hearthstone_simulator/simulator/simulation_action.rs
git commit -m "test(hearthstone): cover trigger enchantment lifecycle" \
  -m "Prove turn timing, series duration, ordering, controller grouping, silence, host context, and captured-trigger abortion." \
  -m "Confidence: high" \
  -m "Scope-risk: narrow"
```

---

### Task 5: Publish Milestone Status And Run Repository Gates

**Files:**

- Modify: `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`
- Modify: `hearthstone_simulator/RULEBOOK_CONFORMANCE.md`
- Modify: `hearthstone_simulator/README.md`

**Interfaces:**

- Consumes: Passing implementation and conformance tests from Tasks 1-4.
- Produces: Accurate Milestone 7 status and complete repository verification evidence.

- [ ] **Step 1: Update the progress record**

In `IMPLEMENTATION_PROGRESS.md`, split the final Milestone 7 line so trigger payloads are checked while draw and transformation/copy remain unchecked:

```markdown
- [x] Generalized permanent, end-of-turn, and end-of-turn-series duration to first-class trigger-bearing enchantments.
- [ ] Finish the still-Partial draw and transformation/copy conformance rows.
```

Update the known-gaps paragraph to state that ordinary trigger enchantments are implemented and added Deathrattles remain a Milestone 9 policy gap.

- [ ] **Step 2: Update conformance and README claims**

In `RULEBOOK_CONFORMANCE.md`, add or update rows for explicit enchantment duration, Play-zone trigger enchantments, attachment-aware conditions, end-turn/turn-series expiration, and captured-trigger abortion. Keep `Draw, burn, and fatigue` and `Transformation and copying` at `Partial`; keep forced deaths and added Deathrattle policy planned.

In `README.md`, add first-class permanent/timed trigger enchantments to the implemented foundation and retain the statement that full Deathrattle-position/policy coverage is incomplete.

- [ ] **Step 3: Format the complete repository**

```bash
aspect format --scope=all
```

Expected: formatting succeeds and changes only files intentionally touched by the implementation or their generated BUILD metadata.

- [ ] **Step 4: Run all Hearthstone tests**

```bash
aspect test //hearthstone_simulator/...
```

Expected: core tests, simulator tests, and CLI build tests pass with zero failures.

- [ ] **Step 5: Verify changed-line coverage**

```bash
bazel run //tools/coverage -- //hearthstone_simulator/...
```

Expected: the report completes successfully and every newly introduced branch is exercised. Inspect any uncovered changed line and add a focused test before continuing.

- [ ] **Step 6: Run repository lint and full build**

```bash
aspect lint
aspect build //...
```

Expected: both commands exit successfully.

- [ ] **Step 7: Record verification and commit documentation**

Append the commands and fresh results to the verification log in `IMPLEMENTATION_PROGRESS.md`, then run `aspect format --scope=all` once more.

```bash
git add hearthstone_simulator/IMPLEMENTATION_PROGRESS.md \
  hearthstone_simulator/RULEBOOK_CONFORMANCE.md \
  hearthstone_simulator/README.md
git commit -m "docs(hearthstone): complete trigger enchantment milestone" \
  -m "Record ordinary trigger-enchantment conformance while preserving explicit draw, copy, Deathrattle, and esoteric gaps." \
  -m "Constraint: Documentation claims require focused passing tests" \
  -m "Confidence: high" \
  -m "Scope-risk: narrow"
```

- [ ] **Step 8: Confirm final repository state**

```bash
git status --short
git log --oneline -5
```

Expected: the worktree is clean and the five task commits appear in order.
