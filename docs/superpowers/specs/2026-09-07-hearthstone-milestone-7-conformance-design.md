# Hearthstone Milestone 7 Conformance Design

## Status

Approved in chat on 2026-09-07. This design closes the remaining Milestone 7 draw, transformation, and copying conformance rows for `AdvancedRulebook2026_06_26`.

## Goal

Implement the general rules in Advanced Rulebook revision 913067 for:

- DC1-DC5 card draw ordering, burning, fatigue, draw-result continuation, and attribution
- Transformation cleanup and spell/non-spell timing
- Zone-specific copying and runtime state transfer

The milestone remains synthetic-card-first. Official card definitions, historical behavior, and named card-specific exceptions remain out of scope unless they expose a missing general mechanic.

## Source Rules

The relevant rulebook behavior is:

- DC1 fixes the number and player order of simultaneous draws, then draws and fully resolves each card sequentially. It does not snapshot every card identity before resolution.
- DC2 moves a successfully drawn card into Hand before Battlefield draw triggers. Battlefield triggers resolve before the drawn card's own Hand trigger.
- DC3 makes an enclosing effect wait until the draw and all its consequences finish. Effects that operate on "that card" require successful entry into Hand when their definition says so.
- DC4 sends a top-deck card to Graveyard when Hand is full. The card is neither drawn nor discarded and produces no draw, discard, Death Event, or death-cache consequences.
- DC5 attributes a draw to its immediate drawing effect rather than an enclosing effect.
- A transform immediately replaces the target's form, detaches modifications, and is neither a death nor an ordinary summon. Eligible non-spell transforms participate in Summon Resolution timing.
- A copy created outside Play copies the source's current form without attached enchantments. A Play-to-Play copy copies runtime state and attached enchantments except aura-derived state, controller, zone, position, play age, and identity.

## Approach

Use explicit, serializable resolution operations and narrow state-transfer policies. Do not introduce a universal mutation pipeline or certify conformance through final-state tests alone.

This approach fits the existing one-shot LIFO resolver:

- Each operation performs one bounded mutation or expansion.
- Nested draw and transform consequences resolve above pending siblings.
- Pending work and result bindings survive checkpoints without resumable frames.
- Canonical traces expose rule-sensitive ordering.

## Draw Model

### Core contracts

Add serializable draw request and result data:

```rust
struct DrawRequest {
    player: PlayerId,
    source: Option<GameEntityId>,
    result: DrawResultSlotId,
}

enum DrawOutcome {
    Drawn(GameEntityId),
    Burned(GameEntityId),
    Fatigue { amount: i32 },
}
```

`ResolutionWork` owns draw result slots in the same manner as prepared-event slots. A slot is allocated before its request is pushed and filled exactly once. A continuation consumes and removes its slot; a finish operation removes a plain draw's unconsumed slot. No slot survives an idle boundary.

Add one-shot operations for processing one draw, applying its fatigue consequence, and consuming its result. The exact variant names may follow existing naming conventions, but each operation must remain serializable and contain logical IDs only.

### Multiple draws

`Effect::Draw { count }` fixes the count and player ordering when it expands. It pushes one request per draw in reverse order. Each request selects the then-current top card when it executes.

The request does not preselect every card. A nested consequence from an earlier draw may change the deck before the next request selects its top card.

### Successful draw

A successful request:

1. Selects the current top card.
2. Moves it from Deck to Hand with `ZoneMovementKind::Draw`.
3. Stores `DrawOutcome::Drawn(card)`.
4. Creates and resolves a `CardDrawn` event only after the move.
5. Completes all event consequences before exposing the result to its continuation or starting the next draw.

The existing complete trigger order key must give Play-zone triggers priority over Hand-zone triggers for `CardDrawn`. The drawn card is already in Hand during candidate collection, so its own when-drawn trigger can join after Battlefield triggers. Event source attribution records the immediate drawing effect, while the drawn card is the event target.

### Burn

When Hand is full, the request moves the selected top card directly to Graveyard and stores `DrawOutcome::Burned(card)`.

This branch creates no `CardDrawn`, discard, or Death Event and does not add a death-cache record. It still records the zone movement and draw outcome in the canonical trace. A continuation that requires a successfully drawn card does not run.

### Fatigue

When Deck is empty, the request increments the player's fatigue counter, stores `DrawOutcome::Fatigue { amount }`, and schedules ordinary damage against that player's active Hero. Fatigue damage uses the existing proposed/actual damage pipeline and therefore preserves protection, reactions, Death Creation, and outcome timing.

No physical card moved into Hand, so this branch does not create a successful `CardDrawn` event. The fatigue consequence nevertheless resolves completely before the enclosing effect continues or the next draw starts.

### Draw-result continuation and attribution

Add a data-oriented single-card draw continuation effect. It allocates a private result slot, pushes the continuation beneath the draw request, and binds a successful `Drawn(card)` result into an extended `EffectContext`. The effect language exposes the bound card through a selector and value expressions needed by general "draw, then use that card" mechanics, initially including its stable ID and current cost.

Burn and fatigue leave the bound card absent. The continuation definition explicitly states whether it runs without a card; consumers such as a Holy Wrath-style fixture can require success.

Each request has its own result slot and immediate source. A nested draw cannot overwrite or satisfy an outer continuation, which implements DC5 attribution without an execution ancestry graph.

## Transformation Model

### Explicit kind

Transformation carries a serializable `TransformKind` selected by the effect definition rather than inferred from incidental runtime context. The general kinds are:

- Spell transformation
- Non-spell transformation eligible for Summon Resolution
- Played self-transformation with its required inserted after-play/after-summon timing

Named card-specific exceptions remain definition or native-effect behavior.

### State replacement

A transform operation validates the target and replacement before mutation. It then:

1. Detaches all attached enchantment entities according to the ordinary transform policy.
2. Replaces definition, display, kind, base/current stats, native keywords, ability programs, triggers, auras, continuous effects, and base/effective cost.
3. Clears old-form damage, silence, pending-destroy state, and other non-preserved runtime form state.
4. Keeps the target's stable engine ID and zone position so already-captured references continue to identify the transformed object.
5. Records the old and new definition against that stable ID in the canonical trace.

Transformation creates no Death Event, death-cache entry, ordinary Summoned event, or resurrection record. Cached aura effects from the old provider are not refreshed early; they expire at the next rulebook aura boundary. A non-spell transform participates only in its specifically required Summon Resolution step.

A played self-transform uses the rulebook's explicit sequence:

1. Its Battlecry phase transforms the original subject.
2. An inserted After Play and After Summon phase resolves eligible triggers for the post-transform subject.
3. Ordinary between-phase boundary work runs.
4. The original After Play phase resolves eligible triggers associated with the pre-transform subject; there is no original After Summon phase.

The transform kind is part of the serialized effect definition so spell transforms and special after-play transforms cannot accidentally enter this sequence.

## Copy Model

### Explicit operation

`Effect::Copy` expands to a one-shot `CopyEntity` request. The request captures source state when it executes, allocates a new stable ID only if creation can succeed, and applies a destination policy selected from source and destination zones.

Missing sources and full generated destinations are deterministic no-ops. They do not consume a game ID or create then destroy a temporary entity.

### Non-Play copies

Copies created in Hand, Deck, or another non-Play zone copy the source's current card form:

- Definition and display data
- Entity kind and base stats
- Native keywords
- Ability, trigger, aura, and continuous-effect definitions belonging to the current form
- Base cost, not the current enchantment-modified effective cost

They do not copy attached stat, keyword, cost, trigger, or continuous-effect enchantments; damage; silence; pending destroy; aura cache entries; controller; position; play order; or other zone-local state.

### Play-to-Play copies

A copy put into Play from an in-Play source receives:

- A fresh stable game ID and play order
- The source's current form and non-aura runtime state
- Current damage and silence state
- Pending-destroy and other copied form state unless a named policy excludes it
- Newly allocated copies of eligible attached enchantment entities, preserving their deterministic attachment order and payloads

It does not copy:

- Aura-derived stat, keyword, or continuous-effect cache entries
- Controller, zone, board position, stable identity, or play order
- Time in play or attack usage

The destination request determines controller and board position. The new copy begins with summoning sickness unless copied native state such as Charge overrides it. It then participates in ordinary summon-resolution and aura timing for an entity put into Play.

Cloned enchantments receive fresh stable IDs, attach to the copy, and retain only fields that the relevant copy policy allows. Their creation order is deterministic and follows the source attachment order.

## Errors And Atomicity

Normal rule outcomes are data, not errors:

- Full-hand draw is `Burned`.
- Empty-deck draw is `Fatigue`.
- Full-zone generation and missing copy sources are no-ops.

Errors are reserved for corrupted resolver state, missing or multiply filled result slots, invalid checkpoint references, and structurally invalid transformations. Transform and copy validate all required source/replacement data before detaching or spawning anything, preventing partial mutation.

Idle and complete simulations must have no draw slots, pending draw operations, prepared events, or choices. Checkpoint restoration validates every logical entity and draw-slot reference before rebuilding executable work.

## Trace And Checkpoints

Canonical traces add enough data to assert:

- Draw request source and player
- `Drawn`, `Burned`, or `Fatigue` outcome
- Successful draw event creation after movement
- Transform stable ID and old/new definitions
- Copy source/new identity and selected state-transfer policy

Draw slots, new operations, extended effect contexts, and transform/copy policies are included in versioned checkpoints. A checkpoint taken while awaiting a choice nested inside draw consequences must restore and continue to the same result, snapshot, and trace. The checkpoint schema version changes rather than silently accepting old payloads under the current version.

## Files

Expected core changes:

- `hearthstone_simulator/core/effect.rs`
- `hearthstone_simulator/core/event.rs`
- `hearthstone_simulator/core/ids.rs`
- `hearthstone_simulator/core/resolver.rs`
- `hearthstone_simulator/core/checkpoint.rs`
- `hearthstone_simulator/core/trace.rs`

Expected simulator changes:

- `hearthstone_simulator/simulator/simulation_effect_executor.rs`
- `hearthstone_simulator/simulator/simulation_event_resolver.rs`
- `hearthstone_simulator/simulator/simulation_checkpoint.rs`
- `hearthstone_simulator/simulator/simulation_player.rs`
- `hearthstone_simulator/simulator/zone.rs`
- Existing focused test modules, split only if file size makes a dedicated draw/copy module clearer

Documentation changes:

- `hearthstone_simulator/IMPLEMENTATION_PROGRESS.md`
- `hearthstone_simulator/RULEBOOK_CONFORMANCE.md`
- `hearthstone_simulator/README.md`

Gazelle determines any BUILD metadata updates after source edits.

## Testing

Focused conformance tests cover:

- DC1 fixed draw count and player order with sequential top-card selection
- A nested first-draw consequence changing the second draw's top card
- DC2 card movement before reactions and Battlefield-before-Hand trigger order
- DC3 completion of all draw consequences before continuation
- Successful and failed draw-result binding
- DC4 burn with no draw, discard, Death Event, or death-cache consequences
- DC5 nested draws retaining immediate attribution and separate result slots
- Fatigue increment, ordinary damage reactions, repeated attempts, lethal timing, and outcome checks
- Transform attachment cleanup, state reset, stable identity, and no death/resurrection consequences
- Delayed aura removal and spell versus non-spell/played-self timing
- Non-Play copies omitting attachments and effective-cost modifiers
- Play copies preserving damage, silence, copied form state, and eligible attachments
- Play copies excluding aura cache, identity, controller, position, play age, and attack usage
- Deterministic cloned-enchantment order and fresh IDs
- Missing-source and full-zone copy no-ops without ID consumption
- Checkpoint JSON round trips and fork equivalence with pending draw work
- Canonical trace order for movement, draw reactions, fatigue, transforms, and copies

Completion requires:

1. Run Gazelle immediately after source edits.
2. Run repository-wide formatting.
3. Pass all Hearthstone simulator tests.
4. Pass repository lint.
5. Cover all changed lines or document an unreachable exception.
6. Pass the full repository build.
7. Mark both conformance rows and Milestone 7 complete only after focused tests pass.

## Non-Goals

- Complete official card coverage
- Historical draw, transform, or copy bugs
- Card-specific transformation exceptions such as Shifter Zerus in core policy
- Player-action sequences from Milestone 8
- Forced Death Phase and Deathrattle-position policies from Milestone 9
- A generalized transaction or mutation framework

## Acceptance Criteria

Milestone 7 is complete when all general source rules above are represented by serializable resolver data, focused tests prove their ordering and state-transfer behavior, pending work survives checkpoints, canonical traces expose the important boundaries, documentation no longer labels the two rows Partial, and the required Bazel verification succeeds.
