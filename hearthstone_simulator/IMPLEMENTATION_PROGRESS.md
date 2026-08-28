# Hearthstone Simulator Implementation Progress

This document is the live implementation record for [`DESIGN.md`](DESIGN.md). It must be updated whenever a milestone changes state. A checked item means code and focused tests exist; it does not claim complete coverage of every official card.

The target redesign and the running implementation use separate milestone tracks during migration. **Target milestones** use the numbering in `DESIGN.md`. **Legacy milestones** below preserve the numbering of the frozen-queue/resolution-node design so existing status and verification history remain interpretable.

## Current status

- Ruleset: `AdvancedRulebook2026_06_26`
- Reference: Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26)
- Target architecture: deterministic immediate `EntityEvent`/Observer dispatch with layered recomputation and a phase coordinator
- Running implementation baseline: ECS resolution nodes and frozen event/trigger queues
- Active target milestone: Target Milestone 2 child enchantments and layered recomputation
- Ongoing baseline work: Legacy Milestone 5 aura timing and Legacy Milestone 7 general mechanics
- Verification: Gazelle, formatting, all Hearthstone targets, and the full repository build pass

## Target redesign migration

- [x] **Target 0 — Architecture and conformance contract**
  - [x] Document the Observer-driven target architecture and migration milestones.
  - [x] Update `RULEBOOK_CONFORMANCE.md` for immediate dispatch and the new phase model.
  - [x] Record behavior-equivalence fixtures for the current resolution graph and frozen queues.
  - [x] Check in a canonical baseline snapshot and ordered nested-trigger trace with quiescence assertions.
  - [x] Keep replay/continuation and divergent-fork isolation fixtures separate from the baseline oracle.
- [x] **Target 1 — Flat entity shapes**
  - [x] Preserve stable logical IDs through required `GameObject` insertion and hook-maintained indexes.
  - [x] Add required-component Hero/minion runtime form markers and validate shapes bidirectionally at stable boundaries.
  - [x] Materialize frequently queried binary keywords as marker components and canonical snapshot state.
  - [x] Preserve authoritative zone indexes and runtime identity across ordinary movement.
- [ ] **Target 2 — Child enchantments and layered recomputation**
- [ ] **Target 3 — Immediate event dispatch**
- [ ] **Target 4 — Phase and death schedule**
- [ ] **Target 5 — Complete mechanics**
- [ ] **Target 6 — Headless cloning and search readiness**
- [ ] **Target 7 — Hardening**

## Legacy implementation baseline (pre-Observer redesign)

The completed items in this section describe the currently running baseline, not completion of the target milestones above. They remain authoritative until equivalence coverage permits each legacy mechanism to be replaced.

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
- [x] **4 — Effects and deterministic randomness**
  - [x] Cloneable selector/value/effect IR and depth-first effect frames.
  - [x] Versioned SplitMix64 RNG with sorted candidates and canonical trace entries.
  - [x] Damage, healing, destroy, draw, resource, summon, and sequence primitives.
  - [x] Stable native-effect registry whose Bevy systems return ordinary effect plans.
  - [x] Event frames, immutable trigger queues, timed conditions, trigger guards, and depth-first trigger effect programs.
- [ ] **5 — Stats, enchantments, and auras**
  - [x] Base/current stats, damage, armor, keywords, enchantment relationships, and aura-cache data.
  - [x] Effect-driven stat attachment, silence removal, and deterministic stat recalculation.
  - [ ] Scheduled aura-provider discovery and continuous Spell Damage exceptions.
- [x] **6 — Damage, healing, and death (vertical slice)**
  - [x] Armor, Immune, Divine Shield, mortality, pending destroy, and ordered boundary removal.
  - [x] Synthetic area-damage test proving deaths are collected before boundary removal.
  - [x] Immutable death records/cache (including turn of death), self-filtered Deathrattles, and chained Death Phases.
  - [x] Irreversible Hero defeat at Death Creation, global simultaneous-death ordering, frozen Death Event batches, and play-order-mingled death triggers.
  - [x] Proposed damage/healing modifiers; DH1/DH2 protection, mutation, and reaction timing; and immutable simultaneous event batches.
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
| 2026-08-27 | `bazel run //:gazelle`                                                         | Passed Target 0/1 compliance corrections    |
| 2026-08-27 | `aspect format --scope=all`                                                    | Passed compliance corrections               |
| 2026-08-27 | `aspect test //hearthstone_simulator/...`                                      | Passed baseline/keyword/shape regressions   |
| 2026-08-27 | `aspect lint`                                                                  | Passed all repository linters               |
| 2026-08-27 | `aspect build //...`                                                           | Passed full repository build                |

## Known gaps

The implementation remains intentionally synthetic-card-first. Unchecked items above are not implemented and must not be inferred from the architectural types alone. Damage and healing now process each proposed event and durable mutation in event order, then resolve the frozen actual-event reaction queue. Damage prevention precedes predamage triggers, no-op health changes do not create actual events, and Armor loss counts as actual damage. Boundary-created death records drive self-filtered Deathrattles through chained Death Phases. Death Creation locks Hero defeat, orders deaths globally by play order, records the turn of death, and freezes every Death Event and trigger queue before batch resolution.
