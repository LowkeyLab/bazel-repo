# Hearthstone Simulator Implementation Progress

This document is the live implementation record for [`DESIGN.md`](DESIGN.md). It must be updated whenever a milestone changes state. A checked item means code and focused tests exist; it does not claim complete coverage of every official card.

## Current status

- Ruleset: `AdvancedRulebook2026_06_26`
- Reference: Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26)
- Active milestone: Milestone 7 general mechanics
- Verification: Gazelle, formatting, all Hearthstone targets, and the full repository build pass

## Milestones

- [x] **0 — Rules contract and conformance skeleton**
  - [x] Pin and name the ruleset profile.
  - [x] Add a conformance matrix and behavior classifications.
  - [x] Add fixture helpers and invariant coverage.
- [x] **1 — Entity and zone foundation**
  - [x] Stable immutable game IDs, required `GameObject`, and hook-maintained lookup index.
  - [x] Authoritative ordered zone indexes.
  - [x] Persistent identity when a card moves from Hand to Play.
- [x] **2 — LIFO resolution work stack**
  - [x] Resource-owned stack of one-shot logical `ResolutionOp` values with no frame graph or active cursor.
  - [x] Strict custom schedules, iterative exact-once execution, and per-sequence budget accounting.
  - [x] Reverse-order expansion, generic choice suspension/resumption, cleanup, and idle-work invariants.
- [x] **3 — Prepared events and trigger snapshots**
  - [x] Immutable event records, complete trigger order keys, and captured candidate membership.
  - [x] Staged pre-check seeds, queue-time candidate snapshots, and resolution-time condition evaluation.
  - [x] Reverse stack expansion, delayed event slots, abort handling, and depth-first tests.
- [x] **4 — Effects and deterministic randomness**
  - [x] Cloneable selector/value/effect IR and depth-first effect frames.
  - [x] Versioned SplitMix64 RNG with sorted candidates and canonical trace entries.
  - [x] Damage, healing, destroy, draw, resource, summon, and sequence primitives.
  - [x] Stable native-effect registry whose Bevy systems return ordinary effect plans.
  - [x] Prepared events, immutable trigger snapshots, timed conditions, and depth-first trigger effect programs without generic recursion guards.
- [x] **5 — Stats, enchantments, and auras**
  - [x] Base/current stats, damage, armor, keywords, enchantment relationships, and H1/H2-preserving stat recalculation.
  - [x] Typed Health, Attack, and Other aura caches with deterministic discovery, played/summoned timing, and post-death expiration.
  - [x] Live Spell Damage contributions from in-play and attached sources, including opponent recipients, with separate Hero Power modifiers.
- [x] **6 — Damage, healing, and death (vertical slice)**
  - [x] Armor, Immune, Divine Shield, mortality, pending destroy, and ordered boundary removal.
  - [x] Synthetic area-damage test proving deaths are collected before boundary removal.
  - [x] Immutable death records/cache (including turn of death), self-filtered Deathrattles, and chained Death Phases.
  - [x] Irreversible Hero defeat at Death Creation, global simultaneous-death ordering, per-event queue-time capture, dominant-player grouping, and same-player death-trigger mingling.
  - [x] Explicit sequence-end outcome checks after all chained Death Phases.
  - [x] Proposed damage/healing modifiers; DH1/DH2 protection, mutation, and reaction timing; and immutable simultaneous event batches.
- [ ] **7 — General mechanics**
  - [x] Drawing, burning, fatigue, signed resources, temporary resources, and Overload counters.
  - [x] Data-driven transformation and copying primitives with stable/generated identities.
  - [x] Ruleset-driven forward/backward/death movement reset policies, full-zone outcomes, and deterministic simultaneous movement.
  - [x] Ruleset-precedence extra-turn composition, including the `ABBBAAA` and `BBAAABB` examples, plus checkpointed turn-series durations.
  - [x] Temporary stat and keyword enchantments with end-of-turn and end-of-turn-series expiration.
  - [x] Reusable Hero replacement preserving Health, Armor, attack usage, weapons, class policy, and irreversible defeat timing while replacing and refreshing the in-Play Hero Power.
  - [x] Profile-owned Hand, Deck, board-row, Hero, Weapon, Hero Power, Secret, and Quest capacity policies.
  - [ ] Generalize temporary duration to remaining cost/trigger payloads and finish the still-Partial draw and transformation/copy conformance rows.
- [ ] **8 — Player-action sequences**
  - [x] Stable-ID minion/spell play, combat, end-turn, and concede actions.
  - [x] Deterministic legal actions and captured declared targets.
  - [ ] Weapon, Hero card, location, Hero Power, redirection, and full subject-guard sequences.
- [ ] **9 — Esoteric compatibility**
  - [x] Durable dominant-player identity and dominant/secondary trigger grouping.
  - [ ] Named forced-death, Deathrattle-position, and wounded-target policies.
- [ ] **10 — Hardening and migration**
  - [x] Canonical logical-ID snapshots/traces, migrated CLI, invariants, and checkpoint-exact `fork`.
  - [x] Versioned logical checkpoints, JSON serialization, restoration validation, and suspended-choice continuation.
  - [ ] Filtered snapshots, stress benchmarks, and full conformance coverage.

## Verification log

| Date       | Command                                                                        | Result                                      |
| ---------- | ------------------------------------------------------------------------------ | ------------------------------------------- |
| 2026-08-25 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed after milestones 0–3                 |
| 2026-08-25 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed effect/RNG/area-death vertical slice |
| 2026-08-25 | `aspect test //hearthstone_simulator/...`                                      | Passed core tests and CLI build             |
| 2026-08-25 | `aspect format --scope=all`                                                    | Passed                                      |
| 2026-08-25 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-25 | `bazel coverage //hearthstone_simulator/core:core_test --combined_report=lcov` | 100% lines and functions                    |
| 2026-08-26 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed event/trigger and native-effect work |
| 2026-08-26 | `aspect test //hearthstone_simulator/...`                                      | Passed core tests and CLI build             |
| 2026-08-26 | `aspect format --scope=all`                                                    | Passed                                      |
| 2026-08-26 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-26 | `aspect test //hearthstone_simulator/...`                                      | Passed chained Death Phase vertical slice   |
| 2026-08-26 | `aspect format --scope=all`                                                    | Passed                                      |
| 2026-08-26 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-26 | `aspect test //hearthstone_simulator/...`                                      | Passed death compliance regression coverage |
| 2026-08-26 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-26 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-27 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed Milestone 6 event-batch coverage     |
| 2026-08-27 | `aspect test //hearthstone_simulator/...`                                      | Passed core tests and CLI build             |
| 2026-08-27 | `bazel coverage //hearthstone_simulator/core:core_test --combined_report=lcov` | 100% changed-line coverage                  |
| 2026-08-27 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-27 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-27 | `aspect format --scope=all`                                                    | Passed DH1/DH2 compliance fixes             |
| 2026-08-27 | `aspect test //hearthstone_simulator/...`                                      | Passed all four compliance regressions      |
| 2026-08-27 | `bazel run //tools/coverage -- //hearthstone_simulator/...`                    | 100% changed-line coverage (481/481)        |
| 2026-08-27 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-27 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-30 | `bazel run //:gazelle`                                                         | Passed staged-trigger/checkpoint generation |
| 2026-08-30 | `aspect format --scope=all`                                                    | Passed repository formatting                |
| 2026-08-30 | `aspect test //hearthstone_simulator/...`                                      | Passed 57 simulator tests and CLI build     |
| 2026-08-30 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-30 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-30 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed Milestone 5 aura conformance tests   |
| 2026-08-30 | `bazel run //tools/coverage -- //hearthstone_simulator/...`                    | 99.5% lines and 98.7% functions             |
| 2026-08-30 | `aspect format --scope=all`                                                    | Passed repository formatting                |
| 2026-08-30 | `aspect test //hearthstone_simulator/...`                                      | Passed aura-enabled simulator and CLI       |
| 2026-08-30 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-30 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-30 | `aspect test //hearthstone_simulator/...`                                      | Passed aura compliance suite and CLI        |
| 2026-08-30 | `bazel run //tools/coverage -- //hearthstone_simulator/...`                    | 99.2% lines and 98.8% functions             |
| 2026-08-30 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-30 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-30 | `bazel run //:gazelle`                                                         | Passed Milestone 7 mechanics generation     |
| 2026-08-30 | `aspect format --scope=all`                                                    | Passed repository formatting                |
| 2026-08-30 | `aspect test //hearthstone_simulator/...`                                      | Passed 103 simulator tests and CLI build    |
| 2026-08-30 | `bazel run //tools/coverage -- //hearthstone_simulator/...`                    | 98.5% lines and 98.7% functions             |
| 2026-08-30 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-30 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-30 | `bazel run //:gazelle`                                                         | Passed compliance remediation generation    |
| 2026-08-30 | `aspect format --scope=all`                                                    | Passed repository formatting                |
| 2026-08-30 | `aspect test //hearthstone_simulator/...`                                      | Passed 119 simulator tests and CLI build    |
| 2026-08-30 | `bazel run //tools/coverage -- //hearthstone_simulator/...`                    | 99.0% lines and 98.9% functions             |
| 2026-08-30 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-30 | `aspect build //...`                                                           | Passed full repository build                |

## Known gaps

The implementation remains intentionally synthetic-card-first. Unchecked items above are not implemented and must not be inferred from the architectural types alone. Zone movement uses the profile's Deck → Hand → Play → Graveyard direction, preserves state forward, rebuilds innate stats and keywords while removing ordinary attachments backward, supports explicit keep-enchantment exceptions, and treats full-zone movement separately from generation and force play. Battlefield membership is distinct from the seven-slot board row; active Heroes, Weapons, and Hero Powers use independent profile limits. Full-zone and ordinary Death Events are globally ordered by play order while the death cache retains creation order. Extra turns use ruleset precedence rather than effect FIFO order, while temporary stat and keyword enchantments distinguish one turn from a contiguous player turn series. Cost- and trigger-bearing temporary payloads remain a Milestone 7 gap. Hero replacement removes the old Hero and its attachments, preserves or explicitly replaces Health, keeps Armor and attack usage, applies weapon/class policy, replaces the in-Play Hero Power with an immediately usable entity, waits for the ordinary phase boundary before refreshing auras, and never clears a defeat already locked at Death Creation. Runtime and checkpoint validation require exactly one structurally valid active Hero and Hero Power per initialized player. The profile follows the dedicated “Replacing your hero” rule for temporary-enchantment removal; the contradictory Hero-card-sequence wording remains confined to the still-unimplemented full Hero-card action sequence. Maximum-Health recalculation preserves H1/H2; Health applications persist through the immediate Death Phase while Attack and Other applications from removed providers expire first. Played providers merge only their own applications, summons refresh all categories, and Spell Damage is queried from live in-play or attached contributions for each spell-originated damage effect. Damage and healing process each proposed event and durable mutation in event order, fill pre-positioned actual-event slots, and delay actual reactions until every simultaneous mutation completes. Damage prevention precedes predamage triggers, no-op health changes do not create actual events, and Armor loss counts as actual damage. Boundary-created death records drive self-filtered Deathrattles through chained Death Phases. Death Creation locks Hero defeat, orders deaths globally by play order, and records each Death Event plus its pre-check-eligible trigger seeds. Each Death Event evaluates queue-time conditions only when it begins resolution, and the final outcome is checked after all chained Death Phases.
