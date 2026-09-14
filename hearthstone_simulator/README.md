# Hearthstone Simulator

A deterministic, headless Hearthstone rules-engine foundation using Bevy 0.19 ECS and schedules. The selected profile pins Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26).

The implementation is split into `hearthstone_simulator_core`, which owns durable models and serializable API contracts, and `hearthstone_simulator`, which owns Bevy execution and rules logic.

This repository is synthetic-card-first: it implements reusable mechanics and conformance fixtures, not the complete official card database. See [`DESIGN.md`](DESIGN.md), [`IMPLEMENTATION_PROGRESS.md`](IMPLEMENTATION_PROGRESS.md), and [`RULEBOOK_CONFORMANCE.md`](RULEBOOK_CONFORMANCE.md). Unchecked progress items are explicit implementation gaps.

## Implemented foundation

- Immutable stable game/resolution IDs, hook-maintained indexes, persistent card identity, and ordered zone indexes
- A single resource-owned LIFO stack of one-shot resolution operations, strict schedules, exact-once iterative execution, choice suspension, and per-sequence safety budgets
- Immutable event records, Death pre-check trigger seeds, and queue-time candidate snapshots that expand directly onto the stack without executable queue entities or cursors
- Data-oriented effects/selectors/values, fork-safe registered native effect handlers, and versioned seeded randomness with canonical RNG traces
- Signed mana/Overload counters, drawing, burning, fatigue, H1/H2-compliant Health recalculation, proposed-value damage/healing modifiers, ordered health mutations with prepared actual-event slots, Armor/Immune/Divine Shield handling, typed Health/Attack/Other auras, live in-play and attached Spell Damage, silence, transformation, zone-aware copying, native-keyword movement resets, and profile-owned zone/Battlefield capacity
- Phase-boundary mortality collection with irreversible Hero defeat, global play-order sorting across ordinary and full-zone deaths, creation-ordered death caching, staged Death Event trigger capture, dominant-player trigger grouping, chained Death Phases, and sequence-end outcomes
- Stable-ID card play/Hero Power/combat/end-turn/concede actions, ruleset-precedence natural/extra-turn scheduling, first-class permanent and turn-series-aware timed stat, keyword, ordered cost, and ordinary trigger enchantments, in-Play Hero Power and Hero replacement invariants, deterministic legal actions, canonical snapshots/traces, versioned JSON checkpoints, suspended-choice restoration, and checkpoint-exact forks

All card-type action sequences, choice-producing card mechanics, full Deathrattle-position and added-Deathrattle policy coverage, and remaining esoteric compatibility policies are tracked in the progress document. The resolver and checkpoint API preserve and restore generic pending choices, normalized turn grants, native state, and temporary durations.

## Action declarations

The supported player declarations are minion and spell plays, character attacks, Hero Power activations, `EndTurn`, and
`Concede`. A card can declare one of four explicit target requirements: no target, required,
optional, or required when matching targets exist. The current filter foundation selects friendly,
enemy, or either-player Minions, Heroes, or Characters; it intentionally does not implement
complete card-by-card targeting legality. Enemy Stealth and Immune prevent direct declarations;
friendly targets remain eligible. Taunt restricts attacks to visible, non-Immune enemy Taunt Minions.

```rust
use hearthstone_simulator::Simulation;
use hearthstone_simulator_core::{
    Card, Effect, GameAction, PlayerConfig, PlayerId, Selector, TargetAudience, TargetFilter,
    TargetKind, TargetRequirement, ValueExpression,
};

let bolt = Card::spell("Bolt", 1)
    .with_targeting(TargetRequirement::Required(TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    }))
    .with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(2),
    }]);
```

Declarations are validated and normalized before resolution begins. Targeted cards retain the
accepted target ID for their effects; resolution does not revalidate that declaration later.
Minion plays enumerate every explicit insertion position in canonical order, while a submitted
omitted minion position normalizes to the final append position. Spells do not accept a board
position, and every nonempty `PlayCard.choice` is currently rejected. `legal_actions()` returns
only canonical supported declarations, including valid character attacks, without changing game
state or trace.

Weapons, Hero cards, locations, combat redirection, complete phase guards, and
other keyword-specific combat legality remain outside this action-contract foundation.

## Taunt and Stealth

Minion, spell, and Hero Power declarations share the same direct-target restrictions. Enemy
Stealth and Immune targets are excluded before evaluating RequiredIfAvailable; friendly targets
remain eligible. Only attacks obey Taunt, and Stealth or Immune suppress a Minion's Taunt without
removing the keyword. Effective Immune includes aura contributions. Area and random effect
selectors are unchanged, and accepted declarations retain their captured targets.

After Attack-event reactions, a guarded `BreakAttackStealth` step consumes the attacker's current
Stealth before the existing damage batch. It uses a permanent ordered keyword-removal enchantment:
recalculation cannot restore consumed grants, while a later grant can restore Stealth. Existing
silence, transformation, backward movement, and copy policies apply to that enchantment. A subject
that has left Play skips this step. Damage success is not required to consume Stealth.

Checkpoint schema 11 persists this step and rejects earlier schemas. The boundary placement is
an explicit engine policy within the existing simplified combat sequence; complete preparation
phases, damage-stat refresh after attack reactions, redirection, and full combat guards remain
unimplemented. Current wiki references support the keyword interactions, but the pinned rulebook
revision remains unavailable for exact conformance verification.

## Hero Power activation

`UseHeroPower { player, power, target }` activates the player's current in-Play power by stable
ID. Validation shares the existing targeting contract, checks resources and exhaustion, and
rejects stale powers before mutation. `legal_actions()` includes activations after attacks and
before concession, in canonical target order.

Payment precedes effects. Effects use Hero Power damage modifiers, then the original power's
usage counter increments and it becomes exhausted. An ordinary death/aura boundary precedes
`AfterHeroPower` reactions; their trigger seeds and PreCheck conditions are captured before
activation effects. QueueTime and ResolutionTime conditions still run at their normal timing.
The final boundary and outcome check follow those reactions.

Under the explicit `OriginalPowerCompletion` engine policy, a power replaced during activation
retains completion bookkeeping on its old ID, including in RemovedFromGame; the new power stays
ready. If the original no longer exists or is no longer a Hero Power with power state, completion
omits that state mutation while remaining sequence work continues. This replacement policy and
boundary placement are not certified against the inaccessible pinned rulebook revision.
Checkpoint schema 11 preserves suspended activations and captured after-use seeds; older schemas
are rejected. Summon-only full-board restrictions, variable use limits, game-wide usage counters,
modal choices, and targeting redirection remain gaps.

Install a synthetic power through an ordinary Hero replacement, then activate it:

```rust
use hearthstone_simulator::Simulation;
use hearthstone_simulator_core::{
    Card, Effect, GameAction, HeroClassPolicy, HeroHealthPolicy, HeroReplacement,
    PlayerConfig, PlayerId, PlayerSelector,
};

let power = Card::hero_power("Resource Spark", 1).with_effects(vec![Effect::GainResource {
    player: PlayerSelector::Controller,
    amount: 2,
    temporary: true,
}]);
let install = Card::spell("Learn Spark", 0).with_effects(vec![Effect::ReplaceHero {
    player: PlayerSelector::Controller,
    replacement: Box::new(HeroReplacement {
        hero: Card::hero("Apprentice", 30),
        hero_power: power,
        armor_gain: 0,
        health: HeroHealthPolicy::Preserve,
        class: HeroClassPolicy::Keep,
        weapon: None,
    }),
}]);
let mut simulation = Simulation::new([
    PlayerConfig::new("One", vec![install]),
    PlayerConfig::new("Two", Vec::new()),
]);
let card = simulation.snapshot().players[0].hand[0];
simulation.apply(GameAction::PlayCard {
    player: PlayerId::One, card, target: None, board_index: None, choice: None,
})?;
let power = simulation.snapshot().players[0].hero_power.unwrap();
simulation.apply(GameAction::UseHeroPower { player: PlayerId::One, power, target: None })?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Draw, transform, and copy semantics

- Draw requests select the then-current top card one at a time. A successful draw moves the card to Hand before creating `CardDrawn`; Play-zone reactions resolve before the drawn card's Hand-zone reactions, and all draw consequences finish before the next draw.
- `DrawThen` uses a private checkpointed result slot. Its continuation runs after draw reactions and can bind the successfully drawn stable ID and read that card's current cost; burn and fatigue follow the continuation's explicit success policy.
- Transform operations preserve stable identity and placement while replacing form-owned state and detaching enchantments. Spell transforms add no summon timing, non-spell transforms run Summon Resolution aura work without a `Summoned` event, and played-self transforms run the inserted post-transform event before the captured original After Play event.
- Copies outside Play receive the source's current form and base cost without attachments or zone-local runtime state. Play-to-Play copies receive fresh identity, play order, controller, and position; clone eligible non-aura runtime state and enchantments with fresh deterministic IDs; reset attack usage; exclude received aura caches; then resolve ordinary summon work.

## Example

```rust
use hearthstone_simulator::Simulation;
use hearthstone_simulator_core::{Card, GameAction, PlayerConfig, PlayerId};

let mut simulation = Simulation::new([
    PlayerConfig::new("One", vec![Card::minion("Training Minion", 1, 1, 2)]),
    PlayerConfig::new("Two", Vec::new()),
]);
let card = simulation.snapshot().players[0].hand[0];
simulation.apply(GameAction::PlayCard {
    player: PlayerId::One,
    card,
    target: None,
    board_index: None,
    choice: None,
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Commands

```bash
aspect test //hearthstone_simulator/...
bazel run //hearthstone_simulator/app
bazel run //tools/coverage -- //hearthstone_simulator/...
```

Use Bazel/Aspect for all repository operations.
