# Hearthstone Milestone 7 Conformance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the simulator's general draw, burn, fatigue, transformation, and copying rules and mark Milestone 7 complete.

**Architecture:** Add narrow serializable resolver operations, draw-result slots, and explicit transform/copy policies on top of the existing one-shot LIFO engine. Reuse prepared events, damage resolution, zone capacity, deterministic IDs, and aura refreshes; keep each mechanic checkpointable and traceable without adding a universal mutation framework.

**Tech Stack:** Rust 2024, Bevy 0.19 ECS, serde/serde_json, googletest, Bazel/Aspect, Gazelle.

**Spec:** `docs/superpowers/specs/2026-09-07-hearthstone-milestone-7-conformance-design.md`

## Global Constraints

- Ruleset remains `AdvancedRulebook2026_06_26`, pinned to Advanced Rulebook revision `913067` dated `2026-06-26`.
- Fix the number and player order of multi-draw requests up front, but select each card from the then-current deck top when its request executes.
- Burn is neither draw, discard, nor death; fatigue damage uses the ordinary proposed/actual damage pipeline.
- Keep runtime state serializable and use logical IDs only in effects, operations, events, traces, and checkpoints.
- Preserve deterministic ordering explicitly; never depend on Bevy query or raw entity order.
- Keep official cards, historical quirks, Milestone 8 actions, and Milestone 9 compatibility policies out of scope.
- The narrow played-self transform sequence required by the rulebook is in scope; unrelated player-action sequence work is not.
- Use Bazel/Aspect only. Do not invoke Cargo directly.
- After every Rust source edit, immediately run `bazel run //:gazelle` before formatting or testing.
- Before completion, run `aspect format --scope=all`, all Hearthstone tests, lint, changed-line coverage, and the full repository build.

---

## File Structure

- `hearthstone_simulator/core/ids.rs`: add the logical draw-result slot ID.
- `hearthstone_simulator/core/effect.rs`: add draw continuation bindings and explicit transform/copy policy data.
- `hearthstone_simulator/core/event.rs`: add draw and narrow transform-sequence event kinds.
- `hearthstone_simulator/core/resolver.rs`: own serializable draw slots, requests, outcomes, and one-shot operations.
- `hearthstone_simulator/core/trace.rs`: define canonical draw, transform, and copy trace records.
- `hearthstone_simulator/core/checkpoint.rs`: bump the checkpoint schema after all serialized contracts are present.
- `hearthstone_simulator/core/error.rs`: expose structural transform/copy errors while resolver slot errors remain `ResolutionError`.
- `hearthstone_simulator/core/lib.rs`: export new public contracts.
- `hearthstone_simulator/simulator/resolver.rs`: allocate, fill, consume, clear, and validate draw slots.
- `hearthstone_simulator/simulator/simulation_player.rs`: perform one draw outcome and schedule fatigue damage.
- `hearthstone_simulator/simulator/simulation_effect_executor.rs`: expand effects into draw/transform/copy operations and apply state-transfer policies.
- `hearthstone_simulator/simulator/simulation_event_resolver.rs`: dispatch new operations and prepare ordered events.
- `hearthstone_simulator/simulator/simulation_action.rs`: add only the played-self transform timing needed by this milestone.
- `hearthstone_simulator/simulator/simulation_card_runtime.rs`: validate capacity before copy ID allocation and spawn captured forms.
- `hearthstone_simulator/simulator/enchantment.rs`: capture and clone eligible attachments deterministically.
- `hearthstone_simulator/simulator/zone.rs`: represent burn without routing it through discard semantics.
- `hearthstone_simulator/simulator/simulation_checkpoint.rs`: validate new counters, slots, operations, and logical references.
- `hearthstone_simulator/simulator/simulation_tests_effects.rs`: draw continuation and transform reducer coverage.
- `hearthstone_simulator/simulator/simulation_tests_events.rs`: draw and played-self trigger ordering coverage.
- `hearthstone_simulator/simulator/simulation_tests_movement.rs`: burn and non-Play copy coverage.
- `hearthstone_simulator/simulator/simulation_tests_auras.rs`: Play-copy and delayed transform aura coverage.
- `hearthstone_simulator/simulator/simulation_tests_api.rs`: checkpoint, trace, and invariant coverage.
- `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`, `hearthstone_simulator/RULEBOOK_CONFORMANCE.md`, `hearthstone_simulator/README.md`: report completion only after all focused tests pass.

---

### Task 1: Add Serializable Milestone 7 Contracts

**Files:**

- Modify: `hearthstone_simulator/core/ids.rs`
- Modify: `hearthstone_simulator/core/effect.rs`
- Modify: `hearthstone_simulator/core/event.rs`
- Modify: `hearthstone_simulator/core/resolver.rs`
- Modify: `hearthstone_simulator/core/trace.rs`
- Modify: `hearthstone_simulator/core/lib.rs`
- Modify: `hearthstone_simulator/simulator/lib.rs`

**Interfaces:**

- Consumes: existing `GameEntityId`, `PlayerId`, `EffectContext`, `Card`, `Zone`, and `ResolutionOp` serialization.
- Produces: `DrawResultSlotId`, `DrawRequest`, `DrawOutcome`, `DrawResultSlot`, `DrawContinuationPolicy`, `TransformKind`, `CopyStatePolicy`, `CopyRequest`, new effect/event/operation/trace variants, and `EffectContext::drawn_card`.

- [ ] **Step 1: Write failing core contract tests**

Add tests in `core/resolver.rs` that instantiate every new operation and assert stable `kind()` names. Add serde round-trip assertions for a `ResolutionWork` containing an empty and a filled draw slot.

```rust
#[test]
fn milestone_seven_operations_have_stable_kind_names() {
    let request = DrawRequest {
        player: PlayerId::One,
        source: Some(GameEntityId(7)),
        result: DrawResultSlotId(3),
    };
    assert_eq!(ResolutionOp::ProcessDraw(request).kind(), "ProcessDraw");
    assert_eq!(
        ResolutionOp::FinishDraw(DrawResultSlotId(3)).kind(),
        "FinishDraw"
    );
}
```

- [ ] **Step 2: Regenerate and verify the tests fail**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/core:core_test --test_filter="milestone_seven_operations_have_stable_kind_names"`

Expected: FAIL because the draw contracts and operation variants do not exist.

- [ ] **Step 3: Add the core data contracts**

Add the following shapes, deriving the same serde/equality traits as neighboring contracts:

```rust
pub struct DrawResultSlotId(pub u64);

pub struct DrawRequest {
    pub player: PlayerId,
    pub source: Option<GameEntityId>,
    pub result: DrawResultSlotId,
}

pub enum DrawOutcome {
    Drawn(GameEntityId),
    Burned(GameEntityId),
    Fatigue { amount: i32 },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DrawResultSlot {
    pub outcome: Option<DrawOutcome>,
}

pub enum DrawContinuationPolicy {
    RequireCard,
    RunWithoutCard,
}

pub enum TransformKind {
    Spell,
    NonSpell,
    PlayedSelf,
}

pub enum CopyStatePolicy {
    CurrentForm,
    InPlayState,
}

pub struct CopyRequest {
    pub source: GameEntityId,
    pub controller: PlayerId,
    pub destination: Zone,
    pub board_index: Option<usize>,
    pub policy: CopyStatePolicy,
}
```

Extend `Effect` with `DrawThen`, add `kind: TransformKind` to `Transform`, and add `board_index: Option<usize>` to `Copy`. Add `Selector::DrawnCard`, `ValueExpression::DrawnCardCost`, and `drawn_card: Option<GameEntityId>` to `EffectContext`.

```rust
DrawThen {
    player: PlayerSelector,
    effects: Vec<Effect>,
    policy: DrawContinuationPolicy,
},
```

Add `EventKind::CardDrawn`, `EventKind::AfterPlayAndSummon`, and `EventKind::AfterPlay`. Add these resolver variants:

```rust
ProcessDraw(DrawRequest),
FinishDraw(DrawResultSlotId),
ContinueDraw {
    result: DrawResultSlotId,
    context: EffectContext,
    effects: Vec<Effect>,
    policy: DrawContinuationPolicy,
},
TransformEntity {
    target: GameEntityId,
    source: Option<GameEntityId>,
    card: Card,
    kind: TransformKind,
},
CopyEntity(CopyRequest),
```

Extend `ResolutionWork` with `next_draw_result_slot_id: u64`, `draw_result_slots: BTreeMap<DrawResultSlotId, DrawResultSlot>`, and `pending_played_self_transforms: BTreeSet<GameEntityId>`. The pending set is introduced with checkpoint schema 7 even though Task 7 first populates it, avoiding a second incompatible schema change inside this feature. Add trace variants carrying complete source/result policy data:

```rust
DrawResolved {
    player: PlayerId,
    source: Option<GameEntityId>,
    outcome: DrawOutcome,
},
EntityTransformed {
    entity: GameEntityId,
    previous_definition: String,
    replacement_definition: String,
    kind: TransformKind,
},
EntityCopied {
    source: GameEntityId,
    copy: GameEntityId,
    policy: CopyStatePolicy,
},
```

Export all public contracts through `core/lib.rs` and update internal imports/re-exports in `simulator/lib.rs`. Set `drawn_card: None` in every existing `EffectContext` literal and set explicit transform/copy fields in existing fixtures without changing behavior.

- [ ] **Step 4: Regenerate, format, and run core plus simulator compilation tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/core:core_test`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test`

Expected: PASS; existing behavior is unchanged and all serialized variants compile.

- [ ] **Step 5: Commit the contract slice**

```bash
git add hearthstone_simulator/core/ids.rs hearthstone_simulator/core/effect.rs hearthstone_simulator/core/event.rs hearthstone_simulator/core/resolver.rs hearthstone_simulator/core/trace.rs hearthstone_simulator/core/lib.rs hearthstone_simulator/simulator/lib.rs
git commit -m "feat(hearthstone): add milestone 7 resolver contracts"
```

---

### Task 2: Implement Draw Slot Lifecycle And Invariants

**Files:**

- Modify: `hearthstone_simulator/core/resolver.rs`
- Modify: `hearthstone_simulator/simulator/resolver.rs`
- Test: `hearthstone_simulator/simulator/resolver.rs`

**Interfaces:**

- Consumes: `DrawResultSlotId`, `DrawOutcome`, `DrawResultSlot`, and `ResolutionWork::draw_result_slots` from Task 1.
- Produces: `allocate_draw_result_slot`, `fill_draw_result_slot`, and `take_draw_result`; sequence cleanup and idle invariants include draw slots.

- [ ] **Step 1: Write failing slot lifecycle tests**

Add tests beside the existing resolver tests for allocation, fill-once, consume-once, abandon cleanup, and idle rejection.

```rust
#[test]
fn draw_result_slots_fill_and_consume_exactly_once() {
    let mut world = world();
    let slot = allocate_draw_result_slot(&mut world);
    fill_draw_result_slot(&mut world, slot, DrawOutcome::Drawn(GameEntityId(9))).unwrap();
    assert!(fill_draw_result_slot(&mut world, slot, DrawOutcome::Fatigue { amount: 1 }).is_err());
    assert_eq!(
        take_draw_result(&mut world, slot).unwrap(),
        DrawOutcome::Drawn(GameEntityId(9))
    );
    assert!(take_draw_result(&mut world, slot).is_err());
}
```

- [ ] **Step 2: Regenerate and verify failure**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="draw_result_slots_fill_and_consume_exactly_once"`

Expected: FAIL because slot helpers and errors are absent.

- [ ] **Step 3: Implement strict slot APIs and cleanup**

Add `ResolutionError::{MissingDrawResultSlot, DrawResultSlotAlreadyFilled, EmptyDrawResultSlot}` and implement:

```rust
pub(crate) fn allocate_draw_result_slot(world: &mut World) -> DrawResultSlotId;
pub(crate) fn fill_draw_result_slot(
    world: &mut World,
    slot: DrawResultSlotId,
    outcome: DrawOutcome,
) -> Result<(), ResolutionError>;
pub(crate) fn take_draw_result(
    world: &mut World,
    slot: DrawResultSlotId,
) -> Result<DrawOutcome, ResolutionError>;
```

Allocation increments the counter before inserting one empty slot. Fill rejects missing or already-filled slots. Take removes the slot and rejects an empty result. Include draw slots and `pending_played_self_transforms` in `begin_sequence`, `abandon_sequence`, and `assert_resolution_invariants`; the idle error text must mention draw slots and transform timing markers.

- [ ] **Step 4: Regenerate, format, and run resolver tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="resolver::tests"`

Expected: PASS, including duplicate-fill and cleanup coverage.

- [ ] **Step 5: Commit draw slot lifecycle**

```bash
git add hearthstone_simulator/core/resolver.rs hearthstone_simulator/simulator/resolver.rs
git commit -m "feat(hearthstone): add draw result slots"
```

---

### Task 3: Resolve Draw, Burn, Fatigue, And Draw Events

**Files:**

- Modify: `hearthstone_simulator/core/zone.rs`
- Modify: `hearthstone_simulator/simulator/zone.rs`
- Modify: `hearthstone_simulator/simulator/simulation_player.rs`
- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_effects.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_movement.rs`

**Interfaces:**

- Consumes: Task 2 draw slot APIs, existing `prepare_event`, `push_resolution_ops`, `ZoneMoveRequest`, and ordinary damage execution.
- Produces: `process_draw(world, DrawRequest)`, dedicated `ProcessDraw` dispatch, semantic burn movement, sequential multi-draw behavior, and `CardDrawn` ordering.

- [ ] **Step 1: Write failing DC1, DC2, DC4, and fatigue tests**

Add focused tests that prove:

```rust
assert_that!(hand, elements_are![eq(first), eq(nested), eq(second)]);
assert_that!(trigger_sources, elements_are![eq(play_source), eq(first)]);
assert_that!(
    trace.iter().position(|entry| matches!(entry, TraceEntry::ZoneMoved { entity, to: Zone::Hand, .. } if *entity == first)),
    lt(trace.iter().position(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::CardDrawn, .. })).unwrap())
);
assert_that!(
    trace.iter().any(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::CardDrawn, targets, .. } if targets.contains(&burned))),
    is_false()
);
assert_that!(death_cache.records.iter().any(|record| record.entity == burned), is_false());
```

Build `first` with a Hand-eligible `CardDrawn` trigger whose effect is `Draw { count: 1 }`, then resolve an outer `Draw { count: 2 }`. Build `play_source` with a Play-eligible `CardDrawn` trigger and use trace-derived `trigger_sources` to prove its priority over `first`. Fill Hand before drawing `burned` and use the shown negative event/death assertions. Extend fatigue coverage to assert two empty-deck requests produce amounts `1` then `2`, and that proposed/actual damage traces complete before the next request. Represent a cross-player simultaneous instruction as an explicitly ordered `Effect::Sequence` and assert that controller order is retained.

- [ ] **Step 2: Regenerate and verify representative failures**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="draw_moves_the_card_before_play_and_hand_triggers_resolve"`

Expected: FAIL because no `CardDrawn` event is created.

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="a_burn_is_not_draw_discard_or_death"`

Expected: FAIL because burn still routes through internal discard movement and has no explicit outcome trace.

- [ ] **Step 3: Implement one-request draw resolution**

Add `ZoneMovementKind::Burn` and make full-Hand draw movement use it rather than `Discard`. Implement:

```rust
pub(super) fn process_draw(
    world: &mut World,
    request: DrawRequest,
) -> Result<(), SimulationError>;
```

For a successful move, fill `Drawn(card)`, append `TraceEntry::DrawResolved`, call `prepare_event` with `EventKind::CardDrawn`, `source: request.source`, `targets: vec![card]`, and `controller: request.player`, then push `ResolveEvent`. For a burn, fill and trace `Burned(card)` without preparing an event. For empty Deck, increment fatigue, fill and trace `Fatigue { amount }`, then call the existing damage scheduler against the active Hero with no physical card target.

In `execute_effect_operation`, replace recursive single-count `Effect::Draw` work with execution-ordered pairs:

```rust
let player = resolve_player(context.controller, *player);
let source = context.source;
let mut operations = Vec::with_capacity(*count as usize * 2);
for _ in 0..*count {
    let result = allocate_draw_result_slot(world);
    operations.extend([
        ResolutionOp::ProcessDraw(DrawRequest { player, source, result }),
        ResolutionOp::FinishDraw(result),
    ]);
}
push_resolution_ops(world, operations);
```

Dispatch `ProcessDraw` to `process_draw`; dispatch `FinishDraw` by consuming and discarding the filled result. Preserve existing Play-before-Hand ordering through the current zone buckets.

- [ ] **Step 4: Regenerate, format, and run all focused draw tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="draw"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="burn"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="fatigue"`

Expected: PASS; trace assertions prove movement precedes draw reactions and each request fully resolves before its successor.

- [ ] **Step 5: Commit draw outcomes**

```bash
git add hearthstone_simulator/core/zone.rs hearthstone_simulator/simulator/zone.rs hearthstone_simulator/simulator/simulation_player.rs hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_event_resolver.rs hearthstone_simulator/simulator/simulation_tests_effects.rs hearthstone_simulator/simulator/simulation_tests_events.rs hearthstone_simulator/simulator/simulation_tests_movement.rs
git commit -m "feat(hearthstone): resolve compliant card draws"
```

---

### Task 4: Add Draw-Result Continuations And DC5 Attribution

**Files:**

- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_effects.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`

**Interfaces:**

- Consumes: `Effect::DrawThen`, `Selector::DrawnCard`, `ValueExpression::DrawnCardCost`, `ContinueDraw`, and Task 3 draw outcomes.
- Produces: bound drawn-card contexts, post-reaction cost reads, success-required behavior, and isolated nested draw attribution.

- [ ] **Step 1: Write failing continuation tests**

Create a synthetic card whose draw trigger changes its cost before a `DrawThen` continuation reads `DrawnCardCost` to deal damage. Add burn/fatigue tests for both continuation policies and a nested-draw test where only the outer request's result is bound.

```rust
let effect = Effect::DrawThen {
    player: PlayerSelector::Controller,
    effects: vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::DrawnCardCost,
    }],
    policy: DrawContinuationPolicy::RequireCard,
};
```

- [ ] **Step 2: Regenerate and verify failure**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="draw_continuation_reads_cost_after_draw_triggers"`

Expected: FAIL because `DrawThen` has no execution path and the bound selector/value are unsupported.

- [ ] **Step 3: Implement continuation binding**

Expand `DrawThen` in this exact execution order:

```rust
let result = allocate_draw_result_slot(world);
push_resolution_ops(
    world,
    [
        ResolutionOp::ProcessDraw(DrawRequest {
            player,
            source: context.source,
            result,
        }),
        ResolutionOp::ContinueDraw {
            result,
            context: context.clone(),
            effects: effects.clone(),
            policy: *policy,
        },
    ],
);
```

`ContinueDraw` consumes its private slot. For `Drawn(card)`, clone the context with `drawn_card: Some(card)` and push its effects. For `Burned` or `Fatigue`, skip `RequireCard`; run `RunWithoutCard` with `drawn_card: None`. `Selector::DrawnCard` returns zero or one entity. `DrawnCardCost` returns the bound entity's current `CardRuntime.cost`, or `0` when no valid binding exists.

- [ ] **Step 4: Regenerate, format, and run continuation/event tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="draw_continuation"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="nested_draw"`

Expected: PASS; every continuation starts after draw reactions and consumes only its own result.

- [ ] **Step 5: Commit continuation support**

```bash
git add hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_event_resolver.rs hearthstone_simulator/simulator/simulation_tests_effects.rs hearthstone_simulator/simulator/simulation_tests_events.rs
git commit -m "feat(hearthstone): add draw result continuations"
```

---

### Task 5: Checkpoint Pending Draw Work

**Files:**

- Modify: `hearthstone_simulator/core/checkpoint.rs`
- Modify: `hearthstone_simulator/simulator/simulation_checkpoint.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_api.rs`

**Interfaces:**

- Consumes: serializable draw operations, result slots, effects, contexts, and counters from Tasks 1-4.
- Produces: checkpoint schema version `7`, monotonic draw-slot checks, operation/slot/entity reference validation, and fork-equivalent pending draw continuation.

- [ ] **Step 1: Write failing checkpoint tests**

Add tests that construct retained draw operations and slots directly, serialize to JSON, restore, and reject stale counters or missing references. Use a synthetic `RequestChoice` above a pending draw continuation to prove suspended restoration without adding a card-level choice effect.

```rust
let slot = DrawResultSlotId(4);
work.next_draw_result_slot_id = 5;
work.draw_result_slots.insert(slot, DrawResultSlot::default());
work.stack.push(StackedResolutionOp {
    id: ResolutionId(8),
    operation: ResolutionOp::ProcessDraw(DrawRequest {
        player: PlayerId::One,
        source: Some(source),
        result: slot,
    }),
});
```

- [ ] **Step 2: Regenerate and verify stale-counter rejection fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="checkpoints_reject_stale_draw_slot_counters"`

Expected: FAIL because checkpoint validation ignores draw counters and slot references.

- [ ] **Step 3: Implement schema and recursive validation**

Set `CHECKPOINT_SCHEMA_VERSION` to `7`. Validate that every stored draw slot ID and every operation-referenced slot is lower than `next_draw_result_slot_id`; operations that consume a slot must reference a retained slot. Validate `DrawRequest::source`, `EffectContext::{source,declared_target,drawn_card}`, `Selector::Entity`, event sources/targets, copy sources, transform targets, bound entities, and every `pending_played_self_transforms` member against the checkpoint entity set.

Retain recursive validation for nested choices and effect programs. Reject schema `6` with the existing version-mismatch path rather than supplying backward compatibility.

- [ ] **Step 4: Regenerate, format, and run checkpoint tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="checkpoint"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="fork"`

Expected: PASS; JSON restoration preserves pending draw state and invalid references fail deterministically.

- [ ] **Step 5: Commit checkpoint schema 7**

```bash
git add hearthstone_simulator/core/checkpoint.rs hearthstone_simulator/simulator/simulation_checkpoint.rs hearthstone_simulator/simulator/simulation_tests_api.rs
git commit -m "feat(hearthstone): checkpoint pending draw work"
```

---

### Task 6: Make Transformation Atomic And Complete

**Files:**

- Modify: `hearthstone_simulator/core/error.rs`
- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- Modify: `hearthstone_simulator/simulator/enchantment.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_effects.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_auras.rs`

**Interfaces:**

- Consumes: `TransformKind` and `ResolutionOp::TransformEntity` from Task 1; existing component schema and attachment relationships.
- Produces: validated atomic `transform_entity(world, target, card, kind)`, deterministic detachment, complete form reset, stable identity, and transform tracing.

- [ ] **Step 1: Write failing atomic transform tests**

Expand existing transformation tests to cover missing target atomicity, deterministic attachment removal, role/cache component cleanup, stable ID/zone/position, trace content, no Death/Summoned event, and delayed old-provider aura expiration.

```rust
let before = simulation.checkpoint().unwrap();
let result = transform_entity(
    simulation.app.world_mut(),
    GameEntityId(u64::MAX),
    Card::minion("Replacement", 2, 2, 3),
    TransformKind::Spell,
);
assert_that!(result, err(anything()));
assert_that!(simulation.checkpoint().unwrap(), eq(&before));
```

- [ ] **Step 2: Regenerate and verify atomicity test fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="invalid_transformation_is_atomic"`

Expected: FAIL because transform currently detaches before target lookup and has no structural validation.

- [ ] **Step 3: Implement explicit transform operation and policy**

Change `Effect::Transform` execution to push one `TransformEntity` per selected target, carrying `source: context.source`. Dispatch that operation to:

```rust
pub(super) fn transform_entity(
    world: &mut World,
    target: GameEntityId,
    card: Card,
    kind: TransformKind,
) -> Result<(), SimulationError>;
```

Before mutation, resolve the target, validate the replacement program and required components, capture the previous definition, and collect attachments sorted by `(PlayOrder, GameEntityId)`. Then detach attachments and replace all form-owned components. Explicitly remove old role markers, received aura caches, `PendingDestroy`, `Silenced`, and `KeepEnchantments`; rebuild only components required by the replacement form. Preserve `GameEntityId`, `Controller`, `Zone`, `ZonePosition`, and the existing play order. Append `EntityTransformed` after mutation.

Do not refresh global aura caches during the reducer. This preserves old-provider applications until the scheduled aura boundary while preventing target-local received caches from masquerading as replacement state.

- [ ] **Step 4: Regenerate, format, and run transform tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="transform"`

Expected: PASS; failures are atomic and transformed state follows the explicit preserved/replaced/removed matrix.

- [ ] **Step 5: Commit atomic transformation**

```bash
git add hearthstone_simulator/core/error.rs hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_event_resolver.rs hearthstone_simulator/simulator/enchantment.rs hearthstone_simulator/simulator/simulation_tests_effects.rs hearthstone_simulator/simulator/simulation_tests_events.rs hearthstone_simulator/simulator/simulation_tests_auras.rs
git commit -m "feat(hearthstone): make transformations atomic"
```

---

### Task 7: Add Narrow Transformation Timing

**Files:**

- Modify: `hearthstone_simulator/core/resolver.rs`
- Modify: `hearthstone_simulator/simulator/resolver.rs`
- Modify: `hearthstone_simulator/simulator/simulation_action.rs`
- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_auras.rs`

**Interfaces:**

- Consumes: Task 6 transform reducer, trigger seed capture, prepared events, `RefreshAuras(AuraRefreshPlan::Summon)`, and phase-boundary operations.
- Produces: spell/no-summon timing, non-spell Summon Resolution participation, and played-self inserted/original after-play ordering.

- [ ] **Step 1: Write failing timing tests**

Add one trace-based test per `TransformKind`. The played-self fixture must have a pre-transform `AfterPlay` trigger and a post-transform `AfterPlayAndSummon` trigger and assert this execution order:

```text
transform
post-transform inserted event
between-phase boundary work
pre-transform after-play event
```

Assert `Spell` produces none of that summon work and `NonSpell` refreshes summon auras without emitting an ordinary `Summoned` event.

- [ ] **Step 2: Regenerate and verify played-self ordering fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="played_self_transform_uses_inserted_then_original_after_play_order"`

Expected: FAIL because the current action compiler has no inserted/original after-play model.

- [ ] **Step 3: Implement only the required transform timing plan**

Add a serializable post-program barrier carrying the stable subject and captured pre-transform seeds:

```rust
FinishPlayedSelfTransform {
    subject: GameEntityId,
    original_after_play: Vec<TriggerSeed>,
},
```

Use `ResolutionWork::pending_played_self_transforms`, introduced and checkpointed in Tasks 1-5. When `play_card` compiles a minion program containing a source-targeted `TransformKind::PlayedSelf`, collect original `AfterPlay` seeds after the card enters Play and append `FinishPlayedSelfTransform` beneath the complete effect program. A successful `TransformEntity { kind: PlayedSelf }` requires `source == Some(target)` and inserts the target into the pending set after replacement. The finish barrier removes the marker and no-ops when none exists; otherwise it prepares the post-transform `AfterPlayAndSummon` event from current state and the original `AfterPlay` event from the captured seeds. Add this helper so the original event does not rediscover seeds from the transformed form:

```rust
pub(super) fn prepare_event_with_seeds(
    world: &mut World,
    context: EventContext,
    seeds: Vec<TriggerSeed>,
) -> EventId;
```

Push barrier consequences in this order:

```rust
[
    ResolutionOp::ResolveEvent(inserted_event),
    ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
    ResolutionOp::ResolveEvent(original_after_play_event),
]
```

For `NonSpell`, push `RefreshAuras(AuraRefreshPlan::Summon)` only; do not create `Summoned`. For `Spell`, push no transform-specific timing. Reject `PlayedSelf` unless `source == Some(target)` and a matching finish barrier remains on the stack. Detect `PlayedSelf` recursively inside `Effect::Sequence`, but do not add weapon, Hero Power, location, or general Milestone 8 sequences.

- [ ] **Step 4: Regenerate, format, and run transform timing tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="transform"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="inserted"`

Expected: PASS with exact trace order and no ordinary summon event for transformations.

- [ ] **Step 5: Commit transform timing**

```bash
git add hearthstone_simulator/core/resolver.rs hearthstone_simulator/simulator/resolver.rs hearthstone_simulator/simulator/simulation_action.rs hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_event_resolver.rs hearthstone_simulator/simulator/simulation_tests_events.rs hearthstone_simulator/simulator/simulation_tests_auras.rs
git commit -m "feat(hearthstone): add transformation timing"
```

---

### Task 8: Implement Explicit Non-Play Copy Policy

**Files:**

- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- Modify: `hearthstone_simulator/simulator/simulation_card_runtime.rs`
- Modify: `hearthstone_simulator/simulator/zone.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_movement.rs`

**Interfaces:**

- Consumes: `CopyRequest`, `CopyStatePolicy::CurrentForm`, explicit `CopyEntity`, existing `copy_card_data`, capacity validation, and `spawn_card`.
- Produces: execution-time source capture, append or explicit board position, no-ID no-ops, narrow error handling, and copy trace.

- [ ] **Step 1: Write failing non-Play copy tests**

Extend movement tests to assert current transformed form is copied while attachments, effective cost, damage, silence, pending destroy, aura caches, controller, position, and play order are absent. Preserve and strengthen the no-ID-consumption test for missing sources and full destinations.

```rust
assert_that!(runtime.base_cost, eq(5));
assert_that!(runtime.cost, eq(5));
assert_that!(world.get::<Silenced>(copy_entity), none());
assert_that!(world.get::<Damage>(copy_entity), eq(Some(&Damage(0))));
```

- [ ] **Step 2: Regenerate and verify explicit copy trace test fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="non_play_copy_uses_current_form_without_runtime_attachments"`

Expected: FAIL because copy has no explicit operation or trace policy.

- [ ] **Step 3: Implement `CurrentForm` copy execution**

When `Effect::Copy` selects a target, inspect its source zone and choose `CurrentForm` unless both source and destination are Play. Push `CopyEntity(CopyRequest)` rather than spawning inline.

The operation captures `copy_card_data` at execution, validates destination capacity before allocating IDs, and calls `spawn_card`. Treat only a missing source and `ZoneError::Full` as normal no-op outcomes; propagate structural/program failures. Apply destination controller and `board_index`, append `EntityCopied`, and leave all source attachments and runtime state behind.

- [ ] **Step 4: Regenerate, format, and run non-Play copy tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="copy"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="full_zones"`

Expected: PASS; missing/full copies are deterministic no-ops without ID consumption and valid copies are traced.

- [ ] **Step 5: Commit non-Play copies**

```bash
git add hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_event_resolver.rs hearthstone_simulator/simulator/simulation_card_runtime.rs hearthstone_simulator/simulator/zone.rs hearthstone_simulator/simulator/simulation_tests_movement.rs
git commit -m "feat(hearthstone): add explicit card copies"
```

---

### Task 9: Clone Play State And Enchantments

**Files:**

- Modify: `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- Modify: `hearthstone_simulator/simulator/simulation_card_runtime.rs`
- Modify: `hearthstone_simulator/simulator/enchantment.rs`
- Modify: `hearthstone_simulator/simulator/aura.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_movement.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_auras.rs`
- Test: `hearthstone_simulator/simulator/simulation_tests_events.rs`

**Interfaces:**

- Consumes: Task 8 `CopyEntity`, `CopyStatePolicy::InPlayState`, attachment relationships, play-order allocation, and summon refresh/event machinery.
- Produces: deterministic Play-to-Play state transfer, fresh cloned enchantments, reset play age, excluded aura caches, and ordinary summon resolution.

- [ ] **Step 1: Write failing Play-copy matrix tests**

Construct one source with damage, silence, pending destroy, modified keywords, all supported attachment payloads, received aura applications, nonzero attack usage, and a known board position. Assert the copy has fresh identity/play order, destination controller/position, copied non-aura state, fresh ordered attachment IDs, and default attack usage with `exhausted: true` unless Charge applies.

```rust
assert_that!(copy_damage, eq(source_damage));
assert_that!(copy_attack_state.attacks_this_turn, eq(0));
assert_that!(copy_attack_state.exhausted, is_true());
assert_that!(copy_aura_cache, none());
assert_that!(copy_enchantments, not(eq(source_enchantments)));
```

Add a trace test showing aura refresh and `Summoned` resolution finish before a later sibling effect.

- [ ] **Step 2: Regenerate and verify state-transfer test fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="play_copy_clones_non_aura_state_and_eligible_enchantments"`

Expected: FAIL because the current Play copy preserves only silence.

- [ ] **Step 3: Implement deterministic capture and clone**

Create private snapshot types that contain only approved state:

```rust
struct PlayCopySnapshot {
    card: Card,
    damage: Damage,
    silenced: bool,
    pending_destroy: bool,
    attachments: Vec<AttachmentCopySnapshot>,
}
```

Move `spawn_attached_enchantment` into `simulator/enchantment.rs` as a `pub(crate)` helper so ordinary effects and copy restoration share one relationship constructor. Collect attached enchantments in `(PlayOrder, GameEntityId)` order. Capture definition/display, duration, silence-removable status, stat/keyword/cost modifiers, runtime triggers, and runtime continuous effects. Exclude current `Keywords`, all aura cache components, and relationship entity IDs; rebuild effective keywords from native keywords plus cloned attachments so received aura keywords cannot leak into the copy.

Validate source, destination capacity, card program, and every attachment payload before spawning. Spawn the card, assign fresh play order and requested board position, restore damage/silence/pending state and copied keywords, then create each attachment with a fresh game ID and attach it to the new entity. Set:

```rust
AttackState {
    attacks_this_turn: 0,
    exhausted: true,
}
```

Recalculate stats, keywords, and cost after all attachments are cloned. Existing attack legality may let Charge or Rush override exhaustion; do not encode those exceptions by clearing the stored exhaustion flag. After successful state transfer, schedule `RefreshAuras(AuraRefreshPlan::Summon)` and a normal `Summoned` event above later sibling work. Append `EntityCopied` only after the copy is structurally complete.

- [ ] **Step 4: Regenerate, format, and run Play-copy tests**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="play_copy"`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="aura"`

Expected: PASS; source/copy mutations are independent, attachment IDs are fresh and deterministic, and summon consequences resolve depth-first.

- [ ] **Step 5: Commit Play copy behavior**

```bash
git add hearthstone_simulator/simulator/simulation_effect_executor.rs hearthstone_simulator/simulator/simulation_card_runtime.rs hearthstone_simulator/simulator/enchantment.rs hearthstone_simulator/simulator/aura.rs hearthstone_simulator/simulator/simulation_tests_movement.rs hearthstone_simulator/simulator/simulation_tests_auras.rs hearthstone_simulator/simulator/simulation_tests_events.rs
git commit -m "feat(hearthstone): copy in-play runtime state"
```

---

### Task 10: Harden Checkpoints, Close Documentation, And Verify

**Files:**

- Modify: `hearthstone_simulator/simulator/simulation_checkpoint.rs`
- Modify: `hearthstone_simulator/simulator/simulation_tests_api.rs`
- Modify: `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`
- Modify: `hearthstone_simulator/RULEBOOK_CONFORMANCE.md`
- Modify: `hearthstone_simulator/README.md`

**Interfaces:**

- Consumes: every new operation, policy, trace, component payload, and test from Tasks 1-9.
- Produces: complete recursive checkpoint validation, checkpoint/fork equivalence for transform/copy work, closed Milestone 7 status, and repository verification evidence.

- [ ] **Step 1: Write failing retained-operation and fork tests**

Add schema-7 tests with pending `TransformEntity`, `CopyEntity`, and cloned-enchantment state. Reject missing transform/copy targets and stale IDs. Fork before executing each retained operation, execute identical continuations, and compare snapshots and traces.

```rust
assert_that!(restored.snapshot(), eq(&fork.snapshot()));
assert_that!(restored.trace(), eq(fork.trace()));
assert_that!(assert_resolution_invariants(restored.app.world()), ok(anything()));
```

- [ ] **Step 2: Regenerate and verify missing-reference test fails**

Run: `bazel run //:gazelle`

Run: `aspect test //hearthstone_simulator/simulator:simulator_test --test_filter="checkpoints_reject_missing_transform_and_copy_references"`

Expected: FAIL until recursive operation validation covers transform/copy IDs and copied attachment relationships.

- [ ] **Step 3: Complete validation and pass all focused tests**

Extend operation validation to cover `TransformEntity`, `CopyEntity`, `FinishPlayedSelfTransform`, `pending_played_self_transforms`, every nested trigger seed, and all event/context logical IDs. Validate cloned attachments with the same relationship, duration, payload, and play-order rules as ordinary enchantments.

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/...`

Expected: PASS for core, simulator, and app targets.

- [ ] **Step 4: Update Milestone 7 documentation**

Only after Step 3 passes:

- Mark Milestone 7 and its draw/transformation/copy item complete in `IMPLEMENTATION_PROGRESS.md`.
- Change both conformance rows from `Partial` to `Implemented foundation` and name the focused suites.
- Update `README.md` to include explicit draw event ordering, draw-result continuation, transform timing, and zone-specific copy state.
- Add the fresh verification commands and results to the progress log.

- [ ] **Step 5: Format and run final repository verification**

Run: `aspect format --scope=all`

Run: `aspect test //hearthstone_simulator/...`

Run: `aspect lint`

Run: `bazel run //tools/coverage -- //hearthstone_simulator/...`

Expected: PASS with all changed lines covered, or one documented unreachable compiler-derived branch.

Run: `aspect build //...`

Expected: PASS for the full repository.

- [ ] **Step 6: Inspect and commit the completed milestone**

Run: `git status --short`

Run: `git diff --check`

Run: `git diff --stat`

Confirm only intended source, tests, generated BUILD metadata, and documentation are staged.

```bash
git add hearthstone_simulator/simulator/simulation_checkpoint.rs hearthstone_simulator/simulator/simulation_tests_api.rs hearthstone_simulator/IMPLEMENTATION_PROGRESS.md hearthstone_simulator/RULEBOOK_CONFORMANCE.md hearthstone_simulator/README.md docs/superpowers/specs/2026-09-07-hearthstone-milestone-7-conformance-design.md docs/superpowers/plans/2026-09-07-hearthstone-milestone-7-conformance.md
git commit -m "feat(hearthstone): complete milestone 7 conformance"
```
