# Hearthstone Simulator

A deterministic, headless Hearthstone rules-engine foundation using Bevy 0.19 ECS and schedules. The selected profile pins Hearthstone Wiki advanced rulebook revision 913067 (2026-06-26).

This repository is synthetic-card-first: it implements reusable mechanics and conformance fixtures, not the complete official card database. See [`DESIGN.md`](DESIGN.md), [`IMPLEMENTATION_PROGRESS.md`](IMPLEMENTATION_PROGRESS.md), and [`RULEBOOK_CONFORMANCE.md`](RULEBOOK_CONFORMANCE.md). Unchecked progress items are explicit implementation gaps.

## Implemented foundation

- Immutable stable game/resolution IDs, hook-maintained indexes, persistent card identity, and ordered zone indexes
- Relationship-backed, remappable resolution frames with strict schedules, suspension primitives, cleanup, and safety budgets
- Explicit event/trigger queue entities with condition timing, complete order keys, immutable frozen membership, and cursor-based resolution
- Data-oriented effects/selectors/values, fork-safe registered native effect handlers, and versioned seeded randomness with canonical RNG traces
- Signed mana/Overload counters, drawing, burning, fatigue, proposed-value damage/healing modifiers, ordered health mutations with frozen actual-event reactions, DH-compliant Armor/Immune/Divine Shield handling, destroy, summon, silence, stat enchantments, transformation, and copying
- Phase-boundary mortality collection with irreversible Hero defeat, globally ordered simultaneous removal, turn-stamped death records, frozen Death Event batches, play-order-mingled Deathrattles, and chained Death Phases
- Stable-ID card play/combat/end-turn/concede actions, deterministic legal actions, canonical snapshots/traces, and replay-equivalent forks

Aura-provider discovery, complete movement reset policies, all card-type action sequences, suspended player choices, and esoteric compatibility policies remain tracked in the progress document.

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
