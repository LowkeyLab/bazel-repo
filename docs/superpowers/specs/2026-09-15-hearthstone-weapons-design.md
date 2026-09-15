# Ordinary weapons: design and implementation plan

Status: implemented and verified on current main on 2026-09-15. Scope agreed on 2026-09-15: weapon play effects, Hero combat, durability, replacement, and Deathrattles. Weapon-granted keywords are deferred.

## Outcome

A player can play a synthetic 3-Attack, 2-durability weapon, attack with their Hero on two turns, and observe the weapon break after the second attack. Replacing it runs the old weapon's Deathrattle. Suspended effects resume identically through JSON restoration and fork.

## Repository starting point (before implementation)

- `core/model.rs` has `Card::weapon(name, mana_cost, attack)` but no durability field.
- `simulator/simulation_action_validation.rs` rejects weapon plays.
- `simulator/zone.rs` enforces a weapon limit separately from the Minion row.
- `simulator/simulation_effect_executor.rs::replace_hero` can install weapons, but moves the old weapon directly to Graveyard without ordinary weapon death processing.
- `simulator/death.rs` collects ordinary deaths only for Minions and Locations. Health-based mortality must not be applied to weapons with zero Health.
- `simulator/simulation_action.rs::attack` captures damage before Attack reactions. It needs a deferred combat preparation step for live weapon Attack.
- Checkpoint schema is currently 15. The new durable state requires a version bump.

## Model and API

Change the synthetic constructor to `Card::weapon(name, mana_cost, attack, durability)` and update every repository caller explicitly. Add a distinct durability field to card definitions and a serializable weapon component containing base and current durability. Weapons retain Attack stats; durability must not reuse Health or Damage.

Expose current durability on object snapshots and the active weapon ID on player snapshots. Store an explicit active weapon reference per player: zone membership alone is insufficient during replacement. Preserve weapon state and references in checkpoints; reject malformed kind/controller/zone references.

Define weapon creation with nonpositive base durability as invalid before action mutation. Zero current durability is valid transient resolution state awaiting death processing. Equipping never resets Hero readiness or attacks spent. The initial scope does not add durability enchantments or arbitrary weapon-keyword transfer.

## Weapon play and replacement

Reuse `PlayCard`, all four existing target requirements, captured targets, cost payment, canonical legal-action ordering, and pre-mutation validation. Reject board positions and nonempty declaration choices. Weapons can be played with a full Minion row or an occupied weapon slot.

Implemented sequence:

1. Capture card, previous active weapon, target, program, and initial AfterPlay trigger seeds. Pay cost and move the new weapon from Hand into Play with fresh play order.
2. Resolve CardPlayed reactions.
3. Run the new weapon's play effects with ordinary, non-spell effect origin.
4. Complete equipping and resolve a dedicated WeaponEquipped event for eligible observers.
5. Retire the captured previous weapon through weapon destruction bookkeeping and resolve the ordinary boundary, including Deathrattles in global play order.
6. Resolve captured AfterPlay reactions, then the final boundary and outcome check.

The old weapon remains eligible as an in-Play effect source during the new weapon's play effects. Allow temporary overlapping weapon membership only under a durable replacement scope, not by increasing the general weapon limit. At idle there is at most one active weapon and no unfinished replacement scope.

The scope owns stable old/new IDs and its completion authority. Nested replacement must not let an outer completion restore a superseded weapon or destroy a newer weapon by looking up the current slot. For the initial engine policy, the new weapon becomes active when it enters Play; the old one remains an observer until retirement. A nested equip supersedes the outer active reference. Completion retires only its captured predecessor if still eligible. Capture and test these references at every suspension point.

Source loss can skip source-dependent work, but must not discard scope cleanup, death processing, or unrelated final sequence work. Installing a weapon through Hero replacement uses shared equip/destruction primitives without replaying the weapon's hand-play effects or CardPlayed event.

## Combat contract

Introduce a deferred damage-preparation step after Attack reactions and Stealth consumption. Reuse a single effective-Attack query for declaration validation, snapshots, and combat:

- Minions use their current Attack.
- Heroes use their own current Attack plus the active weapon's current Attack on the controller's turn.
- Off-turn weapon Attack contributes zero; independent Hero Attack is preserved.

Do not cache weapon Attack into Hero base stats. This avoids stale values and double counting after recalculation or turn changes.

Implemented `WeaponAtDamagePreparation` policy:

1. After Attack reactions, preserve the preparation death/aura boundary and outcome check from main. Require both accepted subjects to remain in Play. Preserve the existing SurvivingCombatSubjects policy across control changes; do not retarget or repeat declaration checks.
2. Read live damage values and capture the attacker's active weapon ID at this point.
3. Resolve the existing simultaneous combat damage batch.
4. Record attack usage and consume one durability from that captured weapon if it is still the attacker's active, in-Play weapon. A weapon equipped later in damage reactions is not charged for the earlier weapon's combat.
5. Resolve AfterAttack reactions before the ordinary death boundary. Zero-durability weapons remain pending until that boundary; weapon and Minion deaths share global play-order processing.

If step 1 aborts, skip damage, durability loss, and AfterAttack. Do not spend the cancelled attack, following the existing SurvivingCombatSubjects engine policy and retain ordinary final cleanup. Add a skip trace. This extends existing guard behavior and needs regression coverage for non-weapon combat.

Damage prevention alone does not refund durability. A Hero attack without a weapon consumes none. Replacing a weapon does not grant another attack. Weapon keywords do not confer Windfury, Lifesteal, Poisonous, or other Hero abilities in this slice.

These timing and abortion choices are explicit engine policies, not certified pinned-rulebook conformance.

## Death and lifecycle

Add weapons to death collection using current durability or PendingDestroy, independently of Health. Replacement must create the same death records, trace entries, trigger seeds, and Deathrattle opportunities as breakage, while retaining remembered source attachments until ordinary death policies release them.

Keep full-zone generation behavior separate from ordinary equipping. Audit existing movement, copy, transform, and Hero replacement paths so they cannot create stale active references or weapon entities missing durability. Backward movement restores base durability; an in-Play copy preserves current durability only where the existing copy contract admits a valid destination. Do not silently turn generic generation into a weapon-play declaration.

## Persistence

Bump the checkpoint schema from 15 to 16. Persist base/current durability, active weapon references, replacement scopes, deferred damage preparation, and captured durability payer IDs. Validate scope ownership and operation references, permitting moved-but-existing captured IDs where continuation policy allows them.

Snapshots and legal action enumeration remain pure. Interrupted replacement is a valid checkpoint state; arbitrary extra weapons without an owning scope are invalid. At sequence completion, reject leftover scopes using the existing resolution-state failure policy.

## Acceptance coverage

- Equip from Hand, pay once, enumerate canonically; reject wrong owner/zone, insufficient mana, invalid target/position/choice, and invalid durability atomically.
- Equip on a full Minion board; replace an existing weapon without increasing the general capacity limit.
- Play effects observe the old weapon; AfterPlay observes completed replacement; weapon-generated damage does not receive Spell Damage.
- Two completed Hero attacks consume two durability, with normal turn refresh and no extra attacks from replacement.
- Weapon Attack adds to independent Hero Attack and disappears off-turn or after destruction.
- Attack reactions change weapon Attack or replace/remove the weapon before damage preparation.
- Damage reactions replace the weapon after preparation; the new weapon loses no durability for the old attack.
- Participant removal aborts consistently; prevented damage still consumes durability.
- Breakage and replacement each run Deathrattles once; simultaneous lethal Minion and weapon deaths use global play order.
- Nested equip, source loss, and Hero replacement cannot resurrect stale active references or discard cleanup.
- Suspended play effects, replacement Deathrattles, Attack reactions, and damage reactions restore/fork to identical complete checkpoints and traces.
- Reject stale weapon IDs, invalid ownership, missing weapon state, orphaned replacement scopes, and older schemas.

## Implementation tasks

1. Add model/runtime/snapshot/checkpoint fields and update weapon fixtures. Relevant files: `core/model.rs`, `core/entity.rs`, `core/snapshot.rs`, `core/checkpoint.rs`, and simulator card runtime, snapshot, and checkpoint modules.
2. Add scoped equip and destruction primitives; integrate zone invariants, death collection, movement/copy/transform, and Hero replacement.
3. Add weapon declaration validation, legal-action enumeration, equip events, and durable play sequence steps in the action/resolver modules.
4. Add shared effective-Attack calculation, deferred damage preparation, durability completion, and explicit abort handling.
5. Add acceptance tests to existing action, Hero, movement, event, and API suites; create a dedicated weapon suite if needed for readability.
6. Update README, IMPLEMENTATION_PROGRESS, and RULEBOOK_CONFORMANCE only as implementation lands. Leave Milestone 8 incomplete.
7. Run `bazel run //:gazelle` immediately after every source-edit batch and before manual BUILD changes or formatting. Run `aspect format --scope=all`, `aspect test //hearthstone_simulator/...`, `aspect build //...`, and scoped lint. Record actual results.

## Rulebook evidence and limitations

The pinned [advanced rulebook revision 913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067) could not be fetched during this design pass. Search excerpts from the [advanced rulebook](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?section=66) describe overlapping weapon presence during play effects, subsequent old-weapon destruction, and combat durability loss sharing a phase with damage and AfterAttack. They also describe shared play-order death processing. These support the lifecycle shape but do not verify the selected active-slot, nested-replacement, captured-payer, or aborted-attack policies. Do not label those policies rulebook-conformant without stronger evidence.

## Implementation notes

Schema 16 stores equipment as a resource with active IDs and replacement scopes keyed by incoming
weapon ID. Completion authority is checked against the durable operation stack during validation.
Replacement moves the predecessor to Graveyard and records its death before the ordinary boundary;
that boundary resolves its Death Event alongside other pending deaths. Existing transformations
remain Minion-only. Hero replacement emits WeaponEquipped but does not replay hand-play effects.
Sequence failure retires superseded weapons and releases scopes without resolving further triggers,
consistent with the existing non-transactional failure policy.

## Verification

Gazelle and repository formatting passed. Simulator tests passed: 9 core tests and 326 simulator
tests, including 17 new weapon regressions. The full repository build passed all 351 targets.
Scoped lint passed with no filtered findings, and `git diff --check` passed.

## Integration with current main

The branch incorporates combat-reaction support merged in #1747. It extends the existing
PrepareCombatDamage step with weapon Attack and durability payment, preserves preparation
deaths/auras and cancelled-attack usage behavior, and advances checkpoints from schema 15 to 16.

## Review hardening

Replacement scopes must connect weapons with the same controller. Current durability is bounded
by zero and base durability, and durability-payment steps require a typed weapon even after it
leaves Play. Combat Attack and durability payment use the same live Hero/controller/turn query.
Equip, replacement retirement, and failure retirement emit their zone transitions. Retirement
uses ordinary death movement to reset runtime state and release ordinary attachments; failure
cleanup does not schedule Death Events or other effects.
