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

| Area                                                                | Classification      | Implementation                                     | Focused coverage                                   | Status                                                     |
| ------------------------------------------------------------------- | ------------------- | -------------------------------------------------- | -------------------------------------------------- | ---------------------------------------------------------- |
| Stable identity across zones                                        | Current rule        | `entity`, `zone`                                   | simulation identity/invariant tests                | Implemented foundation                                     |
| Explicit hand/deck/play ordering                                    | Current rule        | `zone::ZoneIndex`                                  | zone/simulation tests                              | Implemented foundation                                     |
| Depth-first nested resolution                                       | Current rule        | `resolver::ResolutionWork`                         | LIFO/depth-first tests                             | Implemented foundation                                     |
| Immutable event records and queue-time trigger snapshots            | Current rule        | `PreparedEvent`, `trigger`                         | staged capture/reaction tests                      | Implemented foundation                                     |
| Trigger ordering by dominant player, zone, priority, and play order | Current rule        | `DominantPlayer`, order keys                       | dominant grouping/depth-first tests                | Implemented foundation                                     |
| Same-player Death trigger mingling by priority and play order       | Current rule        | `trigger`, Death Events                            | Deathrattle/observer ordering test                 | Implemented vertical slice                                 |
| Trigger pre-check, queue-time, and resolution-time conditions       | Current rule        | `trigger`, prepared events                         | condition/abortion tests                           | Implemented foundation                                     |
| Explicit permanent and timed enchantment duration                   | Current rule        | `EnchantmentDuration`                              | schema/checkpoint invariant tests                  | Implemented foundation                                     |
| Play-zone trigger enchantments outside board-row capacity           | Current rule        | `enchantment`, `trigger`                           | capacity/play-order tests                          | Implemented foundation                                     |
| Attachment-aware host and event-controller conditions               | Current rule        | `TriggerCondition`                                 | validation/controller-grouping tests               | Implemented foundation                                     |
| End-turn and turn-series trigger-enchantment expiration             | Current rule        | phase-boundary expiration                          | natural/extra-turn lifetime tests                  | Implemented foundation                                     |
| Captured trigger abortion after host transformation                 | Current rule        | prepared events, `trigger`                         | transformation-abortion test                       | Implemented foundation                                     |
| Unbounded generated-work safety                                     | Engine policy       | per-sequence operation budget                      | exact-operation budget test                        | Implemented foundation                                     |
| Native exceptional effects return primitive effect plans            | Card definition     | `native_effect`, `effect`                          | registered-handler test                            | Implemented foundation                                     |
| Seeded random selection                                             | Current rule        | `rng`                                              | same-seed test                                     | Implemented foundation                                     |
| Health H1/H2 maximum/current behavior                               | Current rule        | damage-preserving stat reducer                     | gain/clamp/preserve tests                          | Implemented foundation                                     |
| Health/Attack aura boundary steps                                   | Current rule        | typed category caches                              | Health persistence/Attack expiry                   | Implemented foundation                                     |
| Aura Update (Other)                                                 | Current rule        | `OtherAuraCache`, resolver                         | summon/Immune/post-death tests                     | Implemented foundation                                     |
| Played and summoned aura refresh timing                             | Current rule        | planned `RefreshAuras` op                          | provider-only/global refresh tests                 | Implemented foundation                                     |
| Continuously evaluated Spell Damage                                 | Current rule        | live continuous contributions                      | attached/opponent/silence tests                    | Implemented foundation                                     |
| Delayed simultaneous death creation                                 | Current rule        | `death`                                            | synthetic area-damage test                         | Implemented vertical slice                                 |
| Global Death order with staged pre-check and queue-time capture     | Current rule        | `death`, prepared events                           | enabling/exclusion timing tests                    | Implemented vertical slice                                 |
| Hero defeat locked at Death Creation                                | Current rule        | `death::DefeatedHeroes`                            | lethal-then-heal test                              | Implemented vertical slice                                 |
| Outcome after all chained Death Phases                              | Current rule        | explicit `CheckOutcome` op                         | mutual Hero defeat draw test                       | Implemented vertical slice                                 |
| Death records, turn cache, Deathrattles, and chained Death Phases   | Current rule        | `death`, phase-boundary driver                     | cache/chained Deathrattle tests                    | Implemented vertical slice                                 |
| Proposed and actual damage/healing event reactions                  | Current rule        | effect reducer, event batches                      | value-modifier/no-op reaction tests                | Implemented vertical slice                                 |
| DH1/DH2 ordered proposals, mutations, then delayed actual reactions | Current rule        | prepared event slots                               | predamage/interleaving tests                       | Implemented vertical slice                                 |
| Damage protection (Armor, Immune, Divine Shield)                    | Current rule        | effect reducer                                     | protection/Armor trigger tests                     | Implemented vertical slice                                 |
| Draw, burn, and fatigue                                             | Current rule        | draw slots/events/continuations                    | effect/event/movement/API suites                   | Implemented foundation                                     |
| Transformation and copying                                          | Current rule        | explicit transform/copy ops                        | effect/event/aura/movement/API suites              | Implemented foundation                                     |
| Zone generation, movement capacity, direction, and reset policies   | Current rule        | profile capacity/movement rules                    | limits/forward/backward/full-zone                  | Implemented foundation                                     |
| Deterministic movement and globally ordered full-zone Death Events  | Current rule        | movement/death ledger                              | mixed instant/ordinary Deathrattles                | Implemented vertical slice                                 |
| Extra turns and turn-series temporary effects                       | Current rule        | precedence schedule/durations                      | turn series/stat/keyword/cost expiry               | Partial                                                    |
| Active Hero Power Battlefield membership and singleton role         | Current rule        | Play role/capacity invariant                       | zone/board/checkpoint tests                        | Implemented foundation                                     |
| Hero replacement, aura timing, and irreversible defeat timing       | Current rule        | replacement reducer                                | aura/before/after Death Creation                   | Implemented vertical slice                                 |
| Versioned suspended-resolution restoration                          | Engine policy       | `SimulationCheckpoint`                             | JSON/reference-validation tests                    | Implemented foundation                                     |
| Explicit targeting filter foundation                                | Engine policy       | `TargetRequirement`, validator                     | target requirement/atomicity tests                 | Implemented foundation; audience/kind and Stealth/Immune   |
| Canonical supported action enumeration and normalization            | Engine policy       | `validate_action`, `legal_actions`                 | exhaustive/soundness/purity tests                  | Implemented foundation                                     |
| Guarded deferred steps and checkpoint restoration                   | Engine policy       | `SubjectGuard`, schema 13                          | skip/round-trip/fork/reference tests               | Implemented foundation; schemas 7/8/9/10/11/12 rejected    |
| Forced Death Phase timing                                           | Compatibility quirk | named ruleset policy                               | esoteric tests                                     | Planned                                                    |
| Added Deathrattles and Deathrattle-position policy                  | Current rule        | named ruleset policy                               | esoteric tests                                     | Planned                                                    |
| Historical retired interactions                                     | Historical          | excluded by profile                                | profile tests                                      | Planned                                                    |
| Official card-specific exceptions                                   | Card definition     | definition/native effects                          | fixture-specific tests                             | Out of engine scope                                        |
| Hero Power activation and original-power completion                 | Engine policy       | action validation, sequence steps, captured events | Hero activation/targeting/replacement/choice tests | Implemented synthetic foundation; pinned timing unverified |

Taunt/Stealth coverage is implemented in `simulation_tests_actions.rs`: direct-target declarations,
Taunt suppression (including aura-granted Immune), durable consumption and regranting, copy and
movement resets, damage prevention, and checkpoint-exact suspension. Its simplified combat-step
placement is classified as engine policy, with full phase conformance still unverified.

## Core invariants

1. Every `GameObject` has one immutable `GameEntityId`, and the index agrees with ECS membership.
2. Every zoned game entity occurs exactly once in the authoritative zone index.
3. Gameplay ordering never uses Bevy query order or raw `Entity` values.
4. Pre-check trigger seeds are fixed at their ruleset timing; queue-time candidate membership and order cannot change after capture.
5. Only the iterative driver pops one-shot operations from the LIFO stack.
6. Resolution operations and prepared events are resource-owned values, never gameplay entities.
7. Canonical snapshots, traces, and checkpoints contain logical IDs rather than raw Bevy entity references.
8. Idle and complete simulations have no pending operations, events, slots, or choice.
9. A restored checkpoint reproduces durable ECS state, resolution work, RNG state, trace, logical counters, normalized turn grants, native keywords, and temporary durations.
10. Every initialized player has exactly one structurally valid active Hero and one active Hero Power in Play; replacing either uses a new stable logical identity.
11. The seven-slot board row is independent from Hero, Weapon, and Hero Power Battlefield membership.
12. Death Event order is globally sorted by play order and stable ID, independently of death-cache creation order.

This matrix grows alongside implementation. “Implemented” requires a focused test; merely defining a type does not satisfy a rule.

The action-contract rows describe engine policy, not complete official-card rulebook legality.
Targeting filters audience, Minion/Hero/Character kind, and enemy Stealth/Immune; attacks also
respect unsuppressed Minion Taunt. Remaining keyword combat rules, combat redirection, full phase guards, and action sequences for
Weapons, Hero cards, and locations remain Milestone 8 work. Hero Power activation is implemented
for the synthetic targeting contract; summon-only full-board restrictions, variable usage limits,
and game-wide usage counters remain gaps. `OriginalPowerCompletion` updates the activating power
even after replacement removes it from Play, and leaves the new power ready. Completion precedes
an ordinary boundary, then captured after-use reactions, another boundary, and outcome checking.
These completion and boundary choices are explicit engine policy, not verified pinned-rulebook
conformance. Checkpoint schema 13 is
the only accepted action-contract checkpoint schema; older schema versions, including 7, 8, 9, 10, 11, and 12,
are rejected.

For `AdvancedRulebook2026_06_26`, Hero replacement follows the dedicated “Replacing your hero” section and removes attached temporary enchantments. The contradictory sentence in the Hero-card player-action section is not generalized into the replacement reducer; full Hero-card sequencing remains a Milestone 8 gap.

## Taunt and Stealth evidence

Declaration restrictions and Taunt suppression follow the current [Target](https://hearthstone.wiki.gg/wiki/Target),
[Taunt](https://hearthstone.wiki.gg/wiki/Taunt?section=4), and
[Immune](https://hearthstone.wiki.gg/wiki/Immune) references. The
[Stealth reference](https://hearthstone.wiki.gg/wiki/Stealth?section=3) places consumption after
combat preparation regardless of successful damage. The pinned revision could not be retrieved.

The implemented `BreakAttackStealth` step runs after the existing Attack-event reactions and before
the prebuilt damage batch. This placement is engine policy within simplified combat, not certified
full preparation-phase conformance. Ordered removal enchantments preserve consumption through
recalculation and checkpoints and allow later grants. Fixtures cover declarations, Taunt suppression,
silence, expiration, copying, transformation, movement, and suspended continuation.

## Windfury evidence and scope

The current [Windfury reference](https://hearthstone.wiki.gg/wiki/Windfury) describes a dynamic
allowance of two attacks based on attacks already made that turn. The engine supports this for
Heroes and Minions through innate and enchantment-granted keywords, including mid-turn gain,
loss, regranting, and non-stacking grants. Readiness blocking remains independent; gaining
Windfury cannot ready a newly summoned or copied Minion. Snapshot exhaustion and declaration
validation share the same live calculation. Schema 12 preserves both readiness and attacks spent.

The existing FinishAttack timing remains engine policy: usage increments before AfterAttack
reactions, and keyword changes affect the next declaration. Exact pinned-rulebook timing remains
unverified. This slice does not implement Charge/Rush, Frozen, Mega-Windfury, keyword auras,
combat redirection, or full phase guards. Milestone 8 remains incomplete.
