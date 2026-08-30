# Hearthstone Simulator Design

## Status

This document is the implementation plan and target architecture for evolving the current Bevy scaffold into a deterministic, headless Hearthstone rules engine.

The initial behavioral reference is the Hearthstone Wiki advanced rulebook revision [913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067), dated 2026-06-26. The advanced rulebook is unofficial: it records observed game behavior and includes historical behavior, exceptions, and suspected bugs. The engine must therefore identify its ruleset explicitly rather than presenting its behavior as an unversioned universal definition of Hearthstone.

## Scope

### In scope

The first complete ruleset profile covers the current constructed-game mechanics described by the pinned advanced rulebook:

- Runtime entities and zones
- Sequences, phases, events, and triggers
- Immutable event and trigger candidate snapshots
- One-shot, depth-first LIFO resolution
- Order of play, trigger priority, and player/zone ordering
- Aura updates, summon resolution, death creation, and chained death phases
- Start/end turn, card play, combat, location, and Hero Power sequences
- Minions, spells, weapons, Hero cards, locations, permanents, and Dormants
- Enchantments, auras, silence, transformation, and copying
- Damage, healing, drawing, fatigue, mana, Overload, and costs
- Hero replacement and win/loss/draw handling
- Seeded random effects
- Rulebook esoterica through explicit compatibility policies
- Canonical snapshots, deterministic traces, and suspended resolution

### Out of scope

The following are separate projects or adapters:

- A complete official card database
- Rendering and animation
- Wall-clock turn timers and animation slush time
- Battlegrounds and Mercenaries rules
- Network protocol compatibility with Blizzard's client or server
- Historical behavior that is no longer part of the selected ruleset profile

Card fixtures used for conformance tests are intentionally small, synthetic definitions. Adding every official card should build on the engine after its mechanics are stable.

## Current state and required redesign

The current scaffold in `core/model.rs` and `core/simulation.rs` supports two players, mana progression, minion cards, summoning sickness, and attacks against the opposing Hero. It is not a suitable state model for advanced mechanics because it currently:

- Stores cards as values inside a player's hand
- Creates a separate minion entity when a card is played
- Has no persistent card/entity identity across zones
- Applies attack damage and game victory immediately
- Uses unsigned, capped mana values
- Has no board positions, decks, graveyards, or other zones
- Has no events, triggers, phases, auras, enchantments, or death processing
- Has no deterministic random source or resolution trace

These assumptions should be replaced rather than wrapped with exceptions.

## Design goals

1. **Rules fidelity:** engine structure must directly express the advanced rulebook's timing and ordering rules.
2. **Determinism:** the same initial state, ruleset, action sequence, and random seed must produce the same snapshot and trace.
3. **Bevy-native state:** durable game state remains visible to ECS queries, while serializable resolution data is owned by an explicit resource and processed through Bevy schedules and systems.
4. **Explicit ordering:** no gameplay result may depend on Bevy query iteration, archetype layout, raw entity IDs, or parallel scheduler order.
5. **Suspendability:** resolution can pause for a player choice without losing pending LIFO work or prepared-event state.
6. **Inspectability:** tests and debugging tools can inspect pending operations, events, prepared-event slots, and ordered trigger snapshots.
7. **Data-oriented effects:** runtime state contains data, not closures or mutable object graphs.
8. **Searchability:** snapshots and resolution state can eventually support AI branching and replay.
9. **No hard-coded interactions:** general mechanics and ability metadata should explain interactions; exceptional behavior is isolated and named.

## Terminology

This document uses the rulebook's terms:

- A **player action** begins a sequence while the simulation is awaiting input.
- A **sequence** is an ordered plan of phases and steps for one player action or generated operation.
- A **phase** surrounds one or more events or ordered trigger snapshots. A player-action compiler places normal boundary work after each outermost phase.
- An **event** is a game-state change that triggers can observe.
- A **trigger** reacts to an event and may produce nested events and effects.
- A **trigger seed** records a trigger source and definition that passed pre-check at the timing required by the ruleset.
- A **candidate snapshot** is the immutable, explicitly ordered trigger membership captured after queue-time conditions are evaluated when an event begins resolution.
- A **resolution operation** is a small, one-shot internal instruction that is popped from the LIFO work stack, executed atomically, and never resumed.
- **Resolution** is depth-first: newly generated consequences are pushed above pending siblings and therefore complete first.
- A **game entity** is an object meaningful to Hearthstone, such as a card, minion, Hero, weapon, enchantment, or hidden effect.

## High-level architecture

```text
Simulation API
    |
    v
Action validation schedule
    |
    v
Compile one public GameAction into ResolutionOps
    |
    v
Exclusive LIFO resolution driver
    |
    +--> pop one ResolveFrame operation
    |       +--> mutate state and push child operations in reverse order
    |
    +--> execute explicit ResolvePhaseBoundary operations
    |       +--> aura, summon, and death systems
    |
    +--> execute explicit CheckOutcome operations at ruleset checkpoints

Game ECS entities <---- primitive effect reducers ---- ResolutionWork resource
       |                                               |
       +--------------- canonical trace ---------------+
```

Simulator-owned domain state falls into two deliberately separate categories:

- `GameObject` entities hold durable game state.
- `ResolutionWork` holds the serializable LIFO operation stack, safety budget, prepared-event slots, and pending choice data.

Resolution operations and prepared events are values identified by logical IDs, not an ECS relationship graph. Bevy may still own entities for resources, registered systems, observers, and framework facilities. Gameplay systems must select durable simulator entities through `GameObject` rather than iterating every world entity or assuming that every unmarked entity is gameplay state.

## Runtime game entities

### Stable identity

Every game entity receives a stable logical ID independent of Bevy's generational `Entity` value:

```rust
#[derive(Component, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[component(
    immutable,
    on_insert = index_game_entity,
    on_discard = unindex_game_entity,
)]
#[require(GameObject)]
struct GameEntityId(u64);
```

`GameEntityId`, `ResolutionId`, definition identity, and other write-once metadata use Bevy immutable components. Normal creation APIs validate logical-ID uniqueness before insertion; insertion and discard hooks keep the lookup index synchronized and assert the invariant for internal callers. Required components encode structural invariants that have deterministic constructors, but required-component constructors do not allocate logical IDs or perform ruleset validation.

A resource maps logical IDs to Bevy entities for efficient runtime access. Canonical snapshots and traces contain logical IDs, never raw Bevy entity IDs.

Playing a card moves the same game entity from Hand to Play. Transformation is a precisely defined replacement operation, not the normal way cards enter play.

### Core components

The initial game-object component model includes:

```text
GameObject
GameEntityId
DefinitionId
EntityKind
Controller
Zone
ZonePosition
PlayOrder
BaseStats
CurrentStats
Damage
Armor
PendingDestroy
Keywords
Abilities
Enchantments
```

Not every entity has every component. For example, a weapon has durability, an enchantment has an attachment relationship, and a Player has resource counters.

Table storage remains the default. Frequently added or removed marker components, such as `PendingDestroy`, may use Bevy sparse-set storage after benchmarks confirm that avoiding archetype moves outweighs slower iteration. Value components such as `Zone` remain table-stored.

### Zones and indexes

At minimum, the engine supports:

- Deck
- Hand
- Play
- Secret
- Graveyard
- SetAside
- RemovedFromGame

Zone order and zone limits belong to the selected ruleset profile. Controller is separate from zone because changing controller can behave like a zone movement without changing the zone kind.

Explicit ordered zone indexes are authoritative for hand order, deck order, and board position. Bevy queries discover candidates, but cannot define positional or order-of-play semantics.

### Order of play

A monotonic `PlayOrderCounter` assigns timestamps whenever rules say an entity enters play or an attached enchantment establishes its own order. Trigger and simultaneous-event ordering use these values plus ruleset-specific priority and player/zone grouping.

Heroes, weapons, locations, permanents, Dormants, Hero-zone cards, attached enchantments, and added Deathrattles participate when required by the rulebook.

## Card definitions and effect programs

Static `CardDefinition` data is separate from runtime entities. Definitions contain:

- Card type and base tags
- Base cost and stats
- Keywords
- Targeting requirements
- Trigger definitions
- Battlecry, Deathrattle, spell, location, and Hero Power programs
- Aura definitions

Reusable card behavior is represented by cloneable data:

```rust
Effect
Selector
Condition
ValueExpression
TriggerDefinition
AuraDefinition
```

Effects request primitive operations such as damage, healing, movement, summoning, drawing, transformation, or attaching an enchantment. They do not mutate arbitrary world state directly.

A static `NativeEffectId` escape hatch may be added for effects that cannot reasonably be expressed in the common intermediate representation. Native handlers are registered by the plugin as Bevy systems, and a registry maps stable `NativeEffectId` values to typed `SystemId`s. Runtime state stores only the stable handler ID and data, never the entity-backed `SystemId`.

The resolver may invoke a handler through `World::run_system_with`, allowing handlers to use typed inputs and ordinary `SystemParam`s. Native handlers should return effect plans that flow back through primitive reducers rather than mutate arbitrary gameplay state. Because `run_system_with` flushes commands queued by the handler immediately, handlers either avoid `Commands` or treat that flush as an explicit, tested mutation boundary.

## Bevy schedules

Bevy schedules orchestrate the engine, but do not define Hearthstone ordering implicitly.

### Main schedules

```rust
#[derive(ScheduleLabel, Clone, Debug, Eq, Hash, PartialEq)]
struct ResolveFrame;

#[derive(ScheduleLabel, Clone, Debug, Eq, Hash, PartialEq)]
struct ResolvePhaseBoundary;
```

The normal Bevy `Update` schedule accepts at most one pending player action, validates it, compiles it into internal `ResolutionOp` values, and invokes an exclusive resolution driver. The driver pops one operation at a time until the stack is empty, a choice suspends resolution, the game ends, or a configured safety budget is exhausted.

`ResolveFrame` executes the operation already popped by the driver. An operation may mutate state and append more one-shot operations, but it must never invoke the driver or resolve another operation recursively. Explicit `RunPhaseBoundary` operations may invoke `ResolvePhaseBoundary` because it is different from the currently running schedule. The driver must not recursively invoke the same schedule label; Bevy temporarily removes a schedule from the world while running it.

### Schedule guardrails

Resolution schedules are configured strictly:

```rust
ScheduleBuildSettings {
    ambiguity_detection: LogLevel::Error,
    hierarchy_detection: LogLevel::Error,
    ..default()
}
```

Consequently, unordered conflicting systems fail schedule construction. Resolution operations use exclusive direct world access and do not issue buffered `Commands`.

A single-threaded executor may reduce overhead for small, mostly exclusive resolution schedules, but it is a benchmark-driven implementation choice, not a determinism guarantee. Gameplay order always comes from sorted value records and LIFO operation placement.

### Coarse simulation state

Bevy States may represent coarse external lifecycle only:

```text
SettingUp
AwaitingAction
Resolving
AwaitingChoice
Complete
```

They do not model individual rulebook phases. Rulebook phases are nested runtime data, while Bevy States represent one global current state and transition at schedule boundaries.

The repository currently disables Bevy default features. If this design uses Bevy States, the `bevy_state` feature must be enabled explicitly and transition timing must be included in the synchronous `Simulation::apply` contract. Otherwise the same lifecycle is represented by an ordinary resource enum without `OnEnter` or `OnExit` schedules.

### Ordered system sets

Static pipelines use chained `SystemSet`s. For example:

```rust
#[derive(SystemSet, Clone, Debug, Eq, Hash, PartialEq)]
enum PhaseBoundarySet {
    HealthAttackAuras,
    QuestRewards,
    SummonResolution,
    RefreshHealthAttackAuras,
    CreateDeaths,
    OtherAuras,
    CompileDeathPhase,
}
```

```rust
app.configure_sets(
    ResolvePhaseBoundary,
    (
        PhaseBoundarySet::HealthAttackAuras,
        PhaseBoundarySet::QuestRewards,
        PhaseBoundarySet::SummonResolution,
        PhaseBoundarySet::RefreshHealthAttackAuras,
        PhaseBoundarySet::CreateDeaths,
        PhaseBoundarySet::OtherAuras,
        PhaseBoundarySet::CompileDeathPhase,
    )
        .chain(),
);
```

The exact pipeline is ruleset data and must be confirmed against the pinned rulebook during the conformance milestone.

### Direct mutation

Exact mutation timing is essential. Resolution schedules therefore use exclusive systems and direct `World` mutation for primitive game mutations, event-slot preparation, trigger candidate discovery, and LIFO stack expansion. Candidate discovery computes complete ordering keys, sorts value records, and pushes one-shot attempt operations in reverse order. Resolution operations and primitive reducers do not issue buffered `Commands`.

## LIFO resolution work stack

### One-shot operations

A validated public `GameAction` compiles into small internal `ResolutionOp` values. The operations live in one resource-owned `Vec` used strictly as a LIFO stack:

```rust
#[derive(Resource, Clone, Debug)]
struct ResolutionWork {
    stack: Vec<ResolutionOp>,
    remaining_budget: usize,
    next_resolution_id: u64,
    events: BTreeMap<EventId, PreparedEvent>,
    event_slots: BTreeMap<EventSlotId, PreparedEventSlot>,
    pending_choice: Option<PendingChoice>,
}

#[derive(Clone, Debug)]
enum ResolutionOp {
    RunSequenceStep(SequenceStep),
    RunPhaseBoundary(PhaseBoundaryPlan),
    ResolveEvent(EventId),
    ResolveEventSlot(EventSlotId),
    AttemptTrigger(TriggerCandidate),
    RunEffect {
        context: EffectContext,
        effect: Effect,
    },
    ProcessDamage {
        request: DamageRequest,
        actual_event: EventSlotId,
    },
    ApplyDamage {
        request: DamageRequest,
        proposed_event: EventId,
        actual_event: EventSlotId,
    },
    RequestChoice(ChoiceRequest),
}
```

The concrete enum grows with primitive mechanics, but every variant obeys the same contract:

1. The exclusive driver pops it before execution.
2. It performs one bounded mutation or expansion and runs to completion.
3. It may push additional operations in reverse of their required execution order.
4. It never calls the resolution driver, waits for a child, or resumes later.
5. Once popped, that operation is never retried or re-resolved.

Operations store logical game, event, choice, and slot IDs rather than raw Bevy `Entity` values. `EffectContext` carries gameplay inputs such as source, controller, and declared target; it is not execution ancestry. There is no active-frame cursor, parent relationship, execution ancestry, mutable program counter, or per-kind resume state.

### Iterative driver

The driver is the only code allowed to pop work:

```rust
fn drive_resolution(world: &mut World) -> Result<(), ResolutionError> {
    while let Some(operation) = pop_resolution_op(world) {
        consume_resolution_budget(world)?;
        world.insert_resource(CurrentResolutionOp(operation));
        world.run_schedule(ResolveFrame);
        if resolution_is_suspended(world) {
            break;
        }
    }
    Ok(())
}
```

`ResolveFrame` consumes the already-popped operation exactly once. An operation that produces ordered children pushes the last child first and the first child last. The first logical child is therefore on top and runs next. If that child creates nested work, the nested operations land above every pending sibling and complete first, providing depth-first semantics without recursive Rust calls.

For example, resolving event `A` with triggers `T1` and `T2` replaces the popped event operation with this pending work, shown bottom to top:

```text
AttemptTrigger(T2)
AttemptTrigger(T1)  <- top
```

If `T1` produces event `B`, `ResolveEvent(B)` is pushed above `T2`; all work generated by `B` is exhausted before `T2` can be popped.

A per-sequence operation budget prevents pathological or accidental infinite event generation from exhausting memory or CPU. The LIFO exact-once invariant prevents an existing item from being repeated, but it does not prevent effects from generating an unbounded series of new items. Budget exhaustion is therefore the universal resolver safety mechanism and reports the operation that exhausted the budget. There is no generic repeated-event or active-trigger recursion guard; any rulebook-mandated suppression is an explicit named trigger policy.

### Sequences, phases, and boundaries

Sequence builders encode ordering directly by pushing granular operations in reverse. They place a `RunPhaseBoundary` operation beneath all work belonging to an outermost phase, so that the boundary becomes reachable only after the phase's nested consequences are exhausted. No runtime phase ancestry or completion callback is needed.

Nested phases omit the ordinary boundary operation. Forced Death Phases compile an explicit specialized boundary plan rather than pretending to be ordinary outermost phases. Ordering barriers are ordinary one-shot operations pre-positioned below the work they follow, not resumable parent frames.

### Ordered trigger snapshots

Events do not create executable queue entities. Event occurrence records immutable context without necessarily populating its trigger queue. At ruleset-defined pre-check timing, discovery may store immutable `TriggerSeed` values. When `ResolveEvent` is popped, it evaluates queue-time conditions over those seeds (or discovers and pre-checks ordinary-event seeds at that point), computes complete order keys, and stores an immutable candidate snapshot. It then pushes one `AttemptTrigger` operation per candidate in reverse order. The stack is the only executable queue.

```rust
#[derive(Clone, Debug)]
struct TriggerCandidate {
    source: GameEntityId,
    event: EventId,
    definition_index: u32,
    definition: TriggerDefinition,
    controller: PlayerId,
    order: TriggerOrderKey,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TriggerOrderKey {
    player_bucket: u8,
    zone_bucket: u8,
    priority: i16,
    play_order: u64,
    source: GameEntityId,
    tie_breaker: u32,
}
```

The selected ruleset computes every ordering field. The player bucket is derived from durable `DominantPlayer` state, so the dominant player's triggers remain ahead of the secondary player's triggers and priority applies only within those groups. Stable logical source IDs and definition indexes provide deterministic ties, so no gameplay order falls back to ECS iteration or raw entity order.

`AttemptTrigger` checks source eligibility and resolution-time conditions when it is popped. If the source or condition is no longer valid, it records an abort and produces no child work. If valid, it pushes the captured trigger definition's effects in reverse order. Sources created after a pre-check seed snapshot cannot join that event, while current game state may make an existing seed pass its later queue-time conditions.

There is no `QueueState`, `FrozenQueueEntries`, `QueueCursor`, `select_next`, `finish_selected`, or separate queue resolver. Exact-once stack consumption is the mutable progress mechanism. Snapshot immutability is enforced by constructing candidate values once and never exposing a mutation API for their membership or order.

### Prepared event slots

Some events occur now but must react later. A simultaneous batch preallocates logical `EventSlotId` values and places `ResolveEventSlot` operations below all mutation work. Applying a mutation fills its slot with either no event or an immutable prepared event:

```rust
#[derive(Clone, Debug, Default)]
struct PreparedEventSlot {
    event: Option<EventId>,
}

#[derive(Clone, Debug)]
struct PreparedEvent {
    context: EventContext,
    prechecked_triggers: Option<Vec<TriggerSeed>>,
    candidates: Option<Vec<TriggerCandidate>>,
}
```

Applying a positive mutation allocates an `EventId`, inserts its context-only `PreparedEvent` in `ResolutionWork.events`, and writes that ID into the slot. Much later, popping `ResolveEventSlot` takes the ID and pushes `ResolveEvent`; popping `ResolveEvent` captures queue-time candidates and pushes their attempts. An empty slot is a no-op. Slots and event records are data dependencies, not executable queues, and are removed after consumption.

### Choices and suspension

`RequestChoice` is also one-shot. When popped, it stores a serializable `PendingChoice`, changes the simulation to `AwaitingChoice`, and stops the driver while leaving lower stack items untouched. Supplying a valid answer compiles the selected branch into new operations above those pending items and restarts the driver. No operation remains half-executed, and no frame needs suspend/resume state.

## Events and triggers

### Event context

An immutable event record stores the stable data needed throughout resolution:

```text
Event ID
Event kind
Source
Targets
Controller
Proposed and actual values
Sequence subject
Simultaneous-event ordinal
Creation context
```

Event data captures identity and declared targets, but effects query current game state when the rulebook requires the latest state.

### Simultaneous events

An effect that creates simultaneous damage or healing first fixes the request order according to the applicable rule and allocates one eventual-event slot per request. It then pushes actual-event slot resolvers and request processors so the LIFO pop order is:

```text
Process request 1
Process request 2
...
Process request N
Resolve actual-event slot 1
Resolve actual-event slot 2
...
Resolve actual-event slot N
```

Each request processor passes protection, creates and resolves its proposed event, and leaves an `ApplyDamage` or `ApplyHealing` operation directly beneath that proposed event. The apply operation reads the possibly modified value, performs the durable mutation, and fills its actual-event slot only for a positive actual change. Every mutation therefore occurs before any actual-event reaction, while proposed-event work for request N can still observe mutations from requests 1 through N-1.

This requires no batch queue or separate freeze-and-resolve pass. The pre-positioned slot operations are the ordering barrier. Each event records immutable context when it occurs and captures queue-time candidates when its resolver reaches the top. Death batches additionally freeze pre-check-eligible trigger seeds for all Death Events at Death Creation, then evaluate each event's queue-time conditions only after earlier Death Events have resolved fully.

Combat-specific attacker/defender event order is encoded by the combat event builder rather than inferred from play order.

### Trigger definition

A trigger definition separates:

- Event filter
- Pre-check policy
- Queue-time condition
- Resolution-time condition
- Eligible zones
- Source eligibility policy
- Priority
- Controller used for ordering
- Mortally wounded/pending-destroy target policy
- Any rulebook-specific suppression policy
- Effect program

This separation is required for interactions such as Secrets captured as candidates that later abort, and effects that must have been valid when a sequence began.

### Re-entrancy and termination

Each event has a stable logical ID and captures each eligible trigger definition at most once. That snapshot expands into exactly one `AttemptTrigger` operation per candidate, and popping the operation guarantees it cannot run twice for the same event.

A trigger responding to a newly generated event is new work and is not suppressed merely because the same definition caused that event. There is no execution ancestry, active-trigger set, direct self-nesting guard, or generic repeated-event guard. Interactions continue according to ordinary event semantics until they terminate naturally or exhaust the per-sequence operation budget. If the selected ruleset suppresses a specific interaction, that behavior is represented as a named trigger policy with focused conformance tests rather than a resolver-wide recursion rule.

## Primitive reducers

A primitive reducer is a deterministic function that accepts validated input, performs one bounded durable game-state mutation, and returns the observed result. It never resolves an event, invokes the driver, or pushes LIFO work. Initial reducer families include:

```text
MoveEntity
GenerateEntity
ChangeController
SpendResource
GainResource
ApplyDamage
ApplyHealing
MarkPendingDestroy
Transform
AttachEnchantment
DetachEnchantment
Silence
EquipWeapon
ReplaceHero
RefreshHeroPower
```

Resolution operations orchestrate reducers and event preparation. For example, `ProcessDamage` pushes `ApplyDamage` beneath a proposed-damage event. The proposed event and every consequence it generates resolve first; `ApplyDamage` then invokes the damage reducer with the final amount and prepares the delayed actual-damage event for its pre-positioned slot.

Reducers append canonical trace entries for observable mutations and return enough data for the calling operation to prepare any resulting event.

## Phase-boundary processing

The ordinary outermost phase boundary is modeled as ordered systems, not hidden cleanup:

1. Health/Attack aura update
2. Quest reward step, where applicable
3. Summon resolution step
4. Second Health/Attack aura update
5. Death creation step
6. Other aura update
7. Creation of a Death Phase when deaths were recorded

Death Phases are explicit outermost operation plans and can enqueue additional Death Phase plans until no new deaths exist. A normal action plan places `CheckOutcome` below its ordinary boundary, so every chained Death Phase finishes before defeated-Hero markers become the final result. Exceptional combat checkpoints are represented by their own explicit outcome operation.

The Death Creation system:

1. Finds all mortally wounded and pending-destroy entities eligible to die.
2. Orders them by the ruleset's event order.
3. Captures controller and remembered board position.
4. Removes them from play without running intervening triggers.
5. Creates immutable death records and Death Events.
6. Captures each Death Event's pre-check-eligible trigger seeds before any Death Event resolves.
7. Updates the death-event cache.

Forced death processing is a separate resolution plan with the rulebook's specialized aura, summon, death, and follow-up behavior.

## Stats, enchantments, and auras

### Health and damage

The model distinguishes base Health, maximum Health, current Health, damage taken, and armor. Recalculation implements the rulebook's asymmetric behavior when maximum Health rises or falls.

Mortally wounded is a derived state while the entity remains in play. `PendingDestroy` is explicit. Neither state means the entity has died before a Death Creation step.

### Enchantments

Enchantments are game entities with their own controller and play-order timestamp. An attachment relationship links them to their target. Recalculation applies them by explicit category, priority, and order of play.

Removing a temporary enchantment recalculates from base state rather than applying an inverse delta. Silence removes eligible properties and enchantments while preserving effects the rules say remain.

### Auras

Aura providers are queried at explicit aura-update steps. Derived aura applications are cached as game state until the next applicable update, allowing the engine to represent delayed aura removal and separate Health/Attack from other aura timing.

Spell Damage and any other continuously evaluated exceptions use dedicated current-state queries rather than the ordinary aura cache.

## Player-action sequences

Player actions compile into one-shot sequence operations with captured inputs, targets, and subject guards.

Required sequence builders include:

- Start and end turn
- Play spell
- Play weapon
- Play Hero card
- Play and use location
- Play minion
- Summon minion
- Combat
- Use and refresh Hero Power
- Concede

A sequence stores declared targets so targeting legality is not accidentally rechecked later. Each later phase separately checks whether its required subject remains in play, as defined by the ruleset.

Combat has dedicated Proposed Attack, Attack, and After Attack events. Redirection may append another Proposed Attack event. Preparation and combat are separate outer phases with the exceptional game-outcome check between them.

## Zones, copying, and transformation

Zone movement distinguishes:

- Generating a new entity into a full zone
- Moving an existing entity into a full zone
- Moving within the same zone
- Forward movement
- Backward movement
- Force play
- Full-zone instant removal

Reset policies are ruleset data keyed by movement type. Board position is explicit and supports non-minion board entities, adjacency, and remembered Deathrattle summon positions.

Copy effects use a zone-direction policy to decide which enchantments and runtime tags survive. Transformation removes the old in-play form and applies the new definition according to transformation type; it is not represented as death.

## Mana and costs

Mana uses signed values and separate counters for:

- Maximum resources
- Used resources
- Temporary resources
- Pending Overload
- Locked Overload
- Resources spent

Displayed values can clamp without changing internal values. Card and Hero Power cost calculations use ordered modifiers and retain negative intermediate costs where the selected ruleset requires them.

## Randomness

The simulation owns a deterministic, explicitly versioned pseudo-random generator. Random selection:

1. Builds candidates through selectors.
2. Sorts candidates deterministically.
3. Draws an index from the configured generator.
4. Records the candidate set, generator position, and selected logical ID in the trace.

Library-default RNG behavior must not silently change simulation results across dependency upgrades.

## Snapshots, choices, and traces

### Snapshots

`GameSnapshot` is a canonical observable view suitable for tests and clients. `SimulationCheckpoint` is the versioned persistence DTO containing every durable component/resource, selected ruleset and dominant player, RNG and trace state, logical counters, death state, and resolution work. A player-filtered snapshot may hide opponent information, but a checkpoint never omits hidden durable state.

Normally `Simulation::apply` returns only after the operation stack is empty. If a choice is required, the simulation enters `AwaitingChoice` and retains the untouched lower stack, prepared-event slots, pending choice, logical ID counters, and remaining budget. `Simulation::checkpoint`, JSON serialization, and `Simulation::from_checkpoint` preserve these values directly; stack payloads use logical IDs and never depend on raw Bevy entities or runtime ancestry.

Checkpoint construction converts internal Bevy relationship references such as `AttachedTo` into logical `GameEntityId` references. Restoration validates schema/ruleset versions and logical references, rebuilds entities, indexes, zones, and relationships, then checks invariants. `Simulation::fork` uses this checkpoint path, so its equivalence does not depend on action replay or raw Bevy entity allocation.

### Trace

The canonical trace records:

- Action acceptance or rejection
- Sequence and phase-plan execution
- Each popped resolution operation and its logical ID
- Event creation and ordered candidate snapshots
- Trigger activation or abortion
- RNG decisions
- Primitive mutations
- Aura and death steps
- Zone movement
- Outcome checks

Trace data supports conformance assertions, replay diagnostics, and future UI animation. In Bevy 0.19, buffered adapter notifications use Bevy Messages, while immediate targeted diagnostics may use EntityEvents and Observers. Neither is the authoritative gameplay trigger mechanism: rulebook events are immutable resolver data, pending `ResolutionOp` values define execution order, and reducers append the canonical trace before publishing any optional notification.

## Public API direction

The current hand-index and `MinionId` API should be replaced with stable entity-based actions:

```rust
GameAction::PlayCard {
    player: PlayerId,
    card: GameEntityId,
    target: Option<GameEntityId>,
    board_index: Option<usize>,
    choice: Option<ChoiceId>,
}

GameAction::Attack {
    player: PlayerId,
    attacker: GameEntityId,
    defender: GameEntityId,
}
```

Additional API capabilities should include:

```text
Simulation::apply
Simulation::legal_actions
Simulation::snapshot
Simulation::trace
Simulation::pending_choice
Simulation::fork
```

This is intentionally a breaking redesign of the scaffold API.

## Proposed module layout

```text
hearthstone_simulator/core/
├── action.rs
├── aura.rs
├── card_definition.rs
├── combat.rs
├── damage.rs
├── death.rs
├── effect.rs
├── enchantment.rs
├── entity.rs
├── event.rs
├── game.rs
├── ids.rs
├── mana.rs
├── model.rs
├── relationships.rs
├── resolver.rs
├── rng.rs
├── ruleset.rs
├── sequence.rs
├── simulation.rs
├── snapshot.rs
├── trace.rs
├── trigger.rs
└── zone.rs

hearthstone_simulator/core/tests/
├── fixtures.rs
├── resolution_test.rs
├── death_test.rs
├── sequences_test.rs
├── mechanics_test.rs
├── esoteric_test.rs
└── determinism_test.rs
```

Gazelle determines the final Bazel package layout. Subdirectories should not introduce package boundaries that prevent one Rust crate from owning its modules unless separate crates are intentional.

## Implementation milestones

### Milestone 0: Rules contract and conformance skeleton

- Pin the rulebook revision and name the initial ruleset profile.
- Create `RULEBOOK_CONFORMANCE.md` mapping normative rules to implementation and tests.
- Classify historical, current, uncertain, and card-specific behavior.
- Define core invariants and test-fixture builders.

### Milestone 1: Entity and zone foundation

- Introduce immutable logical game entity IDs with index-maintenance hooks and required `GameObject` markers.
- Move hands, decks, and boards to zone indexes.
- Add controller, position, play-order, and entity-kind components.
- Implement basic zone movement and limits.
- Migrate vanilla minion play to move the same entity from Hand to Play.

### Milestone 2: LIFO resolution work stack

- Add serializable `ResolutionOp` and `ResolutionWork` values.
- Add the exclusive pop/execute/push driver and exact-once operation contract.
- Add custom schedules, ordered system sets, and strict ambiguity checks.
- Compile public actions and phase boundaries into granular operations pushed in reverse order.
- Add choice suspension without partially executed operations.
- Add the per-sequence operation budget and stack invariants.

### Milestone 3: Events and trigger expansion

- Add immutable event records, prepared-event slots, trigger candidates, and complete ordering keys.
- Implement pre-check and queue-time candidate capture followed by reverse-order stack expansion.
- Check source eligibility and resolution-time conditions in one-shot `AttemptTrigger` operations.
- Keep executable ordering solely in the LIFO stack; do not add queue entities, cursors, or a separate queue resolver.
- Prove candidate-snapshot immutability, exact-once execution, and depth-first behavior with focused tests.

### Milestone 4: Effects and deterministic randomness

- Add selectors, conditions, values, and effect programs.
- Add the optional `NativeEffectId` to registered-system mapping with an explicit command-flush policy.
- Add primitive reducers and nested event creation.
- Add deterministic candidate ordering and RNG traces.
- Add synthetic fixtures for nested-trigger interactions.

### Milestone 5: Stats, enchantments, and auras

- Add full stat and Health modeling.
- Implement ordered enchantments, temporary removal, and silence.
- Add aura caches and both aura-update categories.
- Add continuously evaluated Spell Damage.

### Milestone 6: Damage, healing, and death

- Implement proposed damage/healing events and predamage triggers.
- Add immunity, Divine Shield, armor, prevention, and multipliers.
- Add simultaneous operation plans with pre-positioned prepared-event slots.
- Implement mortality, pending destroy, death creation, Death Phases, and the death cache.
- Delay outcome checks to ruleset-defined boundaries.

The first complete vertical slice ends here: an area effect damages several entities, nested reactions resolve, deaths are removed together, and ordered Deathrattles resolve through one or more Death Phases.

### Milestone 7: General mechanics

- Drawing, burning, fatigue, and draw triggers
- Signed mana, temporary mana, costs, and Overload
- Transformation, copying, and zone reset behavior
- Positioning and non-minion board entities
- Hero replacement, extra turns, and temporary effects

### Milestone 8: Player-action sequences

- Turn transitions
- Spell, weapon, Hero-card, location, and minion play
- Minion summoning
- Hero Power use and refresh
- Combat preparation, redirection, damage, cancellation, and completion

### Milestone 9: Esoteric compatibility

- Forced Death Phases
- Summon Resolution and Quest Reward steps
- Dominant-player and cross-zone trigger grouping
- Mid-phase removal and Deathrattle eligibility
- Special candidate-ordering and trigger conditions
- Deathrattle summon positions
- Enchantment controller rules
- Mortally wounded targeting policies

Each quirk is a named rule or policy, not an unexplained branch in a card handler.

### Milestone 10: Hardening and migration

- Migrate the CLI example and scaffold tests.
- Add full and filtered snapshots and optional Bevy Message notifications.
- Add suspended choices and `Simulation::fork`.
- Add invariant and stress tests.
- Benchmark deep and broad LIFO resolution workloads.
- Complete rulebook conformance documentation.

## Testing strategy

Every normative rule receives a focused conformance test. Ordering-sensitive tests assert both final state and the relevant canonical trace.

Required suites cover:

- Candidate-snapshot immutability and exact-once stack execution
- Trigger pre-check, queue-time, and resolution-time conditions
- Global order of play and special priority
- Nested depth-first resolution
- Simultaneous event ordering
- Aura update timing
- Mortally wounded and pending-destroy eligibility
- Delayed, simultaneous, and chained deaths
- Zone-full movement versus generation
- Damage and healing prevention and ordering
- Draw resolution and burning
- Target capture and subject removal
- Combat redirection and cancellation
- Win, loss, and draw timing
- Same-seed determinism
- Snapshot and fork equivalence
- Suspension and continuation at a choice
- Cleanup of consumed operations and prepared-event slots

Useful invariants include:

- Every game object has exactly one immutable logical ID, and the logical-ID index agrees with ECS membership.
- Every zoned entity occurs exactly once in its zone index.
- Board and hand limits are never exceeded except by an explicit ruleset exception.
- Captured candidate membership and order never change.
- Only the driver pops operations, and every popped operation executes at most once.
- An operation may push work but cannot invoke resolution recursively or remain partially executed.
- Resolution operations and prepared events never participate in game zones or order of play.
- Stack payloads and canonical output contain logical IDs, never raw Bevy entity IDs.
- An idle or complete simulation has no pending operations, event slots, or pending choice.

## Development workflow

After editing Rust source files:

```bash
bazel run //:gazelle
aspect format --scope=all
aspect test //hearthstone_simulator/...
aspect build //...
```

Run focused tests throughout a milestone, then run the full repository build before completion as required by the monorepo workflow.

## Risks and mitigations

### The rulebook is observational and mutable

Mitigation: pin source revisions, version ruleset profiles, record uncertainties in the conformance matrix, and never silently reinterpret old replay inputs.

### ECS iteration and scheduler parallelism are not gameplay ordering

Mitigation: collect candidates as values through exclusive world access, sort by complete ruleset keys, capture immutable snapshots, push operations in reverse order, and chain mutation-sensitive system sets.

### LIFO work can grow without bound

Mitigation: decrement a per-sequence budget for every popped operation, store compact logical IDs in payloads, consume prepared-event slots eagerly, and stress-test representative deep and broad workloads. Correctness and inspectability take precedence over premature optimization.

### Bevy entity IDs are not stable serialization IDs

Mitigation: assign logical IDs to game entities, events, choices, and prepared-event slots. Resolution operations persist only logical IDs, so snapshot restoration does not reconstruct an execution relationship graph.

### Complete card coverage could distort engine design

Mitigation: implement mechanics using synthetic conformance fixtures first. Add official cards only after their required primitives and timing rules exist.

### The rulebook contains historical bugs and card-specific exceptions

Mitigation: classify each item as current rule, compatibility quirk, historical behavior, uncertain observation, or card definition behavior. Keep these classifications visible in `RULEBOOK_CONFORMANCE.md`.

## Initial architectural decisions

Unless superseded by a documented decision:

1. The initial ruleset targets the pinned 2026-06-26 advanced rulebook revision.
2. Historical mechanics are excluded unless the current ruleset explicitly enables them.
3. The scaffold API may break to preserve entity identity and correct timing.
4. Bevy schedules and system sets orchestrate atomic resolution operations.
5. The sole execution mechanism is a resource-owned LIFO stack of one-shot `ResolutionOp` values.
6. Events record immutable context, preserve any ruleset-timed pre-check seeds, capture immutable ordered candidates at queue time, and expand them onto the stack in reverse order; there are no executable event or trigger queue entities.
7. Bevy Messages, EntityEvents, and Observers provide diagnostics and integration, not authoritative card-trigger ordering.
8. Game and resolution ordering never depend on raw Bevy entity or query order.
9. Resolution uses an iterative driver and per-sequence operation budget, not recursive Rust calls, execution ancestry, recursion guards, active cursors, or resumable frames.
10. Card definitions are data-oriented; exceptional native handlers are identified explicitly and may map to registered Bevy systems.
11. Simultaneous operations pre-position prepared-event slot resolvers below mutation work so all required mutations precede reactions without a separate batch queue.
12. Resolution schedules reject ECS access ambiguities; resolution operations and primitive reducers use exclusive direct mutation rather than deferred commands.
13. Checkpoints translate raw Bevy relationship references into logical IDs and restore indexes and relationships only after validating those references.
