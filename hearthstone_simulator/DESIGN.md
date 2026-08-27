# Hearthstone Simulator Design

## Status

This document defines the target architecture for a deterministic, headless Hearthstone simulator built on Bevy 0.19 ECS. It supersedes the previous design's ECS-modeled resolution stack and frozen queue entities. The current implementation may retain those structures during migration, but they are not the target execution model.

The target design instead uses:

- Flat ECS game state with persistent entity identity
- Required components for entity-shape invariants
- Child entities for enchantments and other attached modifiers
- Layered stat and cost recomputation
- Immediate Bevy `EntityEvent`/Observer dispatch for nested game reactions
- A deterministic phase coordinator and custom schedules for rulebook ordering
- A headless core plugin that can be embedded in separate simulation, AI, and presentation applications
- Efficient state cloning for search and parallel playouts

The initial behavioral reference remains Hearthstone Wiki advanced rulebook revision [913067](https://hearthstone.wiki.gg/wiki/Advanced_rulebook?oldid=913067), dated 2026-06-26. The advanced rulebook is observational and includes historical behavior, exceptions, and suspected bugs. Every simulation therefore identifies its ruleset profile explicitly.

## Motivation

Existing Hearthstone simulators such as Sabberstone and Fireplace demonstrate that the game can be modeled accurately with object-oriented entities, attached behavior objects, and explicit event dispatch. Those architectures are useful references, but they organize mutable state as interconnected object graphs. Deep copies, repeated whole-board scans, and cross-language integration can become expensive in workloads such as Monte Carlo Tree Search (MCTS) and reinforcement learning.

Bevy offers a different organization:

- **Entities** provide identity.
- **Components** hold small pieces of state.
- **Systems** implement rules over matching component sets.
- **Relationships** connect attachments without embedding mutable object graphs.
- **Schedules and Observers** provide explicit execution boundaries and immediate nested reactions.
- **World isolation** prevents one speculative simulation from mutating another.

The purpose of using ECS is not merely to translate classes into components. The design should make frequently queried state contiguous, keep static card data out of cloned worlds, and expose enough structure for deterministic tests, replay, and high-throughput search.

## Scope

### In scope

The first complete ruleset profile covers the current constructed-game mechanics described by the pinned advanced rulebook:

- Persistent runtime entities and ordered zones
- Player actions, sequences, phases, events, and triggers
- Immediate nested event resolution under deterministic phase control
- Explicit order of play, trigger priority, player order, and zone order
- Layered enchantment, aura, stat, and cost calculation
- Damage, healing, mortality, Death Creation, Death Events, and chained death processing
- Start/end turn, card play, combat, location, and Hero Power sequences
- Minions, spells, weapons, Hero cards, locations, permanents, and Dormants
- Silence, transformation, copying, controller changes, and zone resets
- Drawing, fatigue, mana, Overload, costs, Hero replacement, and outcomes
- Seeded random effects
- Canonical snapshots, traces, state cloning, and suspended choices
- Headless execution and optional presentation or language-binding adapters

### Out of scope

The following remain separate projects or adapters:

- A complete official card database
- Rendering, animation, and user-interface policy
- Wall-clock turn timers and animation slush time
- Battlegrounds and Mercenaries rules
- Blizzard client/server protocol compatibility
- Historical behavior not enabled by the selected ruleset profile
- A specific MCTS, neural-network, or reinforcement-learning implementation

Synthetic card definitions are preferred while the engine mechanics are being validated.

## Design goals

1. **Rules fidelity:** phase, trigger, aura, and death timing must directly represent the selected rulebook profile.
2. **Determinism:** the same ruleset, initial snapshot, actions, and random seed produce the same final snapshot and trace.
3. **Data-oriented state:** dynamic game state lives in components; static card definitions are shared and immutable.
4. **Persistent identity:** moving or transforming a card does not invalidate pending references to that game object.
5. **Explicit ordering:** gameplay never depends on Bevy query order, Observer registration order, archetype layout, raw entity IDs, or scheduler parallelism.
6. **Immediate nesting:** a reaction caused by an event resolves before the interrupted parent operation continues when the rulebook requires depth-first behavior.
7. **Headless operation:** the core rules engine has no dependency on windows, graphics, audio, or wall-clock frame pacing.
8. **Fast isolation:** speculative worlds can be copied and advanced independently for tests and search.
9. **Inspectability:** snapshots and traces explain phase transitions, event dispatch, modifier layers, mutations, random choices, and outcomes.
10. **Extensibility:** reusable data describes ordinary cards; exceptional native behavior is explicit and constrained.

## Terminology

- A **game object** is a card, Hero, Hero Power, minion, weapon, location, enchantment, or other object with runtime identity.
- A **definition** is immutable card or ability metadata shared by many worlds.
- A **player action** is validated input accepted while the simulation is awaiting input.
- A **sequence** is the rulebook plan produced from an accepted action or generated operation.
- A **phase** is a deterministic section of a sequence with a defined completion boundary.
- An **event** is an immutable description of something proposed or performed in the game.
- A **trigger** is source-owned behavior eligible to react to an event.
- An **Observer** is the Bevy mechanism used to execute an event dispatch immediately.
- The **phase coordinator** is simulator-owned state that controls sequences, ordered trigger dispatch, phase completion, and quiescence loops.
- The **onion** is the ordered set of layers used to derive stats and costs from base values, enchantments, set effects, auras, and self-modifiers.

## High-level architecture

```text
                         immutable card definitions
                                    |
                                    v
Simulation API --> action validation and sequence construction
                                    |
                                    v
                         deterministic phase coordinator
                                    |
                  +-----------------+-----------------+
                  |                                   |
                  v                                   v
       custom phase schedules                 immediate EntityEvents
                  |                                   |
                  v                                   v
       aura/stat/death systems <---------- ordered Observer dispatch
                  |
                  v
         canonical trace and snapshot

Headless runner / MCTS / tests                Presentation application
              |                                       |
              +------------ CoreRulesPlugin ----------+
```

The core is a Bevy plugin containing only rules state, systems, custom schedules, event types, and deterministic resources. A headless simulation app installs that plugin with minimal Bevy facilities and advances it explicitly. A presentation app may install the same plugin alongside rendering, input, and UI plugins.

The phase coordinator owns progression. Observers execute nested reactions, but Observer registration order never defines game order. Before dispatch, the coordinator discovers eligible trigger sources, computes complete ordering keys, and invokes them one at a time.

## ECS game-state model

### One entity per runtime object

Every card in a deck, Hero, Hero Power, weapon, location, enchantment, and other stateful game object is represented by a Bevy entity. A card is not destroyed and recreated merely because it moves from Deck to Hand or Hand to Play.

Each game object also receives a stable logical ID:

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

The Bevy `Entity` remains valid within one world. `GameEntityId` is used by public actions, snapshots, traces, and cross-world references. A resource indexes logical IDs to local Bevy entities. Creation validates uniqueness before insertion, while component hooks maintain the index and assert internal invariants.

No canonical output contains a raw Bevy entity ID.

### Required components

Primary concept components use Bevy required components to establish deterministic structural defaults. For example, inserting a minion-form component can require game-object identity markers, zone state, base stats, computed stats, health state, and controller state.

Required-component constructors may create constant defaults. They must not:

- Allocate logical IDs
- Read mutable ruleset state
- Resolve a card definition
- Choose a controller or zone implicitly
- Perform gameplay mutations

Validated spawn functions remain responsible for those operations.

Bundles may still be useful for fixtures, but required components are the primary mechanism for expressing entity-shape invariants.

### Component categories

Representative components include:

```text
Identity and definition
  GameObject
  GameEntityId
  DefinitionId
  EntityKind
  Controller

Location and order
  Zone
  ZonePosition
  PlayOrder

Stats and resources
  BaseStats
  ComputedStats
  HealthState
  Armor
  Durability
  ResourceState

Behavior
  AbilitySet
  AuraEmitter
  TriggerSource

Transient state
  PendingDestroy
  Exhausted
  SummoningSick
```

Table storage is the default. Frequently toggled marker components may use sparse-set storage only after profiling shows a benefit.

### Keyword markers

Frequently queried binary keywords are represented as zero-sized marker components, for example:

```text
Taunt
DivineShield
Poisonous
Immune
Stealth
Windfury
```

Card definitions may still store a compact keyword list for loading and inspection. Materializing a runtime form inserts the corresponding markers. Systems can then express eligibility through ECS query filters instead of repeatedly decoding a general-purpose bitset.

Keywords whose behavior includes counters, provenance, or parameters use data components rather than zero-sized markers.

### Zones and authoritative ordering

`Zone` identifies Deck, Hand, Play, Secret, Graveyard, SetAside, or RemovedFromGame. Controller and zone are separate because control can change without ordinary zone movement.

Ordered zone indexes are authoritative for:

- Deck order
- Hand order
- Board position
- Secret order
- Remembered positions used by later effects

Bevy query iteration discovers matching objects but never defines positional order. A monotonic play-order counter assigns timestamps whenever the ruleset says an object or attached enchantment establishes order of play.

Movement preserves both the Bevy entity and `GameEntityId` unless the rule explicitly generates a new object. Zone transitions apply named reset policies for tags, damage, enchantments, visibility, and controller state.

## Static card definitions

Static `CardDefinition` data is stored outside dynamic game entities and shared across simulation worlds. Definitions contain:

- Card type, class, tribe, and printed tags
- Base cost and stats
- Runtime keyword markers to materialize
- Targeting requirements
- Trigger and aura definitions
- Battlecry, Deathrattle, spell, location, Hero Power, and other effect programs

Ordinary card behavior is cloneable data:

```text
Effect
Selector
Condition
ValueExpression
TriggerDefinition
AuraDefinition
ModifierDefinition
```

Effects request primitive operations such as damage, healing, movement, summoning, drawing, transformation, and enchantment attachment. They do not receive unrestricted mutable access to the world.

A stable `NativeEffectId` may identify exceptional behavior that cannot reasonably be represented by the common effect language. Native handlers are registered by the core plugin, produce ordinary effect plans, and may not bypass event, trace, phase, or mutation policy.

## Health and derived stats

### Base and computed values

Printed values and current derived values are separate:

```text
BaseStats      printed attack and health
ComputedStats  attack, maximum health, and other reduced values
HealthState    current health and derived damage status
```

Damage and health changes must preserve Hearthstone's asymmetric maximum-health behavior:

1. Increasing maximum Health increases current Health by the same amount.
2. Decreasing maximum Health leaves current Health unchanged unless it exceeds the new maximum, in which case it is clamped.
3. Damage is derived consistently from the resulting maximum and current Health.
4. Mortality is evaluated from the post-recalculation current Health at the designated Death Creation boundary.

This allows a temporary Health increase to function as healing after it expires. For example, a 2/2 at 1 Health becomes a 3/2 with a maximum of 3 Health after receiving +1/+1; removing that effect leaves it at 2 of 2 Health, so it returns to an undamaged 2/2.

The ruleset defines exact behavior for set-Health effects, damage counters, Hero armor, immunity, Divine Shield, and historical exceptions.

### Transformation

Transformation changes the form of the same runtime object. It does not despawn the target and spawn an unrelated replacement.

The transformation reducer:

1. Captures the target entity and logical identity.
2. Removes form-specific markers, abilities, and runtime tags.
3. Despawns attached enchantment children that do not survive the transformation policy.
4. Clears or resets damage and computed modifier state as required.
5. Replaces the definition reference and inserts the new form's required components and keyword markers.
6. Recomputes derived values.
7. Emits the appropriate trace and transformation events without creating a death.

Pending event targets therefore continue to refer to the same object and observe its transformed form when they resolve.

## Enchantments and the onion system

### Enchantments as child entities

An enchantment is a distinct ECS entity parented to its target with Bevy's `ChildOf` relationship. It contains data such as:

```text
EnchantmentDefinition
ModifierLayer
ModifierOperation
ApplicationOrder
Controller
Duration
RemovalPolicy
```

This avoids embedding mutable modifier lists inside cards and makes attachment lifetime visible to ECS queries. Despawning or silencing an attachment follows explicit policy; it is never inferred solely from hierarchy cleanup.

### Layer model

Derived values are rebuilt from source data rather than maintained through inverse deltas.

| Layer | Source                           | Example                                    |
| ----- | -------------------------------- | ------------------------------------------ |
| 0     | Base value                       | A printed 4/5 minion                       |
| 1     | Permanent/non-aura enchantments  | Gain +4/+4                                 |
| 2     | Set effects                      | Set Attack to 1                            |
| 3     | Applicable auras                 | Adjacent minions have +1 Attack            |
| 4     | Ruleset-defined self adjustments | Cost reduction based on the owner's Health |

Within a layer, modifiers sort by complete deterministic keys, normally including application timestamp, source logical ID, and definition-local tie-breaker. A set operation establishes a new value for subsequent modifiers according to the ruleset's layer policy.

Stats and costs may use different layer tables. Interactions such as multiple cost-setting auras and a card's self-cost adjustment are represented by ordered policy, not card-name conditionals.

### Recalculation systems

A custom recalculation schedule:

1. Discovers entities whose inputs changed.
2. Reads their base values and attached modifier children.
3. Discovers currently applicable aura emitters.
4. Sorts all modifiers by layer and deterministic order.
5. Reduces a new `ComputedStats` or computed cost.
6. Applies maximum-Health transition rules.
7. Records meaningful changes in the trace.

Independent targets may be recalculated in parallel. Parallelism does not affect results because every target uses immutable inputs for that pass and a complete local ordering key.

### Auras

An aura source has an `AuraEmitter` component describing its selector and modifiers. Aura applicability is recomputed at explicit rulebook boundaries or when a dirty-state policy schedules an equivalent update.

Ordinary auras need not create permanent attachment entities on every target. Their applicable modifiers may be collected transiently during recomputation. If a rules interaction requires an aura application to have runtime identity or delayed removal, the engine materializes an explicit derived aura entity.

Silencing a target removes eligible target-owned enchantments and abilities, but does not remove a modifier supplied by an unsilenced external aura source. Silencing or removing the source changes applicability at the next designated aura update.

## Events, Observers, and deterministic dispatch

### Immediate events are authoritative

Rulebook events are represented by immutable Bevy `EntityEvent` values and handled through Observers. Buffered Bevy Messages may notify adapters, telemetry, or UI, but they are not the authoritative mechanism for nested card reactions.

Immediate dispatch is required for chains such as:

```text
play card
  -> Battlecry
    -> damage
      -> damage reaction
        -> generated effect
```

When event dispatch is queued through `Commands`, the phase schedule contains an explicit command-application point. Nested event commands produced by Observers are drained before the parent phase advances. Direct immediate triggering may be used when the reducer already has exclusive world access.

### Observers do not define trigger order

Bevy does not guarantee the gameplay order needed when several Observers can react to the same event. The simulator therefore separates **candidate ordering** from **Observer execution**:

1. The event manager captures an immutable event envelope with a stable event ID.
2. It queries all potentially eligible trigger sources.
3. It evaluates pre-check and queue-time conditions.
4. It computes a complete trigger ordering key.
5. It stores the ordered candidates in coordinator state.
6. It dispatches one targeted trigger event.
7. That trigger and all nested events resolve before the next candidate is dispatched.
8. Resolution-time conditions may abort the selected candidate without reordering the remainder.

Representative ordering keys include dominant-player bucket, zone bucket, explicit priority, play order, source logical ID, and definition-local tie-breaker. Raw `Entity` values and query order are never tie-breakers.

This keeps Bevy Observers as the execution mechanism while centralizing Hearthstone ordering in simulator-owned data.

### Component hooks

Hooks maintain local structural invariants such as logical-ID indexes and dirty markers. Although Bevy defines lifecycle ordering between hooks and Observers, card mechanics must not depend on incidental component insertion/removal order.

A gameplay operation that needs an event uses an explicit reducer and event dispatch. A hook must not silently stand in for a Summon, Death, Play, or zone-change event.

### Re-entrancy and recursion safety

Every event receives a stable logical ID. Trigger execution records event/source/trigger-definition tuples so a trigger cannot respond more than once unless its policy allows that behavior. An active-trigger guard handles prohibited direct self-nesting.

Immediate nested dispatch is logically recursive, but the engine enforces a per-action event and mutation budget. Exhaustion produces a typed error with the active event trace. Where Bevy's command draining would create an unbounded Rust call chain, the coordinator may use an explicit pending stack while preserving the same depth-first semantics.

### Simultaneous events

Simultaneous damage, healing, movement, or death requests first receive deterministic request order. The relevant reducer applies proposal and mutation rules exactly as specified by the ruleset. Reactions that must wait for the whole batch are captured and dispatched only after all batch mutations complete.

The event envelope records:

```text
Event ID
Event kind
Source
Targets
Controller
Proposed and actual values
Sequence subject
Batch ID and ordinal
Creation context
```

Batch handling is explicit phase-coordinator state, not an accidental consequence of deferred commands.

## Custom schedules and phase coordination

### Why a custom schedule is required

The normal real-time `Update` schedule cannot define Hearthstone timing. The simulator advances only when input is accepted or an in-progress sequence can continue. A custom rules schedule provides explicit synchronization points for event dispatch, aura recomputation, death creation, and phase completion.

The core lifecycle is:

```text
SettingUp
AwaitingAction
Resolving
AwaitingChoice
Complete
```

This lifecycle may use Bevy States or an ordinary resource. Individual rulebook phases are coordinator data, not global Bevy States.

### Phase coordinator

The coordinator stores only execution data needed to continue deterministically:

```text
Current sequence and phase
Captured action inputs and targets
Pending ordered event/trigger work
Active batch context
Nested dispatch stack
Pending player choice
Remaining safety budget
```

It is serializable through logical IDs so a choice can suspend and resume, and so a speculative world can be cloned at a stable boundary.

### Phase pipeline

A typical phase-completion loop is:

1. **Event dispatch:** commit the next sequence operation or ordered trigger.
2. **Nested reaction drain:** run immediate Observers and explicit command sync points until the current reaction chain is quiescent.
3. **Aura/stat update:** recompute applicable layers and derived Health/Attack values.
4. **Death Creation:** find all eligible mortally wounded or pending-destroy characters and move them out of play as one ruleset-defined batch.
5. **Death dispatch:** emit ordered Death Events and resolve their nested reactions.
6. **Loop verification:** repeat required aura/death processing while new work exists.
7. **Phase completion:** advance the sequence or return to `AwaitingAction`.

The full advanced-rulebook boundary can refine this into ordered system sets such as:

```text
Health/Attack aura update
Quest reward step
Summon Resolution step
Second Health/Attack aura update
Death Creation step
Other aura update
Death Event and Deathrattle processing
```

Forced Death Phases use their own named plan. They do not reuse the ordinary plan if the rulebook gives them different aura or summon timing.

### Death Creation

An entity at zero or lower Health is mortally wounded but remains in play until Death Creation. Before collecting deaths, the designated aura update runs, so a newly applicable Health aura can save an otherwise mortally wounded minion.

Death Creation:

1. Queries all mortally wounded and pending-destroy eligible characters.
2. Sorts them by the ruleset's simultaneous-death order.
3. Captures controller, play order, remembered board position, and turn metadata.
4. Moves the entire batch out of play without interleaving Deathrattles.
5. Creates immutable death records.
6. Dispatches ordered Death Events after batch removal.
7. Repeats the aura/death loop if reactions create new deaths.

Hero defeat becomes irreversible at the ruleset-defined Death Creation point. Outcome checks occur only at named sequence or phase boundaries.

### Schedule guardrails

Mutation-sensitive system sets are chained explicitly. Ambiguity detection is enabled for custom schedules, and deferred commands become visible only at deliberate synchronization points.

System parallelism is an optimization for independent discovery or recalculation. It never supplies gameplay ordering. Exclusive coordinator systems are acceptable where they simplify immediate nested dispatch and deterministic mutation.

## Primitive reducers

Only primitive reducers mutate durable game state. Initial reducers include:

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

Each reducer:

1. Validates all fallible preconditions before mutation.
2. Applies one named rules operation.
3. Updates authoritative indexes and dirty markers.
4. Emits required immediate events through the coordinator.
5. Appends canonical trace entries.
6. Leaves phase-boundary work to the phase schedule unless the rule specifies an immediate check.

Native card handlers return reducer plans rather than mutating arbitrary components.

## Player-action sequences

Player actions compile into explicit sequence plans with captured inputs and targets. Required sequence builders include:

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

Declared targets are captured when the action is accepted and are not accidentally retargeted by later queries. Later sequence phases separately check whether the required subject remains valid.

Combat has explicit Proposed Attack, Attack, damage, and After Attack events. Redirection and cancellation are event-manager policies, not Observer registration effects.

## Mana, costs, and resources

Mana uses signed internal values with separate counters for:

- Maximum resources
- Used resources
- Temporary resources
- Pending Overload
- Locked Overload
- Resources spent

Displayed values may clamp without changing the underlying rule state. Cost calculation uses the same layered-reduction principles as stats, with a cost-specific layer policy. Negative intermediate values remain available when required by the selected ruleset.

All affordability, target, zone-capacity, and board-position checks occur before spending resources or spawning/moving a game object.

## Deterministic randomness

The simulation owns a versioned pseudo-random generator. Random selection:

1. Builds candidates with a selector.
2. Sorts candidates by complete logical ordering keys.
3. Draws an index from the configured generator.
4. Records the candidate set, generator position, and selection in the trace.

Dependency-default RNG behavior must not silently change simulation results.

## Headless and presentation applications

### Core rules plugin

`HearthstoneRulesPlugin` contains no windowing, rendering, input, audio, or asset-pipeline dependency. It installs:

- Game components and resources
- Card-definition access
- Rules schedules and system sets
- Events and Observers
- Primitive reducers
- Snapshot and trace support

### Headless runner

The headless app uses minimal Bevy facilities and is advanced manually by `Simulation::apply`, a test harness, or an AI controller. It does not sleep to maintain a frame rate. An optional schedule runner may process queued actions continuously for batch simulations.

Input adapters may use channels or async tasks to wait for external agents, but asynchronous waiting is outside the deterministic rules schedule. The world advances only when an action or choice is committed.

### Presentation app

A separate app or plugin adds rendering, UI, animation, and human input. It translates user intent into logical actions and consumes snapshots, traces, or buffered adapter notifications. Presentation timing never delays or reorders authoritative game resolution.

## State cloning and AI workloads

### Clone boundary

MCTS and similar search require an isolated simulation for every speculative branch. The canonical clone boundary includes all dynamic rules state:

- Game-object components and relationships
- Ordered zone indexes
- Phase-coordinator state
- RNG state
- Pending choice state
- Canonical trace position or configured trace policy

Static card definitions, effect programs, and other immutable catalogs are shared rather than copied.

Bevy raw entity IDs differ between worlds. Cloning therefore uses Bevy entity-cloning/remapping facilities or reconstructs a world from a canonical logical-ID snapshot. Components containing local `Entity` references participate in remapping. Clone correctness is defined by snapshot and continuation equivalence, not identical raw entity allocation.

### Parallel playouts

Independent worlds can be distributed across worker threads. Parallelism exists across worlds and in safe intra-world systems, while each world's rule order remains deterministic.

Before adopting copy-on-write storage, selective component cloning, world pools, or snapshot deltas, benchmark representative deep and broad search trees. Correct isolation and reproducibility take precedence over speculative optimization.

### Language bindings

A future PyO3 adapter may expose compact observations, legal actions, snapshots, and batched simulation to Python-based learning systems. The adapter is not part of the core rules crate and must not expose unrestricted world mutation.

## Snapshots, traces, and suspension

### Canonical snapshots

A full snapshot contains:

- Selected ruleset profile
- All durable game objects keyed by logical ID
- Ordered zones
- Player resources and outcome state
- Derived state required for exact continuation
- RNG state
- Serializable phase-coordinator and pending-choice state

A filtered snapshot may hide opponent information for clients. Internal ECS layout is not part of the snapshot contract.

### Trace

The canonical trace records:

- Action acceptance or rejection
- Sequence and phase transitions
- Event creation and dispatch
- Ordered trigger candidates and aborted triggers
- Observer-produced reducer plans
- Modifier collection, layer order, and meaningful recomputation
- Primitive mutations
- Aura and Death Creation steps
- Zone movement
- RNG decisions
- Outcome checks

Buffered Messages may mirror trace records to adapters, but dropping an adapter notification cannot alter gameplay.

### Choices

When a choice is required, the coordinator enters `AwaitingChoice` with its nested dispatch state intact. The pending choice lists legal options by logical ID. Applying a valid choice resumes the same sequence; rejecting an invalid choice leaves the snapshot unchanged.

## Public API direction

Stable-ID actions remain the public contract:

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

The primary API includes:

```text
Simulation::new
Simulation::apply
Simulation::legal_actions
Simulation::snapshot
Simulation::trace
Simulation::pending_choice
Simulation::fork
```

`Simulation::apply` is synchronous from the caller's perspective. It returns after the accepted action reaches `AwaitingAction`, `AwaitingChoice`, `Complete`, or a typed safety/error boundary.

## Proposed module layout

```text
hearthstone_simulator/
├── core/
│   ├── action.rs
│   ├── aura.rs
│   ├── card_definition.rs
│   ├── combat.rs
│   ├── cost.rs
│   ├── death.rs
│   ├── effect.rs
│   ├── enchantment.rs
│   ├── entity.rs
│   ├── event.rs
│   ├── health.rs
│   ├── ids.rs
│   ├── modifier.rs
│   ├── observer.rs
│   ├── phase.rs
│   ├── reducer.rs
│   ├── rng.rs
│   ├── ruleset.rs
│   ├── sequence.rs
│   ├── simulation.rs
│   ├── snapshot.rs
│   ├── trace.rs
│   ├── trigger.rs
│   └── zone.rs
├── app/
│   └── main.rs
├── DESIGN.md
├── IMPLEMENTATION_PROGRESS.md
└── RULEBOOK_CONFORMANCE.md
```

Gazelle determines Bazel package boundaries. The core remains a headless Rust crate. Presentation and language-binding adapters belong in separate targets.

## Migration and implementation milestones

### Milestone 0: Architecture and conformance contract

- Keep the rulebook revision and named ruleset profile pinned.
- Update `RULEBOOK_CONFORMANCE.md` for immediate Observer dispatch and the new phase model.
- Record current frozen-queue/resolution-node behavior that must remain equivalent during migration.
- Define snapshot, trace, and continuation-equivalence fixtures.

### Milestone 1: Flat entity shapes

- Introduce primary concept components with required components.
- Materialize frequently queried keywords as marker components.
- Preserve stable logical IDs and authoritative zone indexes.
- Keep cards as the same entities across ordinary zone movement.

### Milestone 2: Child enchantments and layered recomputation

- Migrate attachments to `ChildOf` enchantment entities.
- Implement explicit stat and cost layer policies.
- Add deterministic application-order keys.
- Implement maximum-Health transition behavior and transformation-in-place tests.

### Milestone 3: Immediate event dispatch

- Define immutable game `EntityEvent` types.
- Add the deterministic event manager and trigger candidate ordering.
- Route trigger effects through targeted Observers and primitive reducers.
- Add explicit command-drain boundaries, re-entrancy guards, and safety budgets.
- Remove authoritative dependence on ECS queue and resolution-frame entities after equivalence tests pass.

### Milestone 4: Phase and death schedule

- Implement the action-driven custom schedule and lifecycle.
- Run aura/stat recalculation before Death Creation where required.
- Implement simultaneous death collection and ordered Death Event dispatch.
- Loop until phase work is quiescent.
- Add named plans for ordinary and forced Death Phases.

### Milestone 5: Complete mechanics

- Complete damage, healing, drawing, fatigue, mana, costs, and Overload.
- Complete transformation, copying, silence, and movement reset policies.
- Complete all player-action sequences and card types.
- Add esoteric rules as named ruleset policies.

### Milestone 6: Headless cloning and search readiness

- Make suspended coordinator state snapshot-safe.
- Validate `Simulation::fork` through continuation equivalence.
- Benchmark full-copy, selective-clone, and world-pool strategies.
- Add parallel-planning stress tests.
- Define a narrow optional Python binding API.

### Milestone 7: Hardening

- Complete normative conformance coverage.
- Add fuzzing and invariant checks around nested Observers and modifier layers.
- Benchmark representative card boards and MCTS workloads.
- Migrate all examples and remove compatibility-only architecture.

## Testing strategy

Every normative rule receives a focused test. Ordering-sensitive tests assert both final state and the canonical trace.

Required suites cover:

- Persistent identity through movement and transformation
- Required-component entity invariants
- Keyword marker insertion and removal
- Enchantment child cleanup and silence policy
- Stat and cost onion ordering
- Maximum-Health increase and decrease behavior
- Aura appearance, removal, and delayed boundary timing
- Deterministic trigger candidate ordering
- Immediate nested Observer resolution
- Observer re-entrancy and safety-budget exhaustion
- Simultaneous event batches
- Aura update before Death Creation
- Delayed, simultaneous, and chained deaths
- Zone-full movement versus generation
- Damage and healing prevention and ordering
- Draw resolution and burning
- Target capture and subject transformation/removal
- Combat redirection and cancellation
- Win, loss, and draw timing
- Same-seed determinism
- Snapshot, fork, and continuation equivalence
- Suspension and resumption at a choice
- Isolation across parallel simulation worlds

Useful invariants include:

- Every game object has exactly one immutable logical ID.
- The logical-ID index agrees with ECS membership.
- Every zoned object occurs exactly once in its authoritative zone index.
- Every attached enchantment has one valid target parent.
- `ComputedStats` equals a fresh deterministic reduction of current inputs.
- No trigger ordering key is incomplete.
- No card mechanic depends on Observer registration or query order.
- A phase cannot advance while its nested reaction chain has pending work.
- No canonical output contains raw Bevy entity IDs.
- Rejected actions and choices leave the canonical snapshot unchanged.
- Forked worlds evolve independently but equivalently under identical inputs.

## Development workflow

After editing Rust source files:

```bash
bazel run //:gazelle
aspect format --scope=all
aspect test //hearthstone_simulator/...
aspect build //...
```

Documentation-only changes still run repository formatting. Focused tests should accompany each migration step, followed by the full repository build before completion.

## Risks and mitigations

### Immediate Observers can hide ordering

Mitigation: Observers execute one candidate selected by the deterministic event manager. Registration order, query order, and raw entity IDs never select the next card trigger.

### Recursive event chains can overflow or loop

Mitigation: track stable event/trigger identities, enforce active-trigger guards and per-action budgets, and use an explicit coordinator stack where command recursion is too deep.

### Aura recomputation can be expensive

Mitigation: begin with correct full or dirty-set recomputation, parallelize independent targets, and introduce dependency indexes only after profiling.

### Marker components can cause archetype churn

Mitigation: use markers for frequently queried binary semantics, group highly volatile data when measurements justify it, and select sparse-set storage only from benchmark evidence.

### Child enchantments can outlive invalid targets

Mitigation: validate parent relationships, use explicit removal policy, and assert attachment invariants after every sequence and clone.

### World cloning is not automatically cheap

Mitigation: share immutable definitions, remap only dynamic entity references, benchmark cloning strategies, and consider pools or snapshot deltas without weakening isolation.

### Presentation code can leak timing into rules

Mitigation: keep the core plugin headless and synchronous. UI and animation consume completed trace data and cannot control rulebook synchronization points.

### The rulebook is observational and mutable

Mitigation: pin revisions, version profiles, classify uncertain and historical behavior, and never silently reinterpret replay inputs.

### Migration can change established behavior

Mitigation: preserve canonical snapshot and trace fixtures, run old/new equivalence tests during each stage, and remove legacy resolution structures only after focused conformance tests pass.

## Architectural decisions

Unless superseded by a documented decision:

1. The initial profile targets advanced rulebook revision 913067 dated 2026-06-26.
2. The simulator core is a headless Bevy plugin; presentation and AI adapters are separate.
3. Every runtime card and attachment is an ECS entity with persistent logical identity.
4. Ordinary zone movement and transformation preserve the target entity's identity.
5. Primary entity shapes use required components; creation APIs perform contextual validation.
6. Frequently queried binary keywords use marker components.
7. Enchantments are child entities, and stats/costs are rebuilt through explicit onion layers.
8. Aura applicability is recalculated at named schedule boundaries, not through permanent hidden mutation.
9. Rulebook reactions use immediate `EntityEvent`/Observer dispatch.
10. A deterministic event manager orders trigger candidates before invoking Observers.
11. Buffered Messages are adapter notifications, not authoritative game events.
12. A custom phase schedule runs nested reactions, aura updates, Death Creation, and follow-up loops in explicit order.
13. Hooks maintain structural invariants but do not implement card mechanics implicitly.
14. Primitive reducers are the only unrestricted path to durable gameplay mutation.
15. Static definitions are shared; dynamic world state is cloneable and isolated for search.
16. Canonical snapshots and traces use logical IDs and define clone equivalence.
17. Gameplay never depends on Bevy query order, Observer registration order, scheduler parallelism, or raw entity IDs.
