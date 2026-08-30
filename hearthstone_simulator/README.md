# Hearthstone Simulator

A deterministic, headless Hearthstone rules-engine foundation using Bevy 0.19 ECS and schedules. The selected profile pins Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26).

This repository is synthetic-card-first: it implements reusable mechanics and conformance fixtures, not the complete official card database. See [`DESIGN.md`](DESIGN.md), [`IMPLEMENTATION_PROGRESS.md`](IMPLEMENTATION_PROGRESS.md), and [`RULEBOOK_CONFORMANCE.md`](RULEBOOK_CONFORMANCE.md). Unchecked progress items are explicit implementation gaps.

## Implemented foundation

- Immutable stable game/resolution IDs, hook-maintained indexes, persistent card identity, and ordered zone indexes
- A single resource-owned LIFO stack of one-shot resolution operations, strict schedules, exact-once iterative execution, choice suspension, and per-sequence safety budgets
- Immutable event records, Death pre-check trigger seeds, and queue-time candidate snapshots that expand directly onto the stack without executable queue entities or cursors
- Data-oriented effects/selectors/values, fork-safe registered native effect handlers, and versioned seeded randomness with canonical RNG traces
- Signed mana/Overload counters, drawing, burning, fatigue, H1/H2-compliant Health recalculation, proposed-value damage/healing modifiers, ordered health mutations with prepared actual-event slots, Armor/Immune/Divine Shield handling, typed Health/Attack/Other auras, live in-play and attached Spell Damage, silence, transformation, and zone-aware copying
- Phase-boundary mortality collection with irreversible Hero defeat, globally ordered simultaneous removal, staged Death Event trigger capture, dominant-player trigger grouping, chained Death Phases, and sequence-end outcomes
- Stable-ID card play/combat/end-turn/concede actions, deterministic legal actions, canonical snapshots/traces, versioned JSON checkpoints, suspended-choice restoration, and checkpoint-exact forks

Complete movement reset policies, all card-type action sequences, choice-producing card mechanics, and remaining esoteric compatibility policies are tracked in the progress document. The resolver and checkpoint API preserve and restore generic pending choices.

## Example

```rust
use hearthstone_simulator_core::{Card, GameAction, PlayerConfig, PlayerId, Simulation};

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
