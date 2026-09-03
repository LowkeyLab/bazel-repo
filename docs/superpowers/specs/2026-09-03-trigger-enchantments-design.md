# Trigger Enchantments Design

## Status

Approved design for the next `hearthstone_simulator` Milestone 7 feature.

## Goal

Add reusable enchantments that grant ordinary triggered abilities for a permanent, end-of-turn, or end-of-turn-series duration. The model must preserve the advanced rulebook's separate enchantment identity, controller, Play-zone membership, timestamp, attachment semantics, and mid-queue removal behavior.

This feature also normalizes duration and zone representation for every attached enchantment so permanent effects are explicit rather than represented by a missing temporary-duration component.

## Rules Contract

The selected `AdvancedRulebook2026_06_26` profile follows advanced rulebook revision 913067:

- An attached enchantment is a distinct game entity in the Play zone.
- It has its own controller and play-order timestamp, including when attached to an opposing entity.
- Trigger-bearing enchantments queue alongside ordinary triggers according to controller grouping, priority, and their own play order.
- The enchantment is the trigger source. Text meaning "this minion" refers to its attached entity.
- End-of-turn triggers resolve before passive between-turn expiration.
- A captured ordinary trigger becomes ineligible if its enchantment detaches or leaves Play before resolution.
- Effects lasting through an opponent's next turn remain active through that player's contiguous extra-turn series.

Added Deathrattles are excluded. They have special host-controller ordering and remembered-source behavior and remain part of Milestone 9.

## Public Model

Rename `TemporaryDuration` to a mandatory enchantment duration:

```rust
pub enum EnchantmentDuration {
    Permanent,
    EndOfTurn(PlayerId),
    EndOfTurnSeries(PlayerId),
}
```

Every `EntityKind::Enchantment` carries `EnchantmentDuration`. Absence is not another spelling of permanence.

Normalize attachment effects so each takes a required duration. Collapse the separate permanent and temporary stat-modifier variants into one duration-bearing stat-modifier variant. Keyword, cost, continuous, and trigger attachment effects follow the same convention.

Add a data-oriented trigger attachment effect equivalent to:

```rust
Effect::AttachTriggerEnchantment {
    targets: Selector,
    triggers: Vec<TriggerDefinition>,
    duration: EnchantmentDuration,
    silence_removable: bool,
}
```

Each selected target receives a distinct enchantment entity. Keeping trigger definitions as existing serializable values avoids a second trigger language or registry.

Add attachment-aware vocabulary:

```rust
Selector::AttachedEntity
TriggerCondition::EventTargetsAttachedEntity
```

`Selector::Source` and source-relative event conditions continue to mean the enchantment itself. Card definitions use the attachment-aware forms where text refers to the enchanted entity. Other attribution differences remain explicit in individual effect programs rather than being inferred globally.

## Entity And Zone Model

Every attached enchantment is spawned with:

- `GameEntityId`
- `DefinitionId`
- `EntityKind::Enchantment`
- `Controller`
- `PlayOrder`
- `Zone::Play`
- `AttachedTo`
- `EnchantmentDuration`
- Its payload components, such as `StatModifier`, `CostModifier`, `RuntimeContinuousEffects`, or `RuntimeTriggers`
- `SilenceRemovable` when applicable

Existing stat, keyword, cost, and continuous-effect enchantments move from `Zone::SetAside` to `Zone::Play`. Enchantments do not consume board-row capacity because board membership continues to filter Play entities by board entity kind.

Detaching or expiring an enchantment removes `AttachedTo` and moves it to `RemovedFromGame`. Its duration remains part of its durable entity record. Expiration scans only active attached enchantments in Play, so removed timed enchantments are not processed repeatedly.

The runtime and checkpoint invariants require every enchantment, including one in `RemovedFromGame`, to retain an `EnchantmentDuration` component.

## Validation

The complete effect program is validated before any attachment mutation. A trigger-enchantment payload must:

- Contain at least one trigger definition.
- Exclude `EventKind::Death`.
- Use exactly `Zone::Play` as its eligible zone.
- Use `SourceEligibilityPolicy::MustRemainInEligibleZone`.
- Contain recursively valid effect programs.

Invalid payloads return a specific `SimulationError` and create no enchantments. Native effect plans pass through the same validation before being pushed onto the resolution stack.

These restrictions encode ordinary attached-trigger behavior and prevent card data from accidentally opting into the deferred Deathrattle or remembered-source policies.

## Resolution Flow

Attaching a trigger enchantment uses deterministic selector order. For each target, the reducer allocates an independent logical ID and play order, creates the entity, inserts it into the controller's Play-zone index, and establishes `AttachedTo`. No new creation-specific trace variant is required; trigger snapshots and resolutions expose the enchantment source, while later expiration or detachment uses the existing trace entries.

No trigger-specific queue or resolver path is added. Existing trigger discovery sees `RuntimeTriggers` on the Play-zone enchantment and captures:

- The enchantment's logical ID as source.
- The enchantment controller for dominant/secondary player grouping.
- The enchantment play order.
- The immutable trigger definition and definition index.

When an attachment-aware selector or condition is evaluated, it resolves `AttachedTo` from the enchantment source to the current host. A missing relationship yields no selected entity or a false condition.

Candidate membership remains immutable after queue-time capture. Resolution-time source eligibility remains live. If an earlier trigger silences the host, moves it through a reset boundary, transforms it, or otherwise detaches the enchantment, the enchantment leaves Play and the already captured trigger aborts through the existing eligibility path.

## Turn Cleanup

The current end-turn sequence already provides the required ordering:

1. Prepare and fully resolve `TurnEnded` and its nested trigger work.
2. Run the ordinary phase boundary.
3. Check the game outcome.
4. Advance the turn.
5. Expire matching enchantments before preparing `TurnStarted`.

Expiration behavior is:

- `Permanent`: never expires through turn cleanup.
- `EndOfTurn(player)`: expires when that player's turn has ended.
- `EndOfTurnSeries(player)`: expires when that player's turn has ended and the next scheduled turn belongs to another player.

Each expiration records `TemporaryEffectExpired`, detaches the enchantment, moves it to `RemovedFromGame`, and recalculates affected derived state where required. Trigger-only enchantments require no trigger-list recalculation because discovery reads active Play entities.

## Silence And Zone Movement

`silence_removable` remains independent from duration. Silence removes eligible permanent and timed enchantments alike. Non-removable enchantments survive silence regardless of duration.

Normal host movement, transformation, Hero replacement, and explicit detachment continue to apply the profile's attachment-reset policies. Moving an enchantment to `RemovedFromGame` does not strip its payload or duration; active behavior ends because it is detached and no longer in Play.

Forward movement that preserves host enchantments preserves trigger enchantments as the existing movement policy directs. The enchantment remains a Play-zone entity even when its attached host is outside Play.

## Checkpoints And Snapshots

Rename the checkpoint duration field to `enchantment_duration: Option<EnchantmentDuration>`. It remains optional at the flat checkpoint-object level because most game entities are not enchantments; validation requires it when `kind == EntityKind::Enchantment`.

Increment `CHECKPOINT_SCHEMA_VERSION` from 5 to 6. Version 5 checkpoints are rejected rather than silently assigning `Permanent` to a missing component. No backward-compatibility migration is added.

The existing checkpoint representation already preserves runtime triggers, controller, zone, play order, attachment references, and payload components. Restoration rebuilds relationships only after logical-reference validation, then checks the enchantment duration and zone invariants.

Canonical snapshots expose the corrected Play-zone membership. Board-row projections remain unchanged because enchantments are not board entities.

## Testing

Focused conformance and regression tests cover:

- Permanent stat, keyword, cost, and continuous enchantments surviving turn transitions.
- Every active and removed enchantment retaining an explicit duration.
- An `EndOfTurn` trigger firing during `TurnEnded`, then expiring before the next turn and not firing again.
- An `EndOfTurnSeries` trigger remaining active across contiguous extra turns and expiring after the series.
- Trigger enchantments intermingling with ordinary triggers using their own play order.
- Controller grouping when an enchantment is attached to an opponent's entity.
- Attachment-aware selection and target conditions referring to the host while traces identify the enchantment as source.
- A captured trigger aborting after an earlier trigger detaches it or removes or transforms its host.
- Every enchantment type occupying Play without consuming board-row capacity.
- Silence-removable and non-removable trigger enchantments.
- Schema-version rejection and version 6 JSON round trips for permanent and timed trigger payloads.
- Atomic rejection of empty payloads, Death triggers, invalid eligible zones, invalid source policy, and invalid nested effects.
- Existing movement, copy, transformation, Hero replacement, aura, snapshot, checkpoint, and invariant behavior after the zone migration.

Ordering-sensitive tests assert both final state and canonical trace entries.

## Documentation And Milestone Status

Update `IMPLEMENTATION_PROGRESS.md`, `RULEBOOK_CONFORMANCE.md`, and `README.md` after implementation. Mark temporary trigger payloads complete while leaving added Deathrattles and the partial draw/transformation-copy conformance rows open.

## Verification

After Rust edits, run the repository workflow in order:

```bash
bazel run //:gazelle
aspect format --scope=all
aspect test //hearthstone_simulator/...
bazel run //tools/coverage -- //hearthstone_simulator/...
aspect lint
aspect build //...
```

## Rejected Alternatives

### Copy Granted Triggers Onto The Host

Flattening payloads into the host's `RuntimeTriggers` simplifies "this minion" references but loses the enchantment's source identity, controller, timestamp, zone, and independent removal. Restoring those semantics requires provenance metadata that duplicates the enchantment entity.

### Separate Trigger-Grant Registry

A resource-backed registry can model grants explicitly but duplicates ECS identity, attachment, zone, checkpoint, and lifecycle behavior. It is less inspectable and introduces another discovery path without adding rule fidelity.
