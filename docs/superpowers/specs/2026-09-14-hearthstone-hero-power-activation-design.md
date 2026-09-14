# Hearthstone Hero Power Activation

## Status

The user selected Hero Power activation as the next feature after brainstorming. This document develops that selection into a Milestone 8 slice. Implementation was authorized on 2026-09-14. The sequence below is implemented as a synthetic engine foundation; replacement and phase-boundary choices are explicit engine policy, not verified pinned-rulebook conformance.

## Motivation and existing support

Before this change the engine accepted minion/spell plays, attacks, end turn, and concession. Its existing in-Play Hero Power entities, `HeroPowerState`, targeting metadata, Hero Power damage modifiers, turn-start refresh, and Hero replacement provided the foundation for the new activation declaration.

Add a complete synthetic activation path through the existing action validator and LIFO resolver. Do not introduce a second effect interpreter or infer targeting requirements from effect programs.

## API and declaration contract

Add `GameAction::UseHeroPower { player, power, target }`, where `power` is a stable `GameEntityId` and `target` is optional. Include the variant in `player()` and `label()`.

Shared read-only validation checks:

1. The game is awaiting an action, has no outcome, and the declaring player is active.
2. The declared power exists, belongs to that player, and is the active in-Play Hero Power. A hand card or replaced power cannot be activated.
3. Runtime power data and `HeroPowerState` exist, and the power is not exhausted.
4. Its effect program is structurally valid under the existing validator.
5. Available resources cover `max(effective_cost, 0)`.
6. The target satisfies the existing explicit `TargetRequirement`.

Use exhaustion as the availability gate; `uses_this_turn` records usage rather than imposing an independent limit that would prevent a later refresh mechanic. This slice does not introduce variable usage limits or refresh effects.

Return structured errors for a non-active/non-power subject and an exhausted power; retain existing status, ownership, zone, resource, and targeting errors where appropriate. Rejections preserve state, RNG, resolver counters, and pending work, except for the existing rejection trace entry.

## Legal actions

Enumerate power activations after attacks and before concession, retaining the relative order of existing actions. Generate target options through the existing targeting helper and filter every candidate through shared validation. Use stable power-ID ordering and absent-target-first, ascending target-ID ordering.

Repeated enumeration must preserve checkpoint, snapshot, RNG, and trace. Every enumerated activation must validate against the same state. Zero-effect powers remain usable if their explicit declaration contract permits it; the engine must not infer usability from whether effects will change state.

## Implemented sequence

Capture the accepted player, source power ID, and declared target. At sequence entry capture the eligible seeds for a new `EventKind::AfterHeroPower` event, using the existing trigger machinery. Keep event source equal to the original power and event targets equal to the accepted target list, including when the target subsequently moves.

One-shot operations:

1. A guarded `UseHeroPower` sequence entry requires the source to remain in Play. Clone its effect program and pay its effective cost before any effects run.
2. Execute the captured effects with `EffectOrigin::HeroPower`, the original source ID, and the accepted target. Nested work completes through the ordinary resolver; never retarget or rerun declaration filters.
3. A serializable completion step records usage and exhausts the activating power after its effects finish. It must never look up and exhaust a replacement power merely because that power now belongs to the same player.
4. Run an ordinary phase boundary, then resolve `AfterHeroPower` from the captured seeds.
5. Finish boundary processing and perform sequence-end outcome checks.

Steps 3–5 use an ordinary boundary after completion and another after after-use reactions, followed by the outcome check. This is the explicit engine policy selected while the pinned reference is unavailable; no early outcome check bypasses remaining reactions.

Store captured after-use seeds in an existing prepared event at sequence entry. Its EventCreated trace therefore precedes the effects, while candidate queueing and execution occur after completion and the boundary. A separate serializable FinishHeroPower step records exhaustion. This reuses checkpoint seed/reference validation without adding a second seed payload. At after-use queue time retain the normal source-eligibility and condition checks. A trigger created by the power's effects must not join the previously captured seed set.

Completion, after-use events, cleanup, and outcome checking must not all sit beneath a source-in-Play guard: replacing the source must not silently discard unrelated remaining sequence work.

## OriginalPowerCompletion policy

Existing `replace_hero` moves the old power to RemovedFromGame and creates a new stable power ID with default, ready `HeroPowerState`.

The implemented `OriginalPowerCompletion` policy keeps the new power ready, retains the original source on the ongoing effects and after-use event, and increments usage/exhausts the original power even after it moves to RemovedFromGame. If the original entity no longer exists, is no longer a Hero Power, or no longer has power state, completion omits the state mutation. Remaining after-use work and boundaries still execute.

This is explicit engine policy, not a verified claim about wiki revision 913067. Tests cover replacement both inside the program and inside after-use reactions.

## Persistence

Persist all new sequence steps, captured source/target IDs, and trigger seeds. Extend checkpoint reference validation to include the new payloads and reuse existing seed validation. Moved but existing source/target IDs are legitimate suspended state.

Checkpoint schema 10 includes these durable additions. Earlier schema versions are explicitly rejected. Fork and JSON restoration must preserve continuation behavior and canonical traces, including suspension before completion.

## Fixtures and acceptance

Use existing Hero replacement effects to install synthetic powers through public APIs; no new player-configuration API is necessary for this slice.

- A targeted damage power consumes resources, applies Hero Power damage modifiers rather than Spell Damage, and retains its declared target.
- An untargeted resource power demonstrates payment-before-effect ordering, post-effect exhaustion, rejection of repeated use, and turn-start refresh.
- Missing, wrong-kind, wrong-controller, wrong-zone, stale, exhausted, unaffordable, and incorrectly targeted declarations fail atomically.
- Cover zero and negative costs and all four existing target requirement modes.
- Enumeration is canonical, complete for supported declarations, and pure.
- Existing after-use observers can react; observers summoned by the activation cannot join that activation's initial seed set. Removed observers follow normal eligibility rules.
- A target removed during effects stays captured on the after-use event.
- A source replaced during effects cannot exhaust the new power or erase remaining after-use work.
- Lethal effects still follow the selected phase and outcome ordering.
- Suspended activation, JSON restoration, and fork produce identical final checkpoints and traces; corrupt references and old schemas are rejected.

## Scope boundaries

This is a synthetic activation foundation, not complete Hero Power conformance. Summon-only full-board declaration restrictions require explicit card-level playability metadata and are deferred; do not infer them by scanning nested or native effect programs. Also defer modal choices, targeting redirection, keyword-specific target restrictions, variable activation limits, game-wide usage counters, official card coverage, weapons, Hero-card plays, and locations. Generic pending-choice restoration remains supported.

## Evidence and limitations

Repository evidence:

- `core/action.rs`: existing action variants.
- `core/entity.rs`: per-power usage/exhaustion state.
- `simulator/simulation_action_validation.rs`: shared targeting and declaration validation.
- `simulator/simulation_action.rs`: action enumeration, sequence compilation, and turn-start refresh.
- `simulator/simulation_effect_executor.rs`: Hero Power damage origin and fresh replacement power state.
- `simulator/simulation_event_resolver.rs`: preparation with captured trigger seeds.
- `simulator/simulation_checkpoint.rs`: durable operation/reference validation.

The exact [pinned revision](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067) could not be retrieved during this design pass. The accessible [advanced rulebook section](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?section=55) describes effects before exhaustion and a later Inspire phase restricted by earlier eligibility. The current [Hero Power reference](https://hearthstone.wiki.gg/wiki/Hero_Power) describes payment before effects and fresh availability after replacement. These current references support the proposed shape but do not establish exact conformance to the pinned revision.
