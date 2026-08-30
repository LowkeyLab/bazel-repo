# Advanced Rulebook Conformance

## Ruleset contract

The initial profile is `AdvancedRulebook2026_06_26`, based on Hearthstone Wiki advanced rulebook revision [913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067), dated 2026-06-26. Replays and canonical snapshots identify this profile explicitly.

Behavior classifications:

- **Current rule** — normative behavior enabled by the profile.
- **Compatibility quirk** — observed non-obvious behavior enabled through a named policy.
- **Historical** — recorded by the wiki but disabled in this profile.
- **Uncertain** — observation requires confirmation and is not silently assumed.
- **Card definition** — belongs in card data rather than the engine.

## Matrix

| Area                                                                | Classification      | Implementation                 | Focused coverage                    | Status                     |
| ------------------------------------------------------------------- | ------------------- | ------------------------------ | ----------------------------------- | -------------------------- |
| Stable identity across zones                                        | Current rule        | `entity`, `zone`               | simulation identity/invariant tests | Implemented foundation     |
| Explicit hand/deck/play ordering                                    | Current rule        | `zone::ZoneIndex`              | zone/simulation tests               | Implemented foundation     |
| Depth-first nested resolution                                       | Current rule        | `resolver::ResolutionWork`     | LIFO/depth-first tests              | Implemented foundation     |
| Immutable event records and queue-time trigger snapshots            | Current rule        | `PreparedEvent`, `trigger`     | staged capture/reaction tests       | Implemented foundation     |
| Trigger ordering by dominant player, zone, priority, and play order | Current rule        | `DominantPlayer`, order keys   | dominant grouping/depth-first tests | Implemented foundation     |
| Same-player Death trigger mingling by priority and play order       | Current rule        | `trigger`, Death Events        | Deathrattle/observer ordering test  | Implemented vertical slice |
| Trigger pre-check, queue-time, and resolution-time conditions       | Current rule        | `trigger`, prepared events     | condition/abortion tests            | Implemented foundation     |
| Unbounded generated-work safety                                     | Engine policy       | per-sequence operation budget  | exact-operation budget test         | Implemented foundation     |
| Native exceptional effects return primitive effect plans            | Card definition     | `native_effect`, `effect`      | registered-handler test             | Implemented foundation     |
| Seeded random selection                                             | Current rule        | `rng`                          | same-seed test                      | Implemented foundation     |
| Health/Attack aura boundary steps                                   | Current rule        | `enchantment`, `resolver`      | stat recalculation only             | Partial                    |
| Delayed simultaneous death creation                                 | Current rule        | `death`                        | synthetic area-damage test          | Implemented vertical slice |
| Global Death order with staged pre-check and queue-time capture     | Current rule        | `death`, prepared events       | enabling/exclusion timing tests     | Implemented vertical slice |
| Hero defeat locked at Death Creation                                | Current rule        | `death::DefeatedHeroes`        | lethal-then-heal test               | Implemented vertical slice |
| Outcome after all chained Death Phases                              | Current rule        | explicit `CheckOutcome` op     | mutual Hero defeat draw test        | Implemented vertical slice |
| Death records, turn cache, Deathrattles, and chained Death Phases   | Current rule        | `death`, phase-boundary driver | cache/chained Deathrattle tests     | Implemented vertical slice |
| Proposed and actual damage/healing event reactions                  | Current rule        | effect reducer, event batches  | value-modifier/no-op reaction tests | Implemented vertical slice |
| DH1/DH2 ordered proposals, mutations, then delayed actual reactions | Current rule        | prepared event slots           | predamage/interleaving tests        | Implemented vertical slice |
| Damage protection (Armor, Immune, Divine Shield)                    | Current rule        | effect reducer                 | protection/Armor trigger tests      | Implemented vertical slice |
| Draw, burn, and fatigue                                             | Current rule        | effect reducer/zone index      | fixture coverage                    | Partial                    |
| Transformation and copying                                          | Current rule        | effect reducer                 | fixture coverage                    | Partial                    |
| Versioned suspended-resolution restoration                          | Engine policy       | `SimulationCheckpoint`         | JSON choice round-trip test         | Implemented foundation     |
| Forced Death Phase timing                                           | Compatibility quirk | named ruleset policy           | esoteric tests                      | Planned                    |
| Historical retired interactions                                     | Historical          | excluded by profile            | profile tests                       | Planned                    |
| Official card-specific exceptions                                   | Card definition     | definition/native effects      | fixture-specific tests              | Out of engine scope        |

## Core invariants

1. Every `GameObject` has one immutable `GameEntityId`, and the index agrees with ECS membership.
2. Every zoned game entity occurs exactly once in the authoritative zone index.
3. Gameplay ordering never uses Bevy query order or raw `Entity` values.
4. Pre-check trigger seeds are fixed at their ruleset timing; queue-time candidate membership and order cannot change after capture.
5. Only the iterative driver pops one-shot operations from the LIFO stack.
6. Resolution operations and prepared events are resource-owned values, never gameplay entities.
7. Canonical snapshots, traces, and checkpoints contain logical IDs rather than raw Bevy entity references.
8. Idle and complete simulations have no pending operations, events, slots, or choice.
9. A restored checkpoint reproduces durable ECS state, resolution work, RNG state, trace, and logical counters.

This matrix grows alongside implementation. “Implemented” requires a focused test; merely defining a type does not satisfy a rule.
