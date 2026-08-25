# Hearthstone Simulator Implementation Progress

This document is the live implementation record for [`DESIGN.md`](DESIGN.md). It must be updated whenever a milestone changes state. A checked item means code and focused tests exist; it does not claim complete coverage of every official card.

## Current status

- Ruleset: `AdvancedRulebook2026_06_26`
- Reference: Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26)
- Active milestone: Final documentation and repository verification; advanced milestones remain explicitly open
- Verification: formatting, all Hearthstone targets, and the full repository build pass

## Milestones

- [x] **0 — Rules contract and conformance skeleton**
  - [x] Pin and name the ruleset profile.
  - [x] Add a conformance matrix and behavior classifications.
  - [x] Add fixture helpers and invariant coverage.
- [x] **1 — Entity and zone foundation**
  - [x] Stable immutable game IDs, required `GameObject`, and hook-maintained lookup index.
  - [x] Authoritative ordered zone indexes.
  - [x] Persistent identity when a card moves from Hand to Play.
- [x] **2 — Bevy resolution graph**
  - [x] Relationship-backed resolution nodes and remappable active cursor.
  - [x] Strict custom schedules and iterative budget accounting.
  - [x] Push, suspend, resume, cleanup, and graph invariants.
- [x] **3 — Event and trigger queues**
  - [x] ECS queue entries, explicit ordering, frozen membership, and cursor.
  - [x] Pre-check, queue-time, and resolution-time condition data/evaluation.
  - [x] Queue immutability, non-advancement while a child resolves, and depth-first tests.
- [ ] **4 — Effects and deterministic randomness**
  - [x] Cloneable selector/value/effect IR and depth-first effect frames.
  - [x] Versioned SplitMix64 RNG with sorted candidates and canonical trace entries.
  - [x] Damage, healing, destroy, draw, resource, summon, and sequence primitives.
  - [ ] Native-effect registry and full rulebook event/trigger integration.
- [ ] **5 — Stats, enchantments, and auras**
  - [x] Base/current stats, damage, armor, keywords, enchantment relationships, and aura-cache data.
  - [x] Effect-driven stat attachment, silence removal, and deterministic stat recalculation.
  - [ ] Scheduled aura-provider discovery and continuous Spell Damage exceptions.
- [ ] **6 — Damage, healing, and death (vertical slice)**
  - [x] Armor, Immune, Divine Shield, mortality, pending destroy, and ordered boundary removal.
  - [x] Synthetic area-damage test proving deaths are collected before boundary removal.
  - [ ] Proposed/pre-damage trigger pipeline, death records, Deathrattles, and chained Death Phases.
- [ ] **7 — General mechanics**
  - [x] Drawing, burning, fatigue, signed resources, temporary resources, and Overload counters.
  - [x] Data-driven transformation and copying primitives with stable/generated identities.
  - [ ] Complete movement reset policies, hero replacement, and extra turns.
- [ ] **8 — Player-action sequences**
  - [x] Stable-ID minion/spell play, combat, end-turn, and concede actions.
  - [x] Deterministic legal actions and captured declared targets.
  - [ ] Weapon, Hero card, location, Hero Power, redirection, and full subject-guard sequences.
- [ ] **9 — Esoteric compatibility**
  - [ ] Named forced-death, dominant-player, Deathrattle-position, and wounded-target policies.
- [ ] **10 — Hardening and migration**
  - [x] Canonical logical-ID snapshots/traces, migrated CLI, invariants, and replay-equivalent `fork`.
  - [ ] True suspended-choice persistence, filtered snapshots, stress benchmarks, and full conformance coverage.

## Verification log

| Date       | Command                                                                        | Result                                      |
| ---------- | ------------------------------------------------------------------------------ | ------------------------------------------- |
| 2026-08-25 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed after milestones 0–3                 |
| 2026-08-25 | `aspect test //hearthstone_simulator/core:core_test`                           | Passed effect/RNG/area-death vertical slice |
| 2026-08-25 | `aspect test //hearthstone_simulator/...`                                      | Passed core tests and CLI build             |
| 2026-08-25 | `aspect format --scope=all`                                                    | Passed                                      |
| 2026-08-25 | `aspect build //...`                                                           | Passed full repository build                |
| 2026-08-25 | `bazel coverage //hearthstone_simulator/core:core_test --combined_report=lcov` | 100% lines and functions                    |

## Known gaps

The implementation remains intentionally synthetic-card-first. Unchecked items above are not implemented and must not be inferred from the architectural types alone. In particular, the current effect reducers create inspectable effect frames but damage/healing do not yet compile the complete proposed-event and trigger pipeline.
