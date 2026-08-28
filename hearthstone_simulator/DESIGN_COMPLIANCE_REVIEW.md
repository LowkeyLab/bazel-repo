# Hearthstone Simulator Design Compliance Review

## Verdict

The flat-entity migration is aligned with [`DESIGN.md`](DESIGN.md) after the corrections below. Target Milestones 0 and 1 now have the required canonical baseline, complete snapshot keyword state, and stable-boundary runtime-shape enforcement.

## Resolution

- `GameObjectSnapshot` records runtime keyword markers in deterministic `BTreeSet` order, with insertion/removal and Divine Shield continuation regressions.
- Checked-in canonical snapshot and ordered trace fixtures lock a nested-trigger sequence and quiescent boundary independently from replay-based fork tests.
- Runtime-shape validation is bidirectional, rejects conflicting forms and missing form-specific requirements, and runs after setup, every action sequence, and fork reconstruction.

## Findings

### Resolved: High — Runtime keyword state is absent from canonical snapshots

**Evidence**

- `core/model.rs:16` adds keywords to card data, and `core/entity.rs:87-118` materializes them as durable runtime marker components.
- Spawn, transformation, copying, silence, and Divine Shield consumption now mutate those markers (`core/simulation_card_runtime.rs:139`, `core/simulation_effect_executor.rs:261-322`, and `core/simulation_health.rs:123-128`).
- `GameObjectSnapshot` and `build_snapshot` do not capture any keyword state (`core/simulation_snapshot.rs:34-47,99-151`).
- `DESIGN.md:641-647` requires a full snapshot to contain all durable game-object and derived state needed for exact continuation.

**Impact**

Two worlds can have identical canonical snapshots while having different continuations. For example, consuming Divine Shield or silencing a minion changes future damage behavior without changing its snapshot. Snapshot equality therefore cannot establish clone or migration equivalence, and the new migration fixture is blind to marker-state regressions.

**Required correction**

Add a deterministic keyword representation to `GameObjectSnapshot` and populate it from runtime markers. Add focused tests proving that marker insertion/removal changes the snapshot and that equal snapshots imply equal keyword-dependent continuation behavior.

### Resolved: High — The migration fixture does not preserve a baseline snapshot or trace

**Evidence**

- `migration_fixture_preserves_snapshot_trace_and_continuation_equivalence` compares a simulation with `Simulation::fork()` before and after identical input (`core/simulation_tests_api.rs:26-68`).
- `Simulation::fork()` reconstructs and replays the same currently compiled implementation (`core/simulation.rs:178-195`).
- No expected baseline snapshot or ordered trace is asserted; only one final health value is fixed.
- `IMPLEMENTATION_PROGRESS.md:19-23` nevertheless marks the migration-equivalence fixture contract complete, while `DESIGN.md:899-901` requires preserved canonical fixtures and old/new equivalence during migration.

**Impact**

After the Observer migration, both sides of this test will execute the new implementation. A deterministic but behaviorally incompatible snapshot or trace change will still pass. The test validates replay consistency, not equivalence with the frozen-queue/resolution-node baseline it is intended to protect.

**Required correction**

Record checked-in expected canonical snapshots and ordered traces for representative baseline sequences, or provide a dual-run harness that compares the legacy and replacement engines. Include nested trigger ordering and quiescence in addition to final health. Keep the current fork test as a separate replay/continuation test.

### Resolved: Medium — Runtime-shape validation is incomplete and is not enforced at sequence boundaries

**Evidence**

- `assert_runtime_shape_invariants` checks only `EntityKind::Hero/Minion -> form marker` and the shared `StatBearing` requirements (`core/entity.rs:212-237`).
- It does not check the reverse direction, so `HeroForm` on a non-Hero or `MinionForm` on a non-minion passes. It also does not verify the `Armor` required by `HeroForm`.
- The normal post-action path checks zone and logical-index invariants but not runtime shapes (`core/simulation_action.rs:196-198`). Runtime shapes are checked only when a caller explicitly invokes `Simulation::assert_invariants`.
- `RULEBOOK_CONFORMANCE.md:80` states that every Hero/minion kind agrees with its form marker and required stat components are present; `IMPLEMENTATION_PROGRESS.md:26` marks that work complete.

**Impact**

Form drift introduced by a reducer can survive a completed action and remain undetected. The current focused test covers only a minion kind missing its marker, so inverse mismatches and missing Hero requirements can regress without failing.

**Required correction**

Validate form/kind agreement bidirectionally, reject both form markers on one entity, and check form-specific requirements such as Hero armor. Run the shape checker after setup, every completed action/sequence, and fork reconstruction, then add focused inverse-drift and missing-required-component tests.

## Compliant aspects

- `ComputedStats` becomes the concrete component while preserving a compatibility alias.
- Hero/minion form components use Bevy required components only for structural defaults; contextual identity, controller, and zone data remain in spawn paths.
- Runtime keywords are represented by individual marker components and are rebuilt explicitly by spawn, transformation, copying, and silence paths.
- Ordinary movement and transformation continue to preserve the logical and Bevy entity identity.
- Existing Hearthstone simulator tests pass: `aspect test //hearthstone_simulator/...`.
