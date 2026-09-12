# Hearthstone Action-Contract Foundation

## Status and purpose

The user approved this design in conversation on 2026-09-11. This document records the first stage of Milestone 8 for review before implementation planning. It does not mark Milestone 8 complete.

Establish a single declaration-time contract for existing minion plays, spell plays, attacks, end-turn actions, and concession. Legal-action enumeration must agree with validation, targets must be explicit card data, and pending sequence steps must support serializable subject guards.

This is an engine API foundation under the existing `AdvancedRulebook2026_06_26` profile. It does not certify complete Hearthstone targeting or combat conformance.

## Current behavior and motivation

`core/action.rs` exposes `PlayCard`, `Attack`, `EndTurn`, and `Concede`. `PlayCard` includes optional target, board position, and choice fields.

`simulator/simulation_action.rs` currently:

- Enumerates EndTurn and affordable hand cards, with no target or position alternatives and no attacks.
- Validates minion/spell kind, resources, board capacity, and movement position, but ignores declared target and choice.
- Validates attacks using readiness, positive attack, zone, and opposing control, without restricting both subjects to characters.
- Captures the declared target in `EffectContext` and queues one-shot operations on the existing LIFO resolver.

`Card`, `CardDefinition`, and `CardRuntime` have effect programs but no targeting requirement. Effects can consume `Selector::DeclaredTarget`, including inside nested or native programs. Inferring action requirements from those programs would be ambiguous.

## Selected architecture

Use explicit targeting metadata and one read-only action validation path. Enumeration generates candidates and passes them through that same path. Validated declarations compile into existing one-shot sequence operations.

Alternatives considered:

- Infer targeting from effects: fewer fields, but cannot reliably distinguish mandatory, optional, and conditional declarations, especially for native effects.
- Separate validators per caller: locally simple, but allows submitted actions and enumerated actions to disagree.

Do not introduce a general predicate language, transaction framework, or replacement resolver.

## Targeting data

Add serializable targeting metadata to card data with these requirement variants:

| Requirement                 | No eligible targets   | Eligible targets exist               |
| --------------------------- | --------------------- | ------------------------------------ |
| None                        | Only absent target    | Only absent target                   |
| Required(filter)            | Card cannot be played | One eligible target required         |
| Optional(filter)            | Absent target allowed | Absent target or one eligible target |
| RequiredIfAvailable(filter) | Absent target allowed | One eligible target required         |

A supplied target must always exist and satisfy its filter. Supplying a target for None is rejected.

Initial filters combine:

- Audience: friendly, enemy, or either, relative to the declaring player.
- Kind: minion, Hero, or character (minion or Hero).
- Zone: Play, fixed for this foundation.

These filters describe declaration candidates, not effect selectors. They do not contain random selection, effect execution, arbitrary expressions, or native callbacks. Target enumeration must never consume RNG.

Card constructors default to None. Add a builder for explicit targeting. Migrate existing fixtures that submit a target to declare their intended requirement; do not infer defaults from their effects. A declared-target selector can still evaluate to no entity in a context with no declared target, as required by optional targeting.

Metadata follows the current card form through Card-to-definition conversion, spawn, runtime state, copying, transformation, and movement reset. Copies receive the source form's metadata; transformation receives the replacement form's metadata; rebuilding an innate form uses that definition's metadata. Effective-cost modifiers do not affect targeting metadata.

## Shared declaration validation

Keep validation read-only and return structured SimulationError variants. Validate before spending resources, allocating resolution IDs, changing status, or queuing sequence work.

Validation covers:

1. Existing game-status, active-player, ownership, and zone requirements.
2. Supported action/card kinds and existing effect-program structural validation.
3. Existing resource and board-capacity rules, including flooring negative cost at payment.
4. Target requirement and filter.
5. Position and choice inputs.
6. Existing attack readiness and positive-attack checks, with attacker and defender restricted to in-Play Heroes or minions and defender controlled by the opponent.

All nonempty PlayCard.choice inputs are rejected as unsupported. Choice suspension and submit_choice remain supported through their existing API; this work does not add declaration-time modal card choices.

For minions, accept explicit insertion positions from zero through the current board-row length. Preserve board_index=None as an append alias. For spells, require board_index=None; the public action must not expose arbitrary Graveyard insertion positions.

Preserve current active-player-only EndTurn and Concede validation. Changing concession availability outside the active player's action window is separate work.

Errors distinguish missing target, unexpected target, invalid target, unsupported declaration choice, and inappropriate position input. Existing resource, ownership, and status errors remain usable.

A rejected declaration preserves gameplay state, RNG, counters, and pending resolution work. The existing ActionRejected trace entry is permitted. This is not a promise to roll back failures arising after an accepted action starts executing effects.

## Deterministic legal actions

Return an empty list unless the game is awaiting an action and has no outcome. Otherwise enumerate canonical actions for the active player in this order:

1. EndTurn.
2. PlayCard, ordered by ascending stable card ID, then absent target before ascending target IDs, then ascending explicit board position.
3. Attack, ordered by ascending attacker ID and then defender ID.
4. Concede.

Only minion and spell plays are included. Unsupported hand card kinds are excluded. Enumerate every target allowed by the requirement table. Enumerate every legal explicit position for each minion target combination. Spells use no position. All generated actions use choice=None.

Every candidate must pass shared validation. Avoid duplicating resource, capacity, target, or readiness policy in the enumerator. Candidate generation may narrow by structural facts such as zone and kind.

Do not emit duplicate append actions: enumeration uses the explicit final insertion index; submitted minion board_index=None remains its accepted alias. Thus completeness means every supported valid declaration has a canonical equivalent in the list, not that all syntactic aliases appear.

This list is complete for the supported foundation contract. Keyword-specific targeting and combat legality remain explicit gaps; this stage does not silently claim Taunt, Stealth, Immune, Frozen, Rush, or Windfury conformance.

## Captured declarations and subject guards

Capture player, source/subject IDs, declared target, and normalized placement when the declaration is accepted. Do not rerun declaration targeting filters after triggers execute, silently substitute another target, or choose a new target with RNG. Effects retain their own applicability behavior; capture does not promise that every later effect succeeds.

Introduce serializable guard data for deferred sequence steps. The initial guard checks a stable subject ID and required zone. Multiple guards on one step are evaluated in declared order; the first failure skips that step and emits one canonical guard-skip trace record identifying the subject, step, expected zone, and observed missing/wrong-zone result.

Guard evaluation occurs when the step is popped, after earlier nested work completes. Skipping consumes the operation exactly once, adds no child work, and does not discard unrelated pending operations. Ordinary boundaries, cleanup, and outcome checks remain outside the guarded step.

Initial integration is deliberately narrow:

- The PlayCard sequence entry requires the card to remain in Hand.
- The Attack sequence entry requires attacker and defender to remain in Play.
- Deferred synthetic fixtures exercise an intervening removal before a guarded step.

Do not add guards to FinishAttack or played-self-transform completion merely because they reference an entity. Do not guard an entire spell program on its source remaining in Play: spells currently move to Graveyard before their effects execute. Full phase-by-phase guard placement belongs with the subsequent player-sequence work.

Do not add controller, card-form, or zone-entry-generation guard fields until an implemented step needs those semantics. For this foundation, leaving and returning to the required zone before the check passes a zone guard; transformation retaining the same stable ID and zone also passes. This explicitly defines the narrow mechanism without claiming complete subject continuity rules.

## Persistence and traces

Persist targeting metadata in runtime checkpoints and guards in pending resolution operations. Bump CHECKPOINT_SCHEMA_VERSION from its current value of 7 when implementing the schema change; reject older versions rather than silently losing the new contract.

Restoration validates guard payloads and logical references using existing checkpoint conventions. An existing entity in a different zone is valid checkpoint state and fails its guard only at execution. Do not confuse a moved subject with a corrupt reference.

Keep accepted targets unchanged through suspended choices, JSON round trips, and fork. Add the guard-skip trace record. Enumeration itself changes neither state nor trace.

## Expected files and boundaries

Core contracts:

- `core/action.rs`: targeting/guard contract types or focused adjacent modules.
- `core/model.rs` and `core/card_definition.rs`: card metadata and builder.
- `core/error.rs`: declaration errors.
- `core/resolver.rs`: guarded sequence operation payloads.
- `core/checkpoint.rs` and `core/trace.rs`: persistence and diagnostics.

Simulator implementation:

- `simulator/simulation_action.rs`: shared validation, candidate enumeration, capture, and compilation.
- `simulator/simulation_card_runtime.rs`: runtime metadata.
- `simulator/simulation_effect_executor.rs` and `simulator/zone.rs`: copy, transform, and reset propagation.
- `simulator/simulation_event_resolver.rs`: guard dispatch if operations are dispatched there.
- `simulator/simulation_checkpoint.rs`: encoding, restoration, and validation.
- Focused action, movement, effect, and checkpoint tests; split a targeting test module only if needed for readability.

Update public exports and existing explicit card/runtime initializers as required. Gazelle owns BUILD metadata updates after source edits.

Update README, IMPLEMENTATION_PROGRESS, and RULEBOOK_CONFORMANCE to describe this foundation and retain remaining Milestone 8 gaps.

## Verification and acceptance criteria

Meaningful behavioral tests must cover:

- Every requirement-table branch, invalid IDs, wrong zone, wrong audience, wrong kind, and unexpected targets.
- Full boards, unaffordable and negative-cost cards, unsupported hand kinds, spell position rejection, invalid minion positions, and unsupported choice input.
- Attacks appearing in the list, with readiness, controller, zone, positive attack, and character-kind rejection cases.
- Every enumerated action passing declaration validation against the same unchanged state; successful representative actions on independent forks.
- Exhaustive small-fixture target/position/action candidates agreeing with canonical enumeration, including the append alias.
- Repeated enumeration preserving snapshot, checkpoint, RNG, and trace and returning identical ordering.
- Rejected declarations preserving gameplay and resolver state, allowing only the rejection trace.
- A captured target surviving declaration-relevant state changes without revalidation or retargeting.
- Guard success and skip after intervening movement; later siblings, boundaries, and outcome checks still running; deterministic skip traces.
- Choice suspension with guarded pending work, checkpoint JSON restoration, and fork continuation producing matching snapshots and traces.
- Targeting metadata preserved or replaced correctly across spawn, copy, transform, backward movement, and checkpoint restoration.

Implementation workflow: run `bazel run //:gazelle` immediately after source edits, then `aspect format --scope=all`, `aspect test //hearthstone_simulator/...`, and `aspect build //...`. Run repository lint before declaring implementation ready. Do not mark tests or milestone items complete without fresh results.

## Non-goals

- Weapon, Hero-card, location, or Hero Power action sequences.
- Full combat preparation, redirection, cancellation, or changed damage timing.
- Complete keyword legality, wounded-target compatibility policies, or official card coverage.
- Arbitrary target predicates, multiple declared targets, or declaration-time choices.
- New phase-specific subject lifetime policies beyond the guards defined here.
- Filtered snapshots, benchmarks, UI, or broad unrelated refactoring.

## Next step

Review this written spec, then create the implementation plan after user approval.
