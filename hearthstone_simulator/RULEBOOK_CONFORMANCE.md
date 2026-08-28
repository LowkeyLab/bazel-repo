# Advanced Rulebook Conformance

## Ruleset contract

The initial profile is `AdvancedRulebook2026_06_26`, based on Hearthstone Wiki advanced rulebook revision [913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067), dated 2026-06-26. Replays and canonical snapshots identify this profile explicitly.

Behavior classifications:

- **Current rule** — normative behavior enabled by the profile.
- **Compatibility quirk** — observed non-obvious behavior enabled through a named policy.
- **Historical** — recorded by the wiki but disabled in this profile.
- **Uncertain** — observation requires confirmation and is not silently assumed.
- **Card definition** — belongs in card data rather than the engine.

## Architecture migration contract

The running baseline still uses ECS `ResolutionNode` entities and frozen event/trigger queues. The target architecture replaces those execution details with immediate `EntityEvent`/Observer dispatch and a deterministic phase coordinator. During migration, conformance is defined by externally meaningful behavior rather than by preserving legacy frame entities:

- Accepted input reaches the same stable lifecycle boundary.
- Canonical snapshots contain the same logical objects, ordered zones, resources, deaths, outcome, and RNG state.
- Canonical traces preserve logical event, trigger, mutation, and ordering semantics. Architecture-specific frame records may be translated or removed by an explicitly reviewed trace-schema change.
- Ordered trigger candidates are captured before dispatch; nested reactions finish before the next candidate.
- A fork taken at a stable boundary has the same snapshot and trace and produces the same continuation under identical input.
- Divergent continuations remain isolated.

The baseline equivalence fixtures are:

| Fixture                                                                                           | Contract locked for migration                                        |
| ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `resolver::tests::active_cursor_follows_depth_first_relationship_path`                            | Depth-first ancestry, parent suspension, and leaf-cursor progression |
| `queue::tests::freeze_sorts_complete_keys_and_rejects_late_entries`                               | Immutable candidate membership and deterministic cursor order        |
| `queue::tests::event_queue_does_not_advance_until_selected_entry_finishes`                        | Parent queue suspension while nested work remains active             |
| `simulation::event_tests::death_event_trigger_queues_are_frozen_before_the_batch_resolves`        | Whole-batch trigger eligibility and no late observer admission       |
| `simulation::event_tests::simultaneous_deaths_use_global_play_order_and_cache_the_turn`           | Global death ordering and immutable captured death metadata          |
| `simulation::api_tests::migration_baseline_fixture_captures_nested_trace_snapshot_and_quiescence` | Checked-in canonical snapshot, nested trigger trace, and quiescence  |
| `simulation::api_tests::fork_preserves_snapshot_trace_and_continuation_equivalence`               | Replay consistency and identical continuation                        |
| `simulation::api_tests::forked_migration_fixture_is_isolated_after_divergent_continuations`       | Isolation between speculative worlds                                 |

These fixtures remain required until Observer-driven replacements have direct equivalence coverage. Tests may stop asserting legacy frame names only when replacement assertions cover the corresponding ordering and quiescence contract.

## Phase model

The target coordinator owns the current sequence, phase, nested dispatch state, active batch, pending choice, and safety budget. Immediate Observers execute only the single trigger selected by deterministic candidate ordering. A phase cannot advance until nested dispatch is quiescent. Named aura/stat, Death Creation, Death Event, and outcome boundaries remain coordinator steps rather than incidental Observer or Bevy schedule order.

Until Target Milestones 3 and 4 replace the baseline, `ResolutionCursor`, resolution frames, and frozen queues implement these semantics. Their presence is compatibility scaffolding, not a target rule.

## Matrix

| Area                                                                  | Classification      | Running implementation               | Target implementation                  | Focused coverage                    | Status                     |
| --------------------------------------------------------------------- | ------------------- | ------------------------------------ | -------------------------------------- | ----------------------------------- | -------------------------- |
| Stable identity across zones                                          | Current rule        | `entity`, `zone`                     | unchanged flat entity/index model      | simulation identity/invariant tests | Implemented foundation     |
| Explicit hand/deck/play ordering                                      | Current rule        | `zone::ZoneIndex`                    | unchanged authoritative zone indexes   | zone/simulation tests               | Implemented foundation     |
| Required Hero/minion runtime shapes                                   | Engine invariant    | `entity::{HeroForm, MinionForm}`     | required-component flat shapes         | bidirectional/boundary invariants   | Target implemented         |
| Runtime binary keyword materialization                                | Current rule        | marker components in `entity`        | marker-component ECS queries           | snapshot/continuation/reducer tests | Target implemented         |
| Depth-first nested resolution                                         | Current rule        | `resolver`                           | phase coordinator + immediate dispatch | resolver/equivalence fixtures       | Migration contract locked  |
| Immutable event/trigger candidate capture                             | Current rule        | `queue`, `simulation::resolve_event` | coordinator-owned ordered candidates   | freeze/cursor/reaction fixtures     | Migration contract locked  |
| Normal trigger ordering by controller, zone, priority, and play order | Current rule        | `queue::TriggerOrderKey`, `trigger`  | event manager before Observer dispatch | collection/depth-first tests        | Implemented foundation     |
| Death trigger mingling by named priority and global play order        | Current rule        | `trigger`, death event batches       | coordinator candidate ordering         | Deathrattle/observer ordering test  | Implemented vertical slice |
| Trigger pre-check, queue-time, and resolution-time conditions         | Current rule        | `trigger`, `queue`                   | event manager condition stages         | condition/abortion tests            | Implemented foundation     |
| Direct trigger self-nesting and repeated-event safeguards             | Current rule        | `trigger::TriggerGuards`             | coordinator dispatch guards            | guard/nested-damage tests           | Implemented foundation     |
| Native exceptional effects return primitive effect plans              | Card definition     | `native_effect`, `effect`            | native reducer-plan registry           | registered-handler test             | Implemented foundation     |
| Seeded random selection                                               | Current rule        | `rng`                                | unchanged versioned RNG                | same-seed test                      | Implemented foundation     |
| Health/Attack aura boundary steps                                     | Current rule        | `enchantment`, `resolver`            | layered recalculation schedule         | stat recalculation only             | Partial                    |
| Delayed simultaneous death creation                                   | Current rule        | `death`                              | named Death Creation step              | synthetic area-damage test          | Implemented vertical slice |
| Global simultaneous death order and frozen Death Event batches        | Current rule        | `death`, event queue                 | coordinator death batch                | cross-controller/frozen-batch tests | Implemented vertical slice |
| Hero defeat locked at Death Creation                                  | Current rule        | `death::DefeatedHeroes`              | named Death Creation step              | lethal-then-heal test               | Implemented vertical slice |
| Death records, turn cache, Deathrattles, and chained Death Phases     | Current rule        | `death`, phase-boundary driver       | coordinator death loop                 | cache/chained Deathrattle tests     | Implemented vertical slice |
| Proposed and actual damage/healing event reactions                    | Current rule        | effect reducer, event batches        | primitive reducers + EntityEvents      | value-modifier/no-op reaction tests | Implemented vertical slice |
| DH1/DH2 ordered proposals, mutations, then frozen actual reactions    | Current rule        | immutable event batches/queues       | coordinator-owned batch envelopes      | predamage/interleaving tests        | Implemented vertical slice |
| Damage protection (Armor, Immune, Divine Shield)                      | Current rule        | effect reducer                       | primitive health reducer               | protection/Armor trigger tests      | Implemented vertical slice |
| Draw, burn, and fatigue                                               | Current rule        | effect reducer/zone index            | primitive draw reducer                 | fixture coverage                    | Partial                    |
| Transformation and copying                                            | Current rule        | effect reducer                       | in-place form reducer/shared data      | fixture coverage                    | Partial                    |
| Forced Death Phase timing                                             | Compatibility quirk | named ruleset policy                 | named coordinator plan                 | esoteric tests                      | Planned                    |
| Historical retired interactions                                       | Historical          | excluded by profile                  | excluded by profile                    | profile tests                       | Planned                    |
| Official card-specific exceptions                                     | Card definition     | definition/native effects            | definition/native reducer plans        | fixture-specific tests              | Out of engine scope        |

## Core invariants

1. Every `GameObject` has one immutable `GameEntityId`, and the index agrees with ECS membership.
2. Every zoned game entity occurs exactly once in the authoritative zone index.
3. Every Hero/minion `EntityKind` agrees bidirectionally with exactly one runtime form marker, and all shared and form-specific required components are present at setup, action, and fork boundaries.
4. Runtime binary keywords are represented by marker components, included in canonical snapshots, and rebuilt explicitly by spawn, transformation, copy, and silence reducers.
5. Gameplay ordering never uses Bevy query order, Observer registration order, or raw `Entity` values.
6. Ordered event/trigger candidate membership cannot change after capture.
7. A parent operation remains suspended until its active nested reaction reaches quiescence.
8. Execution-only state never occurs in gameplay zones or canonical logical-object snapshots.
9. Canonical snapshots and traces contain logical IDs only.
10. Stable `AwaitingAction` and `Complete` boundaries have no pending execution work.
11. Forked worlds have equal snapshots and traces at the clone boundary, continue equivalently under identical input, and remain isolated under divergent input.

For the legacy baseline, invariants 6, 7, 8, and 10 are enforced by frozen queues, the active leaf cursor, non-zoned resolution entities, and absence of a live resolution root. The Observer architecture must enforce the same semantics through coordinator state.

This matrix grows alongside implementation. “Implemented” requires a focused test; merely defining a type does not satisfy a rule.
