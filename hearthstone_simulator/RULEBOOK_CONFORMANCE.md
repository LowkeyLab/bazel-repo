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

| Area                                                           | Classification      | Implementation                      | Focused coverage                    | Status                     |
| -------------------------------------------------------------- | ------------------- | ----------------------------------- | ----------------------------------- | -------------------------- |
| Stable identity across zones                                   | Current rule        | `entity`, `zone`                    | simulation identity/invariant tests | Implemented foundation     |
| Explicit hand/deck/play ordering                               | Current rule        | `zone::ZoneIndex`                   | zone/simulation tests               | Implemented foundation     |
| Depth-first nested resolution                                  | Current rule        | `resolver`                          | resolver ancestry/suspension tests  | Implemented foundation     |
| Immutable event/trigger queues                                 | Current rule        | `queue`                             | freeze/cursor tests                 | Implemented foundation     |
| Trigger ordering by controller, zone, priority, and play order | Current rule        | `queue::TriggerOrderKey`, `trigger` | trigger collection tests            | Implemented foundation     |
| Seeded random selection                                        | Current rule        | `rng`                               | same-seed test                      | Implemented foundation     |
| Health/Attack aura boundary steps                              | Current rule        | `enchantment`, `resolver`           | stat recalculation only             | Partial                    |
| Delayed simultaneous death creation                            | Current rule        | `death`                             | synthetic area-damage test          | Implemented vertical slice |
| Damage protection (Armor, Immune, Divine Shield)               | Current rule        | effect reducer                      | vertical mechanic tests             | Partial                    |
| Draw, burn, and fatigue                                        | Current rule        | effect reducer/zone index           | fixture coverage                    | Partial                    |
| Transformation and copying                                     | Current rule        | effect reducer                      | fixture coverage                    | Partial                    |
| Forced Death Phase timing                                      | Compatibility quirk | named ruleset policy                | esoteric tests                      | Planned                    |
| Historical retired interactions                                | Historical          | excluded by profile                 | profile tests                       | Planned                    |
| Official card-specific exceptions                              | Card definition     | definition/native effects           | fixture-specific tests              | Out of engine scope        |

## Core invariants

1. Every `GameObject` has one immutable `GameEntityId`, and the index agrees with ECS membership.
2. Every zoned game entity occurs exactly once in the authoritative zone index.
3. Gameplay ordering never uses Bevy query order or raw `Entity` values.
4. Frozen queue membership and order cannot change.
5. The active resolution cursor points to the leaf of its ancestry.
6. Resolution entities never occur in gameplay zones.
7. Canonical snapshots and traces contain logical IDs only.
8. Idle and complete simulations have no live resolution root.

This matrix grows alongside implementation. “Implemented” requires a focused test; merely defining a type does not satisfy a rule.
