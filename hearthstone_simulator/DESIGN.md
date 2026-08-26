# Hearthstone Simulator Design

## Status

This document is the implementation plan and target architecture for evolving the current Bevy scaffold into a deterministic, headless Hearthstone rules engine.

The initial behavioral reference is the Hearthstone Wiki advanced rulebook revision [913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067), dated 2026-06-26. The advanced rulebook is unofficial: it records observed game behavior and includes historical behavior, exceptions, and suspected bugs. The engine must therefore identify its ruleset explicitly rather than presenting its behavior as an unversioned universal definition of Hearthstone.

## Scope

### In scope

The first complete ruleset profile covers the current constructed-game mechanics described by the pinned advanced rulebook:

- Runtime entities and zones
- Sequences, phases, events, and triggers
- Immutable event and trigger queues
- Depth-first resolution
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
3. **Bevy-native state:** game and resolution state should be visible to ECS queries and processed through Bevy schedules and systems.
4. **Explicit ordering:** no gameplay result may depend on Bevy query iteration, archetype layout, raw entity IDs, or parallel scheduler order.
5. **Suspendability:** resolution can pause for a player choice and resume without losing queue or stack state.
6. **Inspectability:** tests and debugging tools can inspect active sequences, phases, events, and frozen queues.
7. **Data-oriented effects:** runtime state contains data, not closures or mutable object graphs.
8. **Searchability:** snapshots and resolution state can eventually support AI branching and replay.
9. **No hard-coded interactions:** general mechanics and ability metadata should explain interactions; exceptional behavior is isolated and named.

## Terminology

This document uses the rulebook's terms:

- A **player action** begins a sequence while the simulation is awaiting input.
- A **sequence** is an ordered plan of phases and steps for one player action or generated operation.
- A **phase** surrounds one or more events or trigger queues. Only completion of the outermost phase runs normal boundary processing.
- An **event** is a game-state change that triggers can observe.
- A **trigger** reacts to an event and may produce nested events and effects.
- A **queue** is an ordered, immutable snapshot once resolution starts.
- **Resolution** is depth-first: consequences complete before processing the next queued sibling.
- A **game entity** is an object meaningful to Hearthstone, such as a card, minion, Hero, weapon, enchantment, or hidden effect.
- A **resolution node** is an engine-internal ECS entity representing active resolution. Resolution nodes do not participate in Hearthstone zones or order of play.

## High-level architecture

```text
Simulation API
    |
    v
Action validation schedule
    |
    v
Sequence root resolution entity
    |
    v
Exclusive resolution driver
    |
    +--> ResolveFrame schedule
    |       +--> sequence/phase/event/trigger/effect systems
    |
    +--> ResolvePhaseBoundary schedule
            +--> aura, summon, death, and outcome systems

Game ECS entities <---- primitive effect reducers ---- Resolution ECS entities
       |                                               |
       +--------------- canonical trace ---------------+
```

Simulator-owned domain entities fall into two deliberately separate categories:

- `GameObject` entities hold durable game state.
- `ResolutionNode` entities hold temporary or suspended resolution state.

These are not the only Bevy entities in the world. In Bevy 0.19, resources occupy singleton entities, and registered systems, observers, and other framework facilities may also own entities. Systems and snapshots must therefore select simulator entities through `GameObject` and `ResolutionNode` rather than iterating every world entity or assuming that every unmarked entity is gameplay state.

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

Table storage remains the default. Frequently added or removed marker components, such as `PendingDestroy` and short-lived resolution tags, may use Bevy sparse-set storage after benchmarks confirm that avoiding archetype moves outweighs slower iteration. Value components such as `Zone` and `ResolutionState` remain table-stored.

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

A monotonic `PlayOrderCounter` assigns timestamps whenever rules say an entity enters play or an attached enchantment establishes its own order. Queue ordering uses these values plus ruleset-specific priority and player/zone grouping.

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

The normal Bevy `Update` schedule accepts at most one pending player action and invokes an exclusive resolution driver. The driver runs the custom schedules until resolution finishes, pauses, the game ends, or a configured safety budget is exhausted.

The driver may invoke `ResolveFrame` and `ResolvePhaseBoundary` because they are different from the currently running `Update` schedule. It must not recursively invoke the same schedule label; Bevy temporarily removes a schedule from the world while running it.

### Schedule guardrails

Resolution schedules are configured strictly:

```rust
ScheduleBuildSettings {
    ambiguity_detection: LogLevel::Error,
    hierarchy_detection: LogLevel::Error,
    auto_insert_apply_deferred: false,
    ..default()
}
```

They also disable final deferred application with `Schedule::set_apply_final_deferred(false)`. Consequently, unordered conflicting systems fail schedule construction, and no command buffer becomes visible merely because a schedule or ordered dependency happens to end. Every deferred synchronization point must appear explicitly in the configured pipeline.

A single-threaded executor may reduce overhead for small, mostly exclusive resolution schedules, but it is a benchmark-driven implementation choice, not a determinism guarantee. Gameplay order remains explicit even if independent discovery systems execute in parallel.

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
    QueueDeathPhase,
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
        PhaseBoundarySet::QueueDeathPhase,
    )
        .chain(),
);
```

The exact pipeline is ruleset data and must be confirmed against the pinned rulebook during the conformance milestone.

### Deferred commands

Exact mutation timing is essential. Resolution schedules should therefore use exclusive systems and direct `World` mutation for resolution-node creation and primitive game mutations. Ordinary deferred `Commands` are suitable only when an explicit `ApplyDeferred` boundary is part of the designed timing.

Queue candidate discovery is the primary safe use: independent systems may enqueue candidate entities in parallel, followed by an explicit `ApplyDeferred` and then an exclusive freeze system. Deferred insertion order is irrelevant because the freeze system computes complete ordering keys and sorts after every candidate is visible. Primitive reducers do not use this pattern unless the rulebook explicitly defines a simultaneous collection boundary.

## Resolution graph

### Frames as ECS entities

The resolution stack is modeled as an active path through ECS relationships rather than an opaque Rust call stack.

```rust
#[derive(Component)]
struct ResolutionNode;

#[derive(Component)]
#[component(immutable)]
struct ResolutionIdentity {
    id: ResolutionId,
    kind: ResolutionKind,
}

#[derive(Component)]
struct ResolutionState {
    progress: ResolutionProgress,
}

#[derive(Component)]
#[component(immutable)]
#[relationship(relationship_target = NestedFrames)]
struct NestedUnder(Entity);

#[derive(Component)]
#[relationship_target(relationship = NestedUnder, linked_spawn)]
struct NestedFrames(Vec<Entity>);
```

Representative node kinds are:

```text
Sequence
Phase
EventBatch
Event
EventQueue
TriggerQueue
Trigger
Effect
PhaseBoundary
DeathPhase
Choice
```

A small resource points to the active leaf:

```rust
#[derive(Resource, Default)]
#[component(map_entities)]
struct ResolutionCursor {
    root: Option<Entity>,
    active: Option<Entity>,
    remaining_budget: usize,
}
```

`ResolutionCursor` implements `MapEntities` for its raw entity fields. Components that contain raw entity fields use `#[entities]` where the derive supports it. This allows Bevy cloning and tooling to remap internal references, while canonical serialization still converts them to logical resolution IDs.

The relationship ancestry is the logical stack:

```text
Sequence
└── Phase
    └── Event
        └── Trigger
            └── Effect
```

Pushing a frame spawns a `ResolutionNode` related to the current active frame and changes `ResolutionCursor.active`. Popping reads `NestedUnder`, completes or removes the current node, and restores its parent as active.

Completed nodes may be retained until the sequence ends for diagnostics, then removed through linked relationship cleanup. The canonical trace remains after resolution entities are cleaned up. Bevy relationship helpers such as ancestor, root-ancestor, and depth-first descendant traversal support diagnostics and graph-invariant checks; the active cursor remains authoritative for execution.

### Iterative depth-first resolution

The exclusive driver advances one active frame at a time:

```rust
fn drive_resolution(world: &mut World) {
    while resolution_can_advance(world) {
        match active_resolution_kind(world) {
            ResolutionKind::PhaseBoundary => world.run_schedule(ResolvePhaseBoundary),
            _ => world.run_schedule(ResolveFrame),
        }
    }
}
```

Nested effects push children. The next driver iteration processes the new active child before returning to its parent, producing depth-first resolution without recursive Rust calls.

A per-sequence resolution budget prevents pathological or accidental infinite loops from exhausting memory or CPU. Budget exhaustion is a typed simulation error containing the active resolution trace.

### Phase depth

Each phase records its nesting depth or enough ancestry to determine whether it is outermost. Completing a nested phase returns directly to its parent. Completing the outermost phase creates a `PhaseBoundary` node before the sequence advances.

Forced death phases use an explicit specialized boundary plan and do not pretend to be ordinary outermost phases.

## Bevy-modeled queues

### Queue entities and relationships

Every event or phase requiring ordered responses creates a queue resolution entity:

```rust
#[derive(Component)]
#[component(immutable)]
struct ResolutionQueue(QueueKind);

#[derive(Component)]
enum QueueState {
    Collecting,
    Frozen,
    Resolving,
    Complete,
}

#[derive(Component)]
#[component(immutable)]
#[relationship(relationship_target = QueueEntries)]
struct QueuedIn(Entity);

#[derive(Component)]
#[relationship_target(relationship = QueuedIn, linked_spawn)]
struct QueueEntries(Vec<Entity>);
```

Queue entries are ECS entities with typed payload components. A trigger entry contains at least:

```rust
#[derive(Component)]
#[component(immutable)]
struct QueuedTrigger {
    source: GameEntityId,
    event: ResolutionId,
    order: TriggerOrderKey,
}

#[derive(Component)]
enum QueueEntryStatus {
    Pending,
    Resolving,
    Resolved,
    Aborted,
}
```

An event entry similarly references its event resolution node and event ordering data.

### Ordering keys

Queue order is always explicit:

```rust
struct TriggerOrderKey {
    player_bucket: u8,
    zone_bucket: u8,
    priority: i16,
    play_order: u64,
    source: GameEntityId,
    tie_breaker: u32,
}
```

The selected ruleset computes these fields. The stable logical source ID precedes the per-source definition tie-breaker, so equal play-order values outside the battlefield never fall back to ECS iteration order. The key can therefore represent normal global order of play, special trigger priority, dominant-player grouping, battlefield/hand/deck grouping, and deterministic ties without changing scheduler configuration.

### Queue lifecycle

#### Collecting

Candidate-discovery systems query eligible game entities and spawn related queue-entry entities. Collection checks pre-check and queue-time conditions but does not run trigger effects. Discovery may use deferred commands and parallel systems only in a pipeline with an explicit `ApplyDeferred` before freezing.

Only queues in `Collecting` state accept entries.

#### Frozen

An exclusive freeze system:

1. Reads all related entries.
2. Sorts them by the applicable explicit key.
3. Stores the ordered entity IDs in a frozen snapshot.
4. Changes the queue state to `Frozen`.

```rust
#[derive(Component)]
#[component(immutable)]
struct FrozenQueueEntries {
    #[entities]
    entries: Vec<Entity>,
}

#[derive(Component)]
struct QueueCursor(usize);
```

The immutable frozen vector intentionally duplicates the current relationship ordering. Bevy relationships continue to describe ownership, while `FrozenQueueEntries` is the authoritative rulebook queue and `QueueCursor` is its separately mutable progress. Later entity creation or relationship changes cannot mutate frozen membership through an ordinary mutable query.

#### Resolving

The queue reads the entry at `cursor`, checks source eligibility and resolution-time trigger conditions, and pushes a child `Trigger` frame. The queue does not advance to its next sibling until that child and all descendants complete.

An invalid queued trigger is marked `Aborted`; it is not removed from or replaced in the frozen queue.

#### Complete

After the final entry, the queue is marked complete and its parent frame resumes. Queue entries are cleaned up with their queue owner after trace information is recorded.

### Immutability enforcement

Queue collection and freezing are separate system sets with an explicit deferred-application boundary when collection uses commands. Candidate systems query only collecting queues. Bevy's immutable-component access prevents in-place mutation of frozen membership; internal replacement APIs reject frozen queues, and invariant tests assert that:

- Frozen entry membership never changes
- Frozen order never changes
- Newly created game entities cannot respond to the event currently resolving
- Removed trigger sources leave their queued entry in place but may abort when selected

## Events and triggers

### Event context

An event resolution node records stable data needed throughout resolution:

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

An effect that creates simultaneous damage, healing, movement, or death events first creates all event nodes, orders them according to the applicable rule, and freezes an event queue. Each event then resolves fully before its next sibling begins.

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
- Self-trigger and repeated-event safeguards
- Effect program

This separation is required for interactions such as Secrets that queue but later abort, and effects that must have been valid when a sequence began.

### Re-entrancy

Each event has a stable logical ID. Trigger execution tracks trigger/event pairs so one trigger cannot respond twice to the same event unless explicitly allowed. A separate active-trigger guard prevents unsupported direct self-nesting. Any rulebook-compatible deferred compensation behavior must be added as a named policy with focused tests.

## Primitive reducers

Only primitive reducers mutate durable game state. Initial primitives include:

```text
MoveEntity
GenerateEntity
ChangeController
SpendResource
GainResource
Draw
DealDamage
Heal
Destroy
Summon
Transform
AttachEnchantment
DetachEnchantment
Silence
EquipWeapon
ReplaceHero
RefreshHeroPower
```

A primitive may create nested event nodes. For example, `DealDamage` creates a proposed damage event, processes prevention and predamage triggers, applies the final amount, and then resolves damage reactions.

Reducers append canonical trace entries for all observable changes.

## Phase-boundary processing

The ordinary outermost phase boundary is modeled as ordered systems, not hidden cleanup:

1. Health/Attack aura update
2. Quest reward step, where applicable
3. Summon resolution step
4. Second Health/Attack aura update
5. Death creation step
6. Other aura update
7. Creation of a Death Phase when deaths were recorded

Death Phases are ordinary outermost phase nodes and can be followed by additional Death Phases until no new deaths exist.

The Death Creation system:

1. Finds all mortally wounded and pending-destroy entities eligible to die.
2. Orders them by the ruleset's event order.
3. Captures controller and remembered board position.
4. Removes them from play without running intervening triggers.
5. Creates immutable death records and Death Events.
6. Updates the death-event cache.

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

Player actions compile into sequence resolution entities with captured inputs, targets, and subject guards.

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

A canonical full snapshot includes all durable game state and the selected ruleset and RNG state. A player-filtered view may hide opponent information for clients.

Normally `Simulation::apply` returns only after the sequence resolves. If a choice is required, the simulation enters `AwaitingChoice` and retains the active resolution graph and frozen queues. Snapshot support for suspended resolution must serialize logical resolution IDs and reconstruct Bevy relationships without depending on raw Bevy entities.

Internal components and resources that contain Bevy entity references implement `MapEntities`, allowing Bevy's entity-cloning and relationship tooling to remap references in fixtures and in-memory utilities. `Simulation::fork` may reuse those facilities after profiling, but its observable equivalence is defined by the canonical logical-ID snapshot rather than Bevy world serialization or raw entity allocation.

### Trace

The canonical trace records:

- Action acceptance or rejection
- Sequence, phase, and frame begin/end
- Event creation
- Queue collection and frozen order
- Trigger resolution or abortion
- RNG decisions
- Primitive mutations
- Aura and death steps
- Zone movement
- Outcome checks

Trace data supports conformance assertions, replay diagnostics, and future UI animation. In Bevy 0.19, buffered adapter notifications use Bevy Messages, while immediate targeted diagnostics may use EntityEvents and Observers. Neither is the authoritative gameplay trigger mechanism: rulebook events are resolution entities, and reducers append the canonical trace before publishing any optional notification.

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
├── queue.rs
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

### Milestone 2: Bevy resolution graph

- Add resolution-node components and custom relationships.
- Add `ResolutionCursor` and the exclusive driver.
- Add custom schedules, ordered system sets, strict ambiguity checks, and explicit deferred boundaries.
- Add entity-reference remapping for the resolution cursor and relationship-bearing components.
- Support push, suspend, resume, complete, and cleanup.
- Add resolution budget and graph invariants.

### Milestone 3: Event and trigger queues

- Add queue and entry entities with ownership relationships.
- Implement candidate collection, explicit ordering, immutable frozen membership, and cursor-based resolution.
- Separate immutable frozen entries from mutable queue progress.
- Add event context, trigger definitions, and condition timings.
- Prove queue immutability and depth-first behavior with focused tests.

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
- Add simultaneous event batches.
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
- Special queue and trigger conditions
- Deathrattle summon positions
- Enchantment controller rules
- Mortally wounded targeting policies

Each quirk is a named rule or policy, not an unexplained branch in a card handler.

### Milestone 10: Hardening and migration

- Migrate the CLI example and scaffold tests.
- Add full and filtered snapshots and optional Bevy Message notifications.
- Add suspended choices and `Simulation::fork`.
- Add invariant and stress tests.
- Benchmark deep and broad resolution graphs.
- Complete rulebook conformance documentation.

## Testing strategy

Every normative rule receives a focused conformance test. Ordering-sensitive tests assert both final state and the relevant canonical trace.

Required suites cover:

- Queue immutability
- Queue pre-check, queue-time, and resolution-time conditions
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
- Suspension and resumption at a choice
- Cleanup of linked resolution entities

Useful invariants include:

- Every game object has exactly one immutable logical ID, and the logical-ID index agrees with ECS membership.
- Every zoned entity occurs exactly once in its zone index.
- Board and hand limits are never exceeded except by an explicit ruleset exception.
- Frozen queue membership and order never change.
- The active resolution entity is the leaf of its active ancestry.
- Resolution entities never participate in game zones or order of play.
- No canonical output contains raw Bevy entity IDs.
- An idle or complete simulation has no live resolution root.

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

Mitigation: collect candidates into explicit queue entries, sort by complete ruleset keys, freeze queues, and chain mutation-sensitive system sets.

### Fully ECS-modeled resolution creates entity churn

Mitigation: benchmark representative deep and broad graphs, clean linked nodes at sequence completion, and optimize only after conformance. Correctness and inspectability take precedence initially.

### Bevy entity IDs are not stable serialization IDs

Mitigation: assign logical IDs to both game and resolution entities and remap relationships during snapshot restoration.

### Deferred operations can shift timing

Mitigation: disable automatic and final deferred application in resolution schedules, use direct `World` mutation in exclusive resolution systems, and place explicit `ApplyDeferred` steps where deferred behavior is intended. Native registered handlers either avoid commands or document their immediate post-handler flush as part of the reducer boundary.

### Complete card coverage could distort engine design

Mitigation: implement mechanics using synthetic conformance fixtures first. Add official cards only after their required primitives and timing rules exist.

### The rulebook contains historical bugs and card-specific exceptions

Mitigation: classify each item as current rule, compatibility quirk, historical behavior, uncertain observation, or card definition behavior. Keep these classifications visible in `RULEBOOK_CONFORMANCE.md`.

## Initial architectural decisions

Unless superseded by a documented decision:

1. The initial ruleset targets the pinned 2026-06-26 advanced rulebook revision.
2. Historical mechanics are excluded unless the current ruleset explicitly enables them.
3. The scaffold API may break to preserve entity identity and correct timing.
4. Bevy schedules and system sets orchestrate resolution.
5. The resolution stack is an ECS relationship graph with a small active-cursor resource.
6. Event and trigger queues are ECS entities whose ordered membership is frozen explicitly.
7. Bevy Messages, EntityEvents, and Observers provide diagnostics and integration, not authoritative card-trigger ordering.
8. Game and resolution ordering never depend on raw Bevy entity or query order.
9. Resolution uses an iterative driver and safety budget, not recursive Rust calls.
10. Card definitions are data-oriented; exceptional native handlers are identified explicitly and may map to registered Bevy systems.
11. Write-once identity and frozen queue membership use immutable components; mutable queue progress is stored separately.
12. Resolution schedules reject ECS access ambiguities and expose deferred mutations only at explicit boundaries.
13. Raw Bevy entity references implement remapping for internal cloning, but canonical persistence uses logical IDs.
