use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    Armor, AttachedTo, AttackState, AuraCache, BaseStats, CanonicalTrace, Card, Controller,
    CurrentStats, Damage, DeathEventCache, DeathRecord, DefinitionId, DeterministicRng,
    DisplayName, Effect, EffectContext, EntityKind, EventContext, EventKind, EventOrderKey,
    EventValueOperation, GameEntityId, GameObject, GameOutcome, GameState, Keyword, Keywords,
    NativeEffectId, NestedUnder, PendingDestroy, PlayOrder, Player, PlayerConfig, PlayerId,
    PlayerSelector, QueueKind, QueueState, QueuedEvent, QueuedTrigger, ResolutionCursor,
    ResolutionIdentity, ResolutionKind, ResolutionQueue, ResolveFrame, ResolvePhaseBoundary,
    RngSnapshot, Ruleset, RulesetId, RuntimeTriggers, STARTING_HEALTH, Selector, SimulationStatus,
    TraceEntry, TriggerExecution, ValueExpression, Zone, ZonePosition,
    death::{DefeatedHeroes, PendingDeaths, take_pending_deaths},
    enchantment::{StatModifier, recalculate_stats},
    entity::{
        GameEntityIndex, NextGameEntityId, PlayOrderCounter, allocate_game_id, allocate_play_order,
        game_entity,
    },
    native_effect::{NativeEffectFactory, NativeEffectRegistry},
    queue::{
        QueueMutationError, QueueSelection, abort_selected, add_event_entry, finish_selected,
        freeze_queue, select_next,
    },
    resolver::{
        NextResolutionId, ResolutionError, activate_resolution_child, allocate_resolution_id,
        assert_resolution_invariants, begin_resolution, cleanup_resolution, complete_active,
        configure_resolution, consume_budget, push_resolution, spawn_resolution_child,
    },
    rng::choose_game_entity,
    trigger::{
        TriggerGuards, TriggersSuppressed, begin_trigger_execution, collect_trigger_candidates,
        finish_trigger_execution, reset_trigger_guards,
    },
    zone::{ZoneError, ZoneIndex, assert_zone_invariants, insert_into_zone, move_entity},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GameAction {
    PlayCard {
        player: PlayerId,
        card: GameEntityId,
        target: Option<GameEntityId>,
        board_index: Option<usize>,
        choice: Option<crate::ChoiceId>,
    },
    Attack {
        player: PlayerId,
        attacker: GameEntityId,
        defender: GameEntityId,
    },
    EndTurn {
        player: PlayerId,
    },
    Concede {
        player: PlayerId,
    },
}

impl GameAction {
    fn player(&self) -> PlayerId {
        match self {
            Self::PlayCard { player, .. }
            | Self::Attack { player, .. }
            | Self::EndTurn { player }
            | Self::Concede { player } => *player,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::PlayCard { .. } => "PlayCard",
            Self::Attack { .. } => "Attack",
            Self::EndTurn { .. } => "EndTurn",
            Self::Concede { .. } => "Concede",
        }
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SimulationError {
    #[error("the game is already over")]
    GameOver,
    #[error("the simulation is not awaiting an action")]
    NotAwaitingAction,
    #[error("it is not {0:?}'s turn")]
    NotPlayersTurn(PlayerId),
    #[error("player {0:?} does not exist")]
    PlayerNotFound(PlayerId),
    #[error("game entity {0:?} does not exist")]
    EntityNotFound(GameEntityId),
    #[error("entity {entity:?} is controlled by another player")]
    NotControlled { entity: GameEntityId },
    #[error("entity {entity:?} is not in {expected:?}")]
    WrongZone {
        entity: GameEntityId,
        expected: Zone,
    },
    #[error("entity {0:?} is not a playable card")]
    NotPlayable(GameEntityId),
    #[error("player {player:?} needs {required} mana but only has {available}")]
    NotEnoughMana {
        player: PlayerId,
        required: i32,
        available: i32,
    },
    #[error("player {0:?}'s board is full")]
    BoardFull(PlayerId),
    #[error("attacker {0:?} cannot attack")]
    CannotAttack(GameEntityId),
    #[error("defender {0:?} is not a legal combat target")]
    InvalidDefender(GameEntityId),
    #[error("the simulation did not produce an action result")]
    MissingActionResult,
    #[error("resolution failed: {0}")]
    Resolution(#[from] ResolutionError),
    #[error("zone operation failed: {0}")]
    Zone(#[from] ZoneError),
    #[error("queue operation failed: {0}")]
    Queue(#[from] QueueMutationError),
    #[error("native effect {0:?} is already registered")]
    NativeEffectAlreadyRegistered(NativeEffectId),
    #[error("native effect {0:?} is not registered")]
    NativeEffectNotRegistered(NativeEffectId),
    #[error("native effect {id:?} failed: {reason}")]
    NativeEffectFailed { id: NativeEffectId, reason: String },
    #[error("an event-value modifier requires an active proposed damage or healing event")]
    NoModifiableEventValue,
    #[error("simulation invariant failed: {0}")]
    Invariant(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub entity: GameEntityId,
    pub id: PlayerId,
    pub name: String,
    pub health: i32,
    pub armor: i32,
    pub available_resources: i32,
    pub maximum_resources: i32,
    pub used_resources: i32,
    pub temporary_resources: i32,
    pub pending_overload: i32,
    pub locked_overload: i32,
    pub resources_spent: i32,
    pub fatigue: u32,
    pub hand: Vec<GameEntityId>,
    pub deck: Vec<GameEntityId>,
    pub board: Vec<GameEntityId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameObjectSnapshot {
    pub id: GameEntityId,
    pub definition_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub controller: PlayerId,
    pub zone: Zone,
    pub zone_position: usize,
    pub play_order: u64,
    pub attack: Option<i32>,
    pub maximum_health: Option<i32>,
    pub damage: i32,
    pub exhausted: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSnapshot {
    pub ruleset: RulesetId,
    pub game: GameState,
    pub players: Vec<PlayerSnapshot>,
    pub objects: Vec<GameObjectSnapshot>,
    pub deaths: Vec<DeathRecord>,
    pub rng: RngSnapshot,
}

#[derive(Default, Resource)]
struct PendingActions(VecDeque<GameAction>);

#[derive(Default, Resource)]
struct ActionResults(VecDeque<Result<(), SimulationError>>);

pub struct HearthstoneSimulationPlugin;

impl Plugin for HearthstoneSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
            .init_resource::<Ruleset>()
            .init_resource::<GameEntityIndex>()
            .init_resource::<NextGameEntityId>()
            .init_resource::<PlayOrderCounter>()
            .init_resource::<ZoneIndex>()
            .init_resource::<CanonicalTrace>()
            .init_resource::<DeathEventCache>()
            .init_resource::<PendingDeaths>()
            .init_resource::<DefeatedHeroes>()
            .init_resource::<DeterministicRng>()
            .init_resource::<ResolutionCursor>()
            .init_resource::<NextResolutionId>()
            .init_resource::<TriggerGuards>()
            .init_resource::<NativeEffectRegistry>()
            .init_resource::<PendingActions>()
            .init_resource::<ActionResults>();
        configure_resolution(app);
        app.add_systems(Update, process_next_action);
    }
}

pub struct Simulation {
    app: App,
    initial_players: [PlayerConfig; 2],
    seed: u64,
    action_history: Vec<GameAction>,
    native_effect_factories: BTreeMap<NativeEffectId, NativeEffectFactory>,
}

impl Simulation {
    pub fn new(players: [PlayerConfig; 2]) -> Self {
        Self::with_seed(players, 0)
    }

    pub fn with_seed(players: [PlayerConfig; 2], seed: u64) -> Self {
        let initial_players = players.clone();
        let mut app = App::new();
        app.add_plugins(HearthstoneSimulationPlugin);
        app.world_mut().insert_resource(DeterministicRng::new(seed));
        setup_game(app.world_mut(), players).expect("valid player fixture should initialize");
        app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
        Self {
            app,
            initial_players,
            seed,
            action_history: Vec::new(),
            native_effect_factories: BTreeMap::new(),
        }
    }

    pub fn register_native_effect<M>(
        &mut self,
        id: impl Into<NativeEffectId>,
        handler: impl IntoSystem<In<EffectContext>, Vec<Effect>, M> + Clone + Send + Sync + 'static,
    ) -> Result<(), SimulationError>
    where
        M: 'static,
    {
        let id = id.into();
        let world = self.app.world_mut();
        if world.resource::<NativeEffectRegistry>().0.contains_key(&id) {
            return Err(SimulationError::NativeEffectAlreadyRegistered(id));
        }
        let factory: NativeEffectFactory =
            std::sync::Arc::new(move |world| world.register_system(handler.clone()));
        let system = factory(world);
        world
            .resource_mut::<NativeEffectRegistry>()
            .0
            .insert(id.clone(), system);
        self.native_effect_factories.insert(id, factory);
        Ok(())
    }

    pub fn apply(&mut self, action: GameAction) -> Result<(), SimulationError> {
        self.app
            .world_mut()
            .resource_mut::<PendingActions>()
            .0
            .push_back(action.clone());
        self.app.update();
        let result = self
            .app
            .world_mut()
            .resource_mut::<ActionResults>()
            .0
            .pop_front()
            .ok_or(SimulationError::MissingActionResult)?;
        if result.is_ok() {
            self.action_history.push(action);
        }
        result
    }

    pub fn legal_actions(&mut self) -> Vec<GameAction> {
        let world = self.app.world_mut();
        if world.resource::<GameState>().status != SimulationStatus::AwaitingAction {
            return Vec::new();
        }
        let active = world.resource::<GameState>().active_player;
        let mut actions = vec![GameAction::EndTurn { player: active }];
        let hand = world
            .resource::<ZoneIndex>()
            .entities(active, Zone::Hand)
            .to_vec();
        for card in hand {
            let Some(entity) = game_entity(world, card) else {
                continue;
            };
            let cost = world.get::<CardRuntime>(entity).map_or(0, |card| card.cost);
            if player(world, active)
                .is_some_and(|(_, player, _, _)| player.available_resources() >= cost)
            {
                actions.push(GameAction::PlayCard {
                    player: active,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                });
            }
        }
        actions
    }

    pub fn snapshot(&mut self) -> GameSnapshot {
        snapshot(self.app.world_mut())
    }

    pub fn trace(&self) -> &[TraceEntry] {
        &self.app.world().resource::<CanonicalTrace>().entries
    }

    pub fn fork(&self) -> Result<Self, SimulationError> {
        let mut fork = Self::with_seed(self.initial_players.clone(), self.seed);
        for (id, factory) in &self.native_effect_factories {
            let system = factory(fork.app.world_mut());
            fork.app
                .world_mut()
                .resource_mut::<NativeEffectRegistry>()
                .0
                .insert(id.clone(), system);
            fork.native_effect_factories
                .insert(id.clone(), factory.clone());
        }
        for action in &self.action_history {
            fork.apply(action.clone())?;
        }
        Ok(fork)
    }

    pub fn assert_invariants(&self) -> Result<(), SimulationError> {
        assert_zone_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_resolution_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_game_entity_index(self.app.world()).map_err(SimulationError::Invariant)
    }
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
struct CardRuntime {
    cost: i32,
    program: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DamageRequest {
    source: Option<GameEntityId>,
    target: GameEntityId,
    proposed: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HealingRequest {
    source: Option<GameEntityId>,
    target: GameEntityId,
    proposed: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimultaneousEventOrder {
    OrderOfPlay,
    Given,
}

fn setup_game(world: &mut World, players: [PlayerConfig; 2]) -> Result<(), SimulationError> {
    for (index, config) in players.into_iter().enumerate() {
        let id = PlayerId::ALL[index];
        let starts = id == PlayerId::One;
        spawn_player(world, id, &config.name, starts)?;
        for card in config.deck {
            spawn_card(world, id, card, Zone::Deck)?;
        }
        for card in config.hand {
            spawn_card(world, id, card, Zone::Hand)?;
        }
    }
    Ok(())
}

fn spawn_player(
    world: &mut World,
    player_id: PlayerId,
    name: &str,
    starts: bool,
) -> Result<(), SimulationError> {
    let player_object_id = allocate_game_id(world);
    world.spawn((
        GameObject,
        player_object_id,
        DefinitionId("system:player".to_string()),
        EntityKind::Player,
        Controller(player_id),
        DisplayName(name.to_string()),
        PlayOrder::default(),
        Player {
            id: player_id,
            name: name.to_string(),
            maximum_resources: i32::from(starts),
            used_resources: 0,
            temporary_resources: 0,
            pending_overload: 0,
            locked_overload: 0,
            resources_spent: 0,
            fatigue: 0,
        },
    ));
    insert_into_zone(world, player_object_id, player_id, Zone::SetAside, None)?;

    let hero_id = allocate_game_id(world);
    world.spawn((
        GameObject,
        hero_id,
        DefinitionId("system:hero".to_string()),
        EntityKind::Hero,
        Controller(player_id),
        DisplayName(format!("{name}'s Hero")),
        PlayOrder::default(),
        BaseStats {
            attack: 0,
            health: STARTING_HEALTH,
        },
        CurrentStats {
            attack: 0,
            maximum_health: STARTING_HEALTH,
        },
        Damage::default(),
        Armor::default(),
        AttackState::default(),
        Keywords::default(),
    ));
    insert_into_zone(world, hero_id, player_id, Zone::Play, None)?;
    Ok(())
}

fn spawn_card(
    world: &mut World,
    player_id: PlayerId,
    card: Card,
    zone: Zone,
) -> Result<GameEntityId, SimulationError> {
    let id = allocate_game_id(world);
    let entity = world
        .spawn((
            GameObject,
            id,
            DefinitionId(card.definition_id),
            card.kind,
            Controller(player_id),
            DisplayName(card.name),
            PlayOrder::default(),
            BaseStats {
                attack: card.attack,
                health: card.health,
            },
            CurrentStats {
                attack: card.attack,
                maximum_health: card.health,
            },
            Damage::default(),
            AttackState {
                attacks_this_turn: 0,
                exhausted: true,
            },
            Keywords::default(),
            CardRuntime {
                cost: card.mana_cost,
                program: card.effects,
            },
            RuntimeTriggers(card.triggers),
        ))
        .id();
    if let Err(error) = insert_into_zone(world, id, player_id, zone, None) {
        world.despawn(entity);
        return Err(error.into());
    }
    Ok(id)
}

fn process_next_action(world: &mut World) {
    let Some(action) = world.resource_mut::<PendingActions>().0.pop_front() else {
        return;
    };
    let player = action.player();
    let label = action.label().to_string();
    let result = apply_action(world, action);
    match &result {
        Ok(()) => world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::ActionAccepted {
                player,
                action: label,
            }),
        Err(error) => {
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::ActionRejected {
                    player,
                    reason: error.to_string(),
                })
        }
    }
    world.resource_mut::<ActionResults>().0.push_back(result);
}

fn apply_action(world: &mut World, action: GameAction) -> Result<(), SimulationError> {
    let game = world.resource::<GameState>();
    if game.outcome.is_some() {
        return Err(SimulationError::GameOver);
    }
    if game.status != SimulationStatus::AwaitingAction {
        return Err(SimulationError::NotAwaitingAction);
    }
    validate_turn(world, action.player())?;
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    reset_trigger_guards(world);
    begin_resolution(world, ResolutionKind::Sequence);
    consume_budget(world)?;
    push_resolution(world, ResolutionKind::Phase)?;
    consume_budget(world)?;
    world.run_schedule(ResolveFrame);

    let result = match action {
        GameAction::PlayCard {
            player,
            card,
            target,
            board_index,
            ..
        } => play_card(world, player, card, target, board_index),
        GameAction::Attack {
            player,
            attacker,
            defender,
        } => attack(world, player, attacker, defender),
        GameAction::EndTurn { player } => end_turn(world, player),
        GameAction::Concede { player } => concede(world, player),
    };

    complete_active(world)?;
    resolve_phase_boundaries(world)?;
    check_outcome(world);
    complete_active(world)?;
    cleanup_resolution(world);
    if world.resource::<GameState>().outcome.is_some() {
        world.resource_mut::<GameState>().status = SimulationStatus::Complete;
    } else {
        world.resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
    }
    result?;
    assert_zone_invariants(world).map_err(SimulationError::Invariant)?;
    assert_game_entity_index(world).map_err(SimulationError::Invariant)
}

fn resolve_phase_boundaries(world: &mut World) -> Result<(), SimulationError> {
    loop {
        push_resolution(world, ResolutionKind::PhaseBoundary)?;
        consume_budget(world)?;
        world.run_schedule(ResolvePhaseBoundary);
        complete_active(world)?;

        let deaths = take_pending_deaths(world);
        if deaths.is_empty() {
            return Ok(());
        }
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::DeathPhaseQueued {
                deaths: deaths.iter().map(|record| record.entity).collect(),
            });
        push_resolution(world, ResolutionKind::DeathPhase)?;
        consume_budget(world)?;
        resolve_death_event_batch(world, deaths)?;
        complete_active(world)?;
    }
}

fn resolve_death_event_batch(
    world: &mut World,
    deaths: Vec<DeathRecord>,
) -> Result<(), SimulationError> {
    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;

    // Every Death Event and its trigger queue is created and frozen before the first event starts
    // resolving. Effects introduced by an earlier Deathrattle therefore cannot observe a later
    // death from this simultaneous batch.
    let events = deaths
        .into_iter()
        .map(|record| {
            (
                EventContext {
                    kind: EventKind::Death,
                    source: Some(record.entity),
                    targets: vec![record.entity],
                    controller: record.controller,
                    proposed_value: None,
                    actual_value: None,
                    simultaneous_ordinal: record.simultaneous_ordinal,
                },
                Some(record),
            )
        })
        .collect();
    let (queue, _) = prepare_event_queue(world, events)?;
    resolve_prepared_events(world, queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
    Ok(())
}

fn prepare_event_queue(
    world: &mut World,
    events: Vec<(EventContext, Option<DeathRecord>)>,
) -> Result<(Entity, Vec<Entity>), SimulationError> {
    let parent = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let queue = prepare_collecting_event_queue(world, parent)?;
    activate_resolution_child(world, queue)?;

    let mut event_entities = Vec::with_capacity(events.len());
    for (event, death_record) in events {
        event_entities.push(add_prepared_event(world, queue, event, death_record)?);
    }
    freeze_prepared_event_queue(world, queue)?;
    Ok((queue, event_entities))
}

fn prepare_collecting_event_queue(
    world: &mut World,
    parent: Entity,
) -> Result<Entity, SimulationError> {
    let queue = spawn_resolution_child(world, parent, ResolutionKind::EventQueue);
    consume_budget(world)?;
    world
        .entity_mut(queue)
        .insert((ResolutionQueue(QueueKind::Events), QueueState::Collecting));
    Ok(queue)
}

fn add_prepared_event(
    world: &mut World,
    queue: Entity,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<Entity, SimulationError> {
    let ordinal = event.simultaneous_ordinal;
    let event_entity = prepare_event_child(world, queue, event, death_record)?;
    let event_id = world
        .get::<ResolutionIdentity>(event_entity)
        .expect("prepared event has an identity")
        .id;
    add_event_entry(
        world,
        queue,
        QueuedEvent {
            event: event_id,
            event_entity,
            order: EventOrderKey {
                player_bucket: 0,
                ordinal,
                tie_breaker: 0,
            },
        },
    )
    .expect("new event queue remains collecting while entries are prepared");
    Ok(event_entity)
}

fn freeze_prepared_event_queue(world: &mut World, queue: Entity) -> Result<(), SimulationError> {
    let queue_id = world
        .get::<ResolutionIdentity>(queue)
        .expect("event queue has an identity")
        .id;
    let frozen_ids = freeze_queue(world, queue)?
        .iter()
        .map(|entry| {
            world
                .get::<QueuedEvent>(*entry)
                .expect("event queue entry has a payload")
                .event
        })
        .collect();
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::QueueFrozen {
            queue: queue_id,
            entries: frozen_ids,
        });
    Ok(())
}

fn resolve_prepared_events(world: &mut World, queue: Entity) -> Result<(), SimulationError> {
    loop {
        match select_next(world, queue)? {
            QueueSelection::Complete => return Ok(()),
            QueueSelection::Aborted(_) => {}
            QueueSelection::Selected(entry) => {
                let event_entity = world
                    .get::<QueuedEvent>(entry)
                    .expect("selected event entry has a payload")
                    .event_entity;
                activate_resolution_child(world, event_entity)?;
                resolve_prepared_event(world, event_entity)?;
                complete_active(world)?;
                finish_selected(world, queue, entry)?;
            }
        }
    }
}

fn validate_turn(world: &World, player: PlayerId) -> Result<(), SimulationError> {
    if world.resource::<GameState>().active_player == player {
        Ok(())
    } else {
        Err(SimulationError::NotPlayersTurn(player))
    }
}

fn play_card(
    world: &mut World,
    player_id: PlayerId,
    card_id: GameEntityId,
    declared_target: Option<GameEntityId>,
    board_index: Option<usize>,
) -> Result<(), SimulationError> {
    let card_entity = controlled_entity_in_zone(world, player_id, card_id, Zone::Hand)?;
    let kind = *world
        .get::<EntityKind>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?;
    if !matches!(kind, EntityKind::Minion | EntityKind::Spell) {
        return Err(SimulationError::NotPlayable(card_id));
    }
    if kind == EntityKind::Minion
        && world
            .resource::<ZoneIndex>()
            .entities(player_id, Zone::Play)
            .iter()
            .filter(|id| {
                game_entity(world, **id).and_then(|entity| world.get::<EntityKind>(entity))
                    == Some(&EntityKind::Minion)
            })
            .count()
            >= world.resource::<Ruleset>().board_limit
    {
        return Err(SimulationError::BoardFull(player_id));
    }
    let runtime = world
        .get::<CardRuntime>(card_entity)
        .cloned()
        .ok_or(SimulationError::NotPlayable(card_id))?;
    validate_effect_program(world, &runtime.program, None)?;
    let triggers = world
        .get::<RuntimeTriggers>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?;
    for trigger in &triggers.0 {
        validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
    }
    let cost = runtime.cost;
    spend_resources(world, player_id, cost)?;
    let destination = if kind == EntityKind::Minion {
        Zone::Play
    } else {
        Zone::Graveyard
    };
    let (from, _) = move_entity(world, card_id, destination, board_index)?;
    let order = allocate_play_order(world);
    world.entity_mut(card_entity).insert(order);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: card_id,
            from,
            to: destination,
        });
    resolve_event_if_active(
        world,
        EventContext {
            kind: EventKind::CardPlayed,
            source: Some(card_id),
            targets: declared_target.into_iter().collect(),
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    )?;
    if kind == EntityKind::Minion {
        resolve_event_if_active(
            world,
            EventContext {
                kind: EventKind::Summoned,
                source: Some(card_id),
                targets: vec![card_id],
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            },
        )?;
    }
    execute_effects(
        world,
        &EffectContext {
            source: Some(card_id),
            controller: player_id,
            declared_target,
        },
        &runtime.program,
    )
}

fn attack(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) -> Result<(), SimulationError> {
    let attacker = controlled_entity_in_zone(world, player_id, attacker_id, Zone::Play)?;
    let attack_state = world
        .get::<AttackState>(attacker)
        .copied()
        .ok_or(SimulationError::CannotAttack(attacker_id))?;
    let attack_value = world
        .get::<CurrentStats>(attacker)
        .map_or(0, |stats| stats.attack);
    if attack_state.exhausted || attack_value <= 0 {
        return Err(SimulationError::CannotAttack(attacker_id));
    }
    let defender =
        game_entity(world, defender_id).ok_or(SimulationError::EntityNotFound(defender_id))?;
    if world.get::<Zone>(defender) != Some(&Zone::Play)
        || world
            .get::<Controller>(defender)
            .map(|controller| controller.0)
            != Some(player_id.opponent())
    {
        return Err(SimulationError::InvalidDefender(defender_id));
    }
    let counter_damage = world
        .get::<CurrentStats>(defender)
        .map_or(0, |stats| stats.attack);
    resolve_event_if_active(
        world,
        EventContext {
            kind: EventKind::Attack,
            source: Some(attacker_id),
            targets: vec![defender_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    )?;
    let mut damage = vec![DamageRequest {
        source: Some(attacker_id),
        target: defender_id,
        proposed: attack_value,
    }];
    if counter_damage > 0 {
        damage.push(DamageRequest {
            source: Some(defender_id),
            target: attacker_id,
            proposed: counter_damage,
        });
    }
    apply_damage_batch(world, damage, SimultaneousEventOrder::Given)?;
    let mut state = world
        .get_mut::<AttackState>(attacker)
        .ok_or(SimulationError::CannotAttack(attacker_id))?;
    state.attacks_this_turn += 1;
    state.exhausted = true;
    resolve_event_if_active(
        world,
        EventContext {
            kind: EventKind::AfterAttack,
            source: Some(attacker_id),
            targets: vec![defender_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    )?;
    Ok(())
}

fn end_turn(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
    resolve_event_if_active(
        world,
        EventContext {
            kind: EventKind::TurnEnded,
            source: None,
            targets: Vec::new(),
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    )?;
    let next_player = player_id.opponent();
    let maximum_mana = world.resource::<Ruleset>().maximum_mana;
    {
        let mut game = world.resource_mut::<GameState>();
        game.active_player = next_player;
        game.turn_number += 1;
    }
    let (_, mut player, _, _) = player_mut(world, next_player)?;
    player.maximum_resources = (player.maximum_resources + 1).min(maximum_mana);
    player.used_resources = 0;
    player.temporary_resources = 0;
    player.locked_overload = player.pending_overload;
    player.pending_overload = 0;

    let board = world
        .resource::<ZoneIndex>()
        .entities(next_player, Zone::Play)
        .to_vec();
    for id in board {
        if let Some(entity) = game_entity(world, id)
            && let Some(mut state) = world.get_mut::<AttackState>(entity)
        {
            state.attacks_this_turn = 0;
            state.exhausted = false;
        }
    }
    let turn = world.resource::<GameState>().turn_number;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TurnChanged {
            active_player: next_player,
            turn,
        });
    resolve_event_if_active(
        world,
        EventContext {
            kind: EventKind::TurnStarted,
            source: None,
            targets: Vec::new(),
            controller: next_player,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    )
}

fn concede(world: &mut World, player: PlayerId) -> Result<(), SimulationError> {
    let winner = player.opponent();
    world.resource_mut::<GameState>().outcome = Some(GameOutcome::Winner(winner));
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Outcome {
            winner: Some(winner),
        });
    Ok(())
}

fn spend_resources(
    world: &mut World,
    player_id: PlayerId,
    amount: i32,
) -> Result<(), SimulationError> {
    let amount = amount.max(0);
    let (_, mut player, _, _) = player_mut(world, player_id)?;
    let available = player.available_resources();
    if available < amount {
        return Err(SimulationError::NotEnoughMana {
            player: player_id,
            required: amount,
            available,
        });
    }
    player.used_resources += amount;
    player.resources_spent += amount;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ResourceSpent {
            player: player_id,
            amount,
        });
    Ok(())
}

fn apply_damage(
    world: &mut World,
    source: Option<GameEntityId>,
    target: GameEntityId,
    proposed: i32,
) -> Result<(), SimulationError> {
    apply_damage_batch(
        world,
        vec![DamageRequest {
            source,
            target,
            proposed,
        }],
        SimultaneousEventOrder::Given,
    )
}

fn apply_damage_batch(
    world: &mut World,
    mut requests: Vec<DamageRequest>,
    order: SimultaneousEventOrder,
) -> Result<(), SimulationError> {
    validate_health_change_targets(world, requests.iter().map(|request| request.target))?;
    order_health_change_requests(world, &mut requests, order, |request| request.target);
    if requests.is_empty() {
        return Ok(());
    }
    if world.resource::<ResolutionCursor>().active.is_none() {
        for (ordinal, mut request) in requests.into_iter().enumerate() {
            request.proposed = request.proposed.max(0);
            if damage_passes_protection(world, request) {
                reduce_damage(
                    world,
                    request,
                    u32::try_from(ordinal).expect("damage batch exceeds u32"),
                );
            } else {
                trace_damage(world, request, 0);
            }
        }
        return Ok(());
    }

    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;
    let actual_queue = prepare_collecting_event_queue(world, batch)?;
    for (ordinal, mut request) in requests.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).expect("damage batch exceeds u32");
        request.proposed = request.proposed.max(0);
        if !damage_passes_protection(world, request) {
            trace_damage(world, request, 0);
            continue;
        }

        let proposed_event = EventContext {
            kind: EventKind::ProposedDamage,
            source: request.source,
            targets: vec![request.target],
            controller: event_controller(world, request.source, request.target),
            proposed_value: Some(request.proposed),
            actual_value: None,
            simultaneous_ordinal: ordinal,
        };
        request.proposed = resolve_proposed_health_event(world, batch, proposed_event)?;
        let actual = reduce_damage(world, request, ordinal);
        if actual.actual_value.is_some_and(|value| value > 0) {
            add_prepared_event(world, actual_queue, actual, None)?;
        }
    }

    freeze_prepared_event_queue(world, actual_queue)?;
    activate_resolution_child(world, actual_queue)?;
    resolve_prepared_events(world, actual_queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
    Ok(())
}

fn damage_passes_protection(world: &mut World, request: DamageRequest) -> bool {
    if request.proposed == 0 {
        return false;
    }
    let entity = game_entity(world, request.target)
        .expect("validated damage target remains indexed during damage prevention");
    if world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::Immune))
    {
        return false;
    }
    if world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::DivineShield))
    {
        world
            .get_mut::<Keywords>(entity)
            .expect("keywords were just read")
            .0
            .remove(&Keyword::DivineShield);
        return false;
    }
    true
}

fn resolve_proposed_health_event(
    world: &mut World,
    parent: Entity,
    event: EventContext,
) -> Result<i32, SimulationError> {
    let event_entity = prepare_event_child(world, parent, event, None)?;
    activate_resolution_child(world, event_entity)?;
    resolve_prepared_event(world, event_entity)?;
    let proposed = world
        .get::<EventContext>(event_entity)
        .and_then(|event| event.proposed_value)
        .unwrap_or_default()
        .max(0);
    complete_active(world)?;
    Ok(proposed)
}

fn reduce_damage(
    world: &mut World,
    request: DamageRequest,
    simultaneous_ordinal: u32,
) -> EventContext {
    let entity = game_entity(world, request.target)
        .expect("validated damage target remains indexed during damage mutation");
    let proposed = request.proposed.max(0);
    let armor = world.get::<Armor>(entity).map_or(0, |armor| armor.0);
    let absorbed = armor.min(proposed);
    if absorbed > 0 {
        world
            .get_mut::<Armor>(entity)
            .expect("armor was just read")
            .0 -= absorbed;
    }
    let health_damage = proposed - absorbed;
    if health_damage > 0 {
        world
            .entity_mut(entity)
            .entry::<Damage>()
            .or_default()
            .into_mut()
            .0 += health_damage;
    }
    let actual = absorbed + health_damage;
    trace_damage(world, request, actual);
    EventContext {
        kind: EventKind::Damage,
        source: request.source,
        targets: vec![request.target],
        controller: event_controller(world, request.source, request.target),
        proposed_value: Some(proposed),
        actual_value: Some(actual),
        simultaneous_ordinal,
    }
}

fn trace_damage(world: &mut World, request: DamageRequest, actual: i32) {
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Damage {
            source: request.source,
            target: request.target,
            proposed: request.proposed,
            actual,
        });
}

fn apply_healing_batch(
    world: &mut World,
    mut requests: Vec<HealingRequest>,
    order: SimultaneousEventOrder,
) -> Result<(), SimulationError> {
    validate_health_change_targets(world, requests.iter().map(|request| request.target))?;
    order_health_change_requests(world, &mut requests, order, |request| request.target);
    if requests.is_empty() {
        return Ok(());
    }
    if world.resource::<ResolutionCursor>().active.is_none() {
        for (ordinal, request) in requests.into_iter().enumerate() {
            reduce_healing(
                world,
                request,
                u32::try_from(ordinal).expect("healing batch exceeds u32"),
            );
        }
        return Ok(());
    }

    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;
    let actual_queue = prepare_collecting_event_queue(world, batch)?;
    for (ordinal, mut request) in requests.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).expect("healing batch exceeds u32");
        request.proposed = request.proposed.max(0);
        if request.proposed > 0 {
            let proposed_event = EventContext {
                kind: EventKind::ProposedHealing,
                source: request.source,
                targets: vec![request.target],
                controller: event_controller(world, request.source, request.target),
                proposed_value: Some(request.proposed),
                actual_value: None,
                simultaneous_ordinal: ordinal,
            };
            request.proposed = resolve_proposed_health_event(world, batch, proposed_event)?;
        }
        let actual = reduce_healing(world, request, ordinal);
        if actual.actual_value.is_some_and(|value| value > 0) {
            add_prepared_event(world, actual_queue, actual, None)?;
        }
    }

    freeze_prepared_event_queue(world, actual_queue)?;
    activate_resolution_child(world, actual_queue)?;
    resolve_prepared_events(world, actual_queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
    Ok(())
}

fn reduce_healing(
    world: &mut World,
    request: HealingRequest,
    simultaneous_ordinal: u32,
) -> EventContext {
    let entity = game_entity(world, request.target)
        .expect("validated healing target remains indexed during simultaneous mutation");
    let proposed = request.proposed.max(0);
    let actual = if let Some(mut damage) = world.get_mut::<Damage>(entity) {
        let actual = damage.0.min(proposed);
        damage.0 -= actual;
        actual
    } else {
        0
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Healing {
            source: request.source,
            target: request.target,
            proposed,
            actual,
        });
    EventContext {
        kind: EventKind::Healing,
        source: request.source,
        targets: vec![request.target],
        controller: event_controller(world, request.source, request.target),
        proposed_value: Some(proposed),
        actual_value: Some(actual),
        simultaneous_ordinal,
    }
}

fn validate_health_change_targets(
    world: &World,
    targets: impl IntoIterator<Item = GameEntityId>,
) -> Result<(), SimulationError> {
    for target in targets {
        if game_entity(world, target).is_none() {
            return Err(SimulationError::EntityNotFound(target));
        }
    }
    Ok(())
}

fn order_health_change_requests<T>(
    world: &World,
    requests: &mut [T],
    order: SimultaneousEventOrder,
    target: impl Fn(&T) -> GameEntityId,
) {
    if order == SimultaneousEventOrder::OrderOfPlay {
        requests.sort_by_key(|request| {
            let target = target(request);
            let play_order = game_entity(world, target)
                .and_then(|entity| world.get::<PlayOrder>(entity))
                .map_or(0, |order| order.0);
            (play_order, target)
        });
    }
}

fn event_controller(world: &World, source: Option<GameEntityId>, target: GameEntityId) -> PlayerId {
    source
        .and_then(|source| game_entity(world, source))
        .or_else(|| game_entity(world, target))
        .and_then(|entity| world.get::<Controller>(entity))
        .map_or(PlayerId::One, |controller| controller.0)
}

fn resolve_event_if_active(world: &mut World, event: EventContext) -> Result<(), SimulationError> {
    if world.resource::<ResolutionCursor>().active.is_none() {
        return Ok(());
    }
    resolve_event(world, event)
}

fn resolve_event(world: &mut World, event: EventContext) -> Result<(), SimulationError> {
    resolve_event_with_death_record(world, event, None)
}

fn resolve_event_with_death_record(
    world: &mut World,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<(), SimulationError> {
    let parent = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let event_entity = prepare_event_child(world, parent, event, death_record)?;
    activate_resolution_child(world, event_entity)?;
    resolve_prepared_event(world, event_entity)?;
    complete_active(world)?;
    Ok(())
}

fn prepare_event_child(
    world: &mut World,
    parent: Entity,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<Entity, SimulationError> {
    let event_entity = spawn_resolution_child(world, parent, ResolutionKind::Event);
    consume_budget(world)?;
    let event_identity = *world
        .get::<ResolutionIdentity>(event_entity)
        .expect("new event frame has an identity");
    let trace = TraceEntry::EventCreated {
        id: event_identity.id,
        kind: event.kind,
        source: event.source,
        targets: event.targets.clone(),
        proposed: event.proposed_value,
        actual: event.actual_value,
    };
    world.entity_mut(event_entity).insert(event);
    if let Some(record) = death_record {
        world.entity_mut(event_entity).insert(record);
    }
    world.resource_mut::<CanonicalTrace>().entries.push(trace);

    let queue = spawn_resolution_child(world, event_entity, ResolutionKind::TriggerQueue);
    consume_budget(world)?;
    let queue_identity = *world
        .get::<ResolutionIdentity>(queue)
        .expect("new trigger queue has an identity");
    world
        .entity_mut(queue)
        .insert((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting));
    let entries = collect_trigger_candidates(world, queue, event_entity)?;
    for entry in entries {
        let id = allocate_resolution_id(world);
        world.entity_mut(entry).insert(ResolutionIdentity {
            id,
            kind: ResolutionKind::Trigger,
        });
    }
    let frozen = freeze_queue(world, queue)?;
    let frozen_ids = frozen
        .iter()
        .map(|entry| {
            world
                .get::<ResolutionIdentity>(*entry)
                .expect("collected queue entry has an identity")
                .id
        })
        .collect();
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::QueueFrozen {
            queue: queue_identity.id,
            entries: frozen_ids,
        });
    Ok(event_entity)
}

fn resolve_prepared_event(world: &mut World, event_entity: Entity) -> Result<(), SimulationError> {
    let event = world
        .get::<EventContext>(event_entity)
        .expect("prepared event has event context")
        .clone();
    let queue = world
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<NestedUnder>().map(|parent| parent.0) == Some(event_entity)
                && entity.get::<ResolutionQueue>() == Some(&ResolutionQueue(QueueKind::Triggers)))
            .then_some(entity.id())
        })
        .expect("prepared event has a trigger queue");
    activate_resolution_child(world, queue)?;
    loop {
        match select_next(world, queue)? {
            QueueSelection::Complete => break,
            QueueSelection::Aborted(entry) => trace_trigger_aborted(world, entry),
            QueueSelection::Selected(entry) => {
                resolve_selected_trigger(world, queue, entry, &event)?;
            }
        }
    }
    complete_active(world)?;
    Ok(())
}

fn resolve_selected_trigger(
    world: &mut World,
    queue: Entity,
    entry: Entity,
    event: &EventContext,
) -> Result<(), SimulationError> {
    let queued = *world
        .get::<QueuedTrigger>(entry)
        .expect("selected trigger entry has a payload");
    let entry_id = world
        .get::<ResolutionIdentity>(entry)
        .expect("selected trigger entry has an identity")
        .id;
    let Some(source_entity) = game_entity(world, queued.source) else {
        abort_selected(world, queue, entry)?;
        trace_trigger_aborted(world, entry);
        return Ok(());
    };
    let Some(definition) = world
        .get::<RuntimeTriggers>(source_entity)
        .and_then(|triggers| triggers.0.get(queued.definition_index as usize))
        .cloned()
    else {
        abort_selected(world, queue, entry)?;
        trace_trigger_aborted(world, entry);
        return Ok(());
    };
    if !begin_trigger_execution(world, &queued, &definition) {
        abort_selected(world, queue, entry)?;
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::TriggerAborted {
                id: entry_id,
                source: queued.source,
            });
        return Ok(());
    }

    let controller = world
        .get::<Controller>(source_entity)
        .expect("trigger source has a controller")
        .0;
    let source_kind = *world
        .get::<EntityKind>(source_entity)
        .expect("trigger source has an entity kind");
    let trigger = push_resolution(world, ResolutionKind::Trigger)?;
    consume_budget(world)?;
    world.entity_mut(trigger).insert(TriggerExecution {
        source: queued.source,
        controller,
        source_kind,
    });
    let result = execute_effects(
        world,
        &EffectContext {
            source: Some(queued.source),
            controller,
            declared_target: event.targets.first().copied(),
        },
        &definition.effect_program,
    );
    finish_trigger_execution(world, &queued);
    complete_active(world)?;
    finish_selected(world, queue, entry)?;
    result?;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TriggerResolved {
            id: entry_id,
            source: queued.source,
        });
    Ok(())
}

fn trace_trigger_aborted(world: &mut World, entry: Entity) {
    let (Some(identity), Some(trigger)) = (
        world.get::<ResolutionIdentity>(entry),
        world.get::<QueuedTrigger>(entry),
    ) else {
        return;
    };
    let (id, source) = (identity.id, trigger.source);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TriggerAborted { id, source });
}

fn execute_effects(
    world: &mut World,
    context: &EffectContext,
    effects: &[Effect],
) -> Result<(), SimulationError> {
    for effect in effects {
        push_resolution(world, ResolutionKind::Effect)?;
        consume_budget(world)?;
        let result = execute_effect(world, context, effect);
        complete_active(world)?;
        result?;
    }
    Ok(())
}

fn execute_effect(
    world: &mut World,
    context: &EffectContext,
    effect: &Effect,
) -> Result<(), SimulationError> {
    match effect {
        Effect::DealDamage { targets, amount } => {
            let targets = select_entities(world, context, targets);
            let value = evaluate_value(world, context, *amount, targets.len());
            let requests = targets
                .into_iter()
                .map(|target| DamageRequest {
                    source: context.source,
                    target,
                    proposed: value,
                })
                .collect();
            apply_damage_batch(world, requests, SimultaneousEventOrder::OrderOfPlay)
        }
        Effect::Heal { targets, amount } => {
            let targets = select_entities(world, context, targets);
            let value = evaluate_value(world, context, *amount, targets.len());
            let requests = targets
                .into_iter()
                .map(|target| HealingRequest {
                    source: context.source,
                    target,
                    proposed: value,
                })
                .collect();
            apply_healing_batch(world, requests, SimultaneousEventOrder::OrderOfPlay)
        }
        Effect::ModifyEventValue { operation, value } => {
            modify_active_event_value(world, context, *operation, *value)
        }
        Effect::Destroy { targets } => {
            for target in select_entities(world, context, targets) {
                if let Some(entity) = game_entity(world, target) {
                    world.entity_mut(entity).insert(PendingDestroy);
                }
            }
            Ok(())
        }
        Effect::Draw { player, count } => {
            let player = resolve_player(context.controller, *player);
            for _ in 0..*count {
                draw_card(world, player)?;
            }
            Ok(())
        }
        Effect::GainResource {
            player,
            amount,
            temporary,
        } => {
            let player_id = resolve_player(context.controller, *player);
            let maximum = world.resource::<Ruleset>().maximum_mana;
            let (_, mut player, _, _) = player_mut(world, player_id)?;
            if *temporary {
                player.temporary_resources += *amount;
            } else {
                player.maximum_resources = (player.maximum_resources + *amount).min(maximum);
            }
            Ok(())
        }
        Effect::Summon {
            player,
            card,
            board_index,
        } => {
            let player = resolve_player(context.controller, *player);
            if world
                .resource::<ZoneIndex>()
                .entities(player, Zone::Play)
                .len()
                < world.resource::<Ruleset>().board_limit
            {
                let summoned = spawn_card(world, player, card.clone(), Zone::Play)?;
                if let Some(index) = board_index {
                    move_entity(world, summoned, Zone::Play, Some(*index))?;
                }
                let order = allocate_play_order(world);
                let entity = game_entity(world, summoned).expect("summoned entity was indexed");
                world.entity_mut(entity).insert(order);
                resolve_event_if_active(
                    world,
                    EventContext {
                        kind: EventKind::Summoned,
                        source: context.source,
                        targets: vec![summoned],
                        controller: player,
                        proposed_value: None,
                        actual_value: None,
                        simultaneous_ordinal: 0,
                    },
                )?;
            }
            Ok(())
        }
        Effect::AttachStatModifier { targets, modifier } => {
            for target in select_entities(world, context, targets) {
                attach_stat_modifier(world, context.controller, target, *modifier)?;
            }
            Ok(())
        }
        Effect::Silence { targets } => {
            for target in select_entities(world, context, targets) {
                silence_entity(world, target)?;
            }
            Ok(())
        }
        Effect::Transform { targets, card } => {
            for target in select_entities(world, context, targets) {
                transform_entity(world, target, card.clone())?;
            }
            Ok(())
        }
        Effect::Copy {
            targets,
            player,
            zone,
        } => {
            let controller = resolve_player(context.controller, *player);
            for target in select_entities(world, context, targets) {
                if let Some(card) = copy_card_data(world, target) {
                    let _ = spawn_card(world, controller, card, *zone);
                }
            }
            Ok(())
        }
        Effect::Native(id) => {
            let system = world
                .resource::<NativeEffectRegistry>()
                .0
                .get(id)
                .copied()
                .ok_or_else(|| SimulationError::NativeEffectNotRegistered(id.clone()))?;
            // Bevy flushes Commands queued by a registered system before returning. This is the
            // native-handler mutation boundary documented by the design; durable rules changes
            // should still be returned as an effect plan and resolved below.
            let plan = world
                .run_system_with(system, context.clone())
                .map_err(|error| SimulationError::NativeEffectFailed {
                    id: id.clone(),
                    reason: error.to_string(),
                })?;
            execute_effects(world, context, &plan)
        }
        Effect::Sequence(nested) => execute_effects(world, context, nested),
    }
}

fn validate_effect_program(
    world: &World,
    effects: &[Effect],
    event: Option<EventKind>,
) -> Result<(), SimulationError> {
    for effect in effects {
        match effect {
            Effect::Native(id) if !world.resource::<NativeEffectRegistry>().0.contains_key(id) => {
                return Err(SimulationError::NativeEffectNotRegistered(id.clone()));
            }
            Effect::ModifyEventValue { .. }
                if !matches!(
                    event,
                    Some(EventKind::ProposedDamage | EventKind::ProposedHealing)
                ) =>
            {
                return Err(SimulationError::NoModifiableEventValue);
            }
            Effect::Sequence(nested) => validate_effect_program(world, nested, event)?,
            Effect::Summon { card, .. } | Effect::Transform { card, .. } => {
                validate_effect_program(world, &card.effects, None)?;
                for trigger in &card.triggers {
                    validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn attach_stat_modifier(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    modifier: StatModifier,
) -> Result<(), SimulationError> {
    let target_entity =
        game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let id = allocate_game_id(world);
    let order = allocate_play_order(world);
    world.spawn((
        id,
        DefinitionId("synthetic:stat_modifier".to_string()),
        EntityKind::Enchantment,
        Controller(controller),
        DisplayName("Stat modifier".to_string()),
        order,
        modifier,
        AttachedTo(target_entity),
    ));
    insert_into_zone(world, id, controller, Zone::SetAside, None)?;
    recalculate_stats(world, target);
    Ok(())
}

fn silence_entity(world: &mut World, target: GameEntityId) -> Result<(), SimulationError> {
    let entity = game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    if let Some(mut keywords) = world.get_mut::<Keywords>(entity) {
        keywords.0.clear();
    }
    world.entity_mut(entity).remove::<PendingDestroy>();
    world
        .entity_mut(entity)
        .insert((AuraCache::default(), TriggersSuppressed));
    let enchantments = world
        .iter_entities()
        .filter_map(|candidate| {
            if candidate.get::<AttachedTo>().map(|attached| attached.0) == Some(entity)
                && candidate
                    .get::<StatModifier>()
                    .is_some_and(|modifier| modifier.silence_removable)
            {
                Some((*candidate.get::<GameEntityId>()?, candidate.id()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (id, enchantment) in enchantments {
        world.entity_mut(enchantment).remove::<AttachedTo>();
        let _ = move_entity(world, id, Zone::RemovedFromGame, None);
    }
    recalculate_stats(world, target);
    Ok(())
}

fn transform_entity(
    world: &mut World,
    target: GameEntityId,
    card: Card,
) -> Result<(), SimulationError> {
    let entity = game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    world.entity_mut(entity).insert((
        DefinitionId(card.definition_id),
        DisplayName(card.name),
        card.kind,
        BaseStats {
            attack: card.attack,
            health: card.health,
        },
        CurrentStats {
            attack: card.attack,
            maximum_health: card.health,
        },
        Damage::default(),
        Keywords::default(),
        CardRuntime {
            cost: card.mana_cost,
            program: card.effects,
        },
        RuntimeTriggers(card.triggers),
    ));
    world.entity_mut(entity).remove::<PendingDestroy>();
    world.entity_mut(entity).remove::<TriggersSuppressed>();
    Ok(())
}

fn copy_card_data(world: &World, source: GameEntityId) -> Option<Card> {
    let entity = game_entity(world, source)?;
    let base = world.get::<BaseStats>(entity)?;
    let runtime = world.get::<CardRuntime>(entity)?;
    Some(Card {
        definition_id: world.get::<DefinitionId>(entity)?.0.clone(),
        name: world.get::<DisplayName>(entity)?.0.clone(),
        kind: *world.get::<EntityKind>(entity)?,
        mana_cost: runtime.cost,
        attack: base.attack,
        health: base.health,
        effects: runtime.program.clone(),
        triggers: world
            .get::<RuntimeTriggers>(entity)
            .map_or_else(Vec::new, |triggers| triggers.0.clone()),
    })
}

fn select_entities(
    world: &mut World,
    context: &EffectContext,
    selector: &Selector,
) -> Vec<GameEntityId> {
    let mut selected = match selector {
        Selector::Source => context.source.into_iter().collect(),
        Selector::DeclaredTarget => context.declared_target.into_iter().collect(),
        Selector::Entity(entity) => vec![*entity],
        Selector::InZone { player, zone } => world
            .resource::<ZoneIndex>()
            .entities(resolve_player(context.controller, *player), *zone)
            .to_vec(),
        Selector::FriendlyMinions
        | Selector::EnemyMinions
        | Selector::AllMinions
        | Selector::FriendlyCharacters
        | Selector::EnemyCharacters
        | Selector::AllCharacters => world
            .resource::<ZoneIndex>()
            .0
            .iter()
            .filter(|((player, zone), _)| {
                *zone == Zone::Play
                    && match selector {
                        Selector::FriendlyMinions | Selector::FriendlyCharacters => {
                            *player == context.controller
                        }
                        Selector::EnemyMinions | Selector::EnemyCharacters => {
                            *player == context.controller.opponent()
                        }
                        _ => true,
                    }
            })
            .flat_map(|(_, entities)| entities.iter().copied())
            .filter(|id| {
                let kind =
                    game_entity(world, *id).and_then(|entity| world.get::<EntityKind>(entity));
                match selector {
                    Selector::FriendlyMinions | Selector::EnemyMinions | Selector::AllMinions => {
                        kind == Some(&EntityKind::Minion)
                    }
                    _ => matches!(kind, Some(EntityKind::Hero | EntityKind::Minion)),
                }
            })
            .collect(),
        Selector::Random(inner) => {
            let candidates = select_entities(world, context, inner);
            choose_game_entity(world, candidates).into_iter().collect()
        }
    };
    selected.sort_unstable();
    selected.dedup();
    selected
}

fn evaluate_value(
    world: &World,
    context: &EffectContext,
    expression: ValueExpression,
    target_count: usize,
) -> i32 {
    match expression {
        ValueExpression::Constant(value) => value,
        ValueExpression::SourceAttack => context
            .source
            .and_then(|source| game_entity(world, source))
            .and_then(|entity| world.get::<CurrentStats>(entity))
            .map_or(0, |stats| stats.attack),
        ValueExpression::TargetCount => target_count as i32,
    }
}

fn modify_active_event_value(
    world: &mut World,
    context: &EffectContext,
    operation: EventValueOperation,
    expression: ValueExpression,
) -> Result<(), SimulationError> {
    let mut current = world.resource::<ResolutionCursor>().active;
    let event_entity = loop {
        let Some(entity) = current else {
            return Err(SimulationError::NoModifiableEventValue);
        };
        if world.get::<EventContext>(entity).is_some_and(|event| {
            (event.kind == EventKind::ProposedDamage || event.kind == EventKind::ProposedHealing)
                && event.proposed_value.is_some()
        }) {
            break entity;
        }
        current = world.get::<NestedUnder>(entity).map(|parent| parent.0);
    };
    let event = world
        .get::<EventContext>(event_entity)
        .expect("modifiable event was just found");
    let previous = event
        .proposed_value
        .expect("modifiable event has a proposed value");
    let operand = evaluate_value(world, context, expression, event.targets.len());
    let current = match operation {
        EventValueOperation::Replace => operand,
        EventValueOperation::Add => previous.saturating_add(operand),
        EventValueOperation::Multiply => previous.saturating_mul(operand),
    }
    .max(0);
    world
        .get_mut::<EventContext>(event_entity)
        .expect("modifiable event still exists")
        .proposed_value = Some(current);
    let event = world
        .get::<ResolutionIdentity>(event_entity)
        .expect("event has a resolution identity")
        .id;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::EventValueChanged {
            event,
            operation,
            previous,
            current,
        });
    Ok(())
}

const fn resolve_player(controller: PlayerId, selector: PlayerSelector) -> PlayerId {
    match selector {
        PlayerSelector::Controller => controller,
        PlayerSelector::Opponent => controller.opponent(),
        PlayerSelector::Player(player) => player,
    }
}

fn draw_card(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
    let card = world
        .resource::<ZoneIndex>()
        .entities(player_id, Zone::Deck)
        .first()
        .copied();
    if let Some(card) = card {
        let destination = if world
            .resource::<ZoneIndex>()
            .entities(player_id, Zone::Hand)
            .len()
            >= world.resource::<Ruleset>().hand_limit
        {
            Zone::Graveyard
        } else {
            Zone::Hand
        };
        let (from, _) = move_entity(world, card, destination, None)?;
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::ZoneMoved {
                entity: card,
                from,
                to: destination,
            });
    } else {
        let fatigue = {
            let (_, mut player, _, _) = player_mut(world, player_id)?;
            player.fatigue += 1;
            player.fatigue as i32
        };
        let hero = hero_id(world, player_id).ok_or(SimulationError::PlayerNotFound(player_id))?;
        apply_damage(world, None, hero, fatigue)?;
    }
    Ok(())
}

fn hero_id(world: &World, player: PlayerId) -> Option<GameEntityId> {
    world
        .resource::<ZoneIndex>()
        .entities(player, Zone::Play)
        .iter()
        .copied()
        .find(|id| {
            game_entity(world, *id).and_then(|entity| world.get::<EntityKind>(entity))
                == Some(&EntityKind::Hero)
        })
}

fn check_outcome(world: &mut World) {
    let defeated = world
        .resource::<DefeatedHeroes>()
        .0
        .iter()
        .copied()
        .collect::<Vec<_>>();
    let outcome = match defeated.as_slice() {
        [] => None,
        [player] => Some(GameOutcome::Winner(player.opponent())),
        _ => Some(GameOutcome::Draw),
    };
    if let Some(outcome) = outcome {
        let winner = match outcome {
            GameOutcome::Winner(player) => Some(player),
            GameOutcome::Draw => None,
        };
        world.resource_mut::<GameState>().outcome = Some(outcome);
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::Outcome { winner });
    }
}

fn controlled_entity_in_zone(
    world: &World,
    player: PlayerId,
    id: GameEntityId,
    expected: Zone,
) -> Result<Entity, SimulationError> {
    let entity = game_entity(world, id).ok_or(SimulationError::EntityNotFound(id))?;
    if world
        .get::<Controller>(entity)
        .map(|controller| controller.0)
        != Some(player)
    {
        return Err(SimulationError::NotControlled { entity: id });
    }
    if world.get::<Zone>(entity) != Some(&expected) {
        return Err(SimulationError::WrongZone {
            entity: id,
            expected,
        });
    }
    Ok(entity)
}

fn player(world: &World, id: PlayerId) -> Option<(Entity, &Player, &CurrentStats, &Damage)> {
    let entity = world
        .iter_entities()
        .find(|entity| entity.get::<Player>().is_some_and(|player| player.id == id))?
        .id();
    let player = world.get::<Player>(entity)?;
    let hero = world
        .resource::<ZoneIndex>()
        .entities(id, Zone::Play)
        .iter()
        .find_map(|game_id| {
            let entity = game_entity(world, *game_id)?;
            (world.get::<EntityKind>(entity) == Some(&EntityKind::Hero)).then_some(entity)
        })?;
    Some((
        entity,
        player,
        world.get::<CurrentStats>(hero)?,
        world.get::<Damage>(hero)?,
    ))
}

fn player_mut(
    world: &mut World,
    id: PlayerId,
) -> Result<(Entity, Mut<'_, Player>, CurrentStats, Damage), SimulationError> {
    let (entity, stats, damage) = {
        let (entity, _, stats, damage) =
            player(world, id).ok_or(SimulationError::PlayerNotFound(id))?;
        (entity, *stats, *damage)
    };
    let player = world
        .get_mut::<Player>(entity)
        .ok_or(SimulationError::PlayerNotFound(id))?;
    Ok((entity, player, stats, damage))
}

fn snapshot(world: &mut World) -> GameSnapshot {
    let ruleset = world.resource::<Ruleset>().id;
    let game = world.resource::<GameState>().clone();
    let zones = world.resource::<ZoneIndex>().clone();
    let mut player_query = world.query::<(Entity, &GameEntityId, &Player)>();
    let mut players = player_query
        .iter(world)
        .map(|(_, game_id, player_data)| {
            let (_, _, stats, damage) =
                player(world, player_data.id).expect("every player has a hero");
            let hero = zones
                .entities(player_data.id, Zone::Play)
                .iter()
                .find_map(|id| {
                    let entity = game_entity(world, *id)?;
                    (world.get::<EntityKind>(entity) == Some(&EntityKind::Hero)).then_some(entity)
                })
                .expect("every player has a hero");
            PlayerSnapshot {
                entity: *game_id,
                id: player_data.id,
                name: player_data.name.clone(),
                health: stats.maximum_health - damage.0,
                armor: world.get::<Armor>(hero).map_or(0, |armor| armor.0),
                available_resources: player_data.available_resources(),
                maximum_resources: player_data.maximum_resources,
                used_resources: player_data.used_resources,
                temporary_resources: player_data.temporary_resources,
                pending_overload: player_data.pending_overload,
                locked_overload: player_data.locked_overload,
                resources_spent: player_data.resources_spent,
                fatigue: player_data.fatigue,
                hand: zones.entities(player_data.id, Zone::Hand).to_vec(),
                deck: zones.entities(player_data.id, Zone::Deck).to_vec(),
                board: zones.entities(player_data.id, Zone::Play).to_vec(),
            }
        })
        .collect::<Vec<_>>();
    players.sort_by_key(|player| player.id);

    let mut object_query = world.query::<(
        &GameEntityId,
        &DefinitionId,
        &DisplayName,
        &EntityKind,
        &Controller,
        &Zone,
        &ZonePosition,
        &PlayOrder,
        Option<&CurrentStats>,
        Option<&Damage>,
        Option<&AttackState>,
    )>();
    let mut objects = object_query
        .iter(world)
        .map(
            |(
                id,
                definition,
                name,
                kind,
                controller,
                zone,
                position,
                order,
                stats,
                damage,
                attack,
            )| {
                GameObjectSnapshot {
                    id: *id,
                    definition_id: definition.0.clone(),
                    name: name.0.clone(),
                    kind: *kind,
                    controller: controller.0,
                    zone: *zone,
                    zone_position: position.0,
                    play_order: order.0,
                    attack: stats.map(|stats| stats.attack),
                    maximum_health: stats.map(|stats| stats.maximum_health),
                    damage: damage.map_or(0, |damage| damage.0),
                    exhausted: attack.map(|attack| attack.exhausted),
                }
            },
        )
        .collect::<Vec<_>>();
    objects.sort_by_key(|object| object.id);

    let rng = world.resource::<DeterministicRng>().state();
    GameSnapshot {
        ruleset,
        game,
        players,
        objects,
        deaths: world.resource::<DeathEventCache>().records.clone(),
        rng,
    }
}

fn assert_game_entity_index(world: &World) -> Result<(), String> {
    let index = world.resource::<GameEntityIndex>();
    for (id, entity) in &index.0 {
        if world.get::<GameObject>(*entity).is_none()
            || world.get::<GameEntityId>(*entity) != Some(id)
        {
            return Err(format!("game entity index disagrees for {id:?}"));
        }
    }
    let count = world
        .iter_entities()
        .filter(|entity| entity.contains::<GameObject>())
        .count();
    if count != index.0.len() {
        return Err("not every GameObject is indexed".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn simulation() -> Simulation {
        Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Training Minion", 1, 3, 2)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ])
    }

    fn hand_card(simulation: &mut Simulation, player: PlayerId) -> GameEntityId {
        simulation.snapshot().players[player.bucket() as usize].hand[0]
    }

    fn hero(simulation: &mut Simulation, player: PlayerId) -> GameEntityId {
        let snapshot = simulation.snapshot();
        snapshot.players[player.bucket() as usize]
            .board
            .iter()
            .copied()
            .find(|id| {
                snapshot
                    .objects
                    .iter()
                    .any(|object| object.id == *id && object.kind == EntityKind::Hero)
            })
            .expect("hero should be on the board")
    }

    fn self_event_trigger(
        event: EventKind,
        effect_program: Vec<Effect>,
    ) -> crate::TriggerDefinition {
        crate::TriggerDefinition {
            event,
            eligible_zones: vec![Zone::Play],
            conditions: vec![crate::TimedCondition {
                timing: crate::ConditionTiming::QueueTime,
                condition: crate::TriggerCondition::EventTargetsSelf,
            }],
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::IncludeMortallyWounded,
            effect_program,
        }
    }

    #[googletest::test]
    fn cards_keep_identity_when_played() {
        let mut simulation = simulation();
        let card = hand_card(&mut simulation, PlayerId::One);

        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .expect("card should be playable");
        let snapshot = simulation.snapshot();

        assert_that!(snapshot.players[0].hand.is_empty(), is_true());
        assert_that!(snapshot.players[0].board.contains(&card), is_true());
        assert_that!(
            snapshot
                .objects
                .iter()
                .filter(|object| object.id == card)
                .count(),
            eq(1)
        );
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[googletest::test]
    fn actions_use_stable_entity_targets() {
        let mut simulation = simulation();
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .expect("card should be playable");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .expect("turn should end");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .expect("turn should end");
        let defender = hero(&mut simulation, PlayerId::Two);

        simulation
            .apply(GameAction::Attack {
                player: PlayerId::One,
                attacker: card,
                defender,
            })
            .expect("minion should attack");

        assert_that!(simulation.snapshot().players[1].health, eq(27));
    }

    #[googletest::test]
    fn accepted_actions_are_appended_in_chronological_order() {
        let mut simulation = simulation();
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .unwrap();

        let accepted = simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::ActionAccepted { action, .. } => Some(action.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_that!(accepted, eq(&["PlayCard", "EndTurn"]));
    }

    #[googletest::test]
    fn negative_card_costs_are_floored_at_zero() {
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Discounted", -2, 1, 1)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();

        let player = &simulation.snapshot().players[0];
        assert_that!(player.used_resources, eq(0));
        assert_that!(player.resources_spent, eq(0));
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::ResourceSpent {
                    player: PlayerId::One,
                    amount: 0,
                }
            )),
            is_true()
        );
    }

    #[googletest::test]
    fn rejected_actions_leave_resolution_idle() {
        let mut simulation = simulation();
        let missing = GameEntityId(99_999);

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: missing,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::EntityNotFound(missing)))
        );
        assert_that!(
            simulation.snapshot().game.status,
            eq(SimulationStatus::AwaitingAction)
        );
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[googletest::test]
    fn area_damage_removes_deaths_together_at_the_phase_boundary() {
        let blast = Card::spell("Synthetic Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(2),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![
                    Card::minion("First", 0, 1, 2),
                    Card::minion("Second", 0, 1, 2),
                    blast,
                ],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        for _ in 0..3 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .expect("synthetic card should resolve");
        }

        let snapshot = simulation.snapshot();
        let living_minions = snapshot.players[0]
            .board
            .iter()
            .filter(|id| {
                snapshot
                    .objects
                    .iter()
                    .any(|object| object.id == **id && object.kind == EntityKind::Minion)
            })
            .count();
        let deaths = simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::EntityDied { .. }))
            .count();

        assert_that!(living_minions, eq(0));
        assert_that!(deaths, eq(2));
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[googletest::test]
    fn proposed_damage_modifiers_apply_in_program_order_and_can_prevent_damage() {
        let modifier =
            Card::minion("Ordered Modifier", 0, 1, 12).with_triggers(vec![self_event_trigger(
                EventKind::ProposedDamage,
                vec![
                    Effect::ModifyEventValue {
                        operation: EventValueOperation::Replace,
                        value: ValueExpression::Constant(2),
                    },
                    Effect::ModifyEventValue {
                        operation: EventValueOperation::Add,
                        value: ValueExpression::Constant(1),
                    },
                    Effect::ModifyEventValue {
                        operation: EventValueOperation::Multiply,
                        value: ValueExpression::Constant(3),
                    },
                ],
            )]);
        let preventer =
            Card::minion("Preventer", 0, 1, 12).with_triggers(vec![self_event_trigger(
                EventKind::ProposedDamage,
                vec![Effect::ModifyEventValue {
                    operation: EventValueOperation::Replace,
                    value: ValueExpression::Constant(0),
                }],
            )]);
        let blast = Card::spell("Modifiable Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(5),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![modifier, preventer, blast]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let modifier = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: modifier,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let preventer = hand_card(&mut simulation, PlayerId::One);
        for _ in 0..2 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        let snapshot = simulation.snapshot();
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == modifier)
                .unwrap()
                .damage,
            eq(9)
        );
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == preventer)
                .unwrap()
                .damage,
            eq(0)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter_map(|entry| match entry {
                    TraceEntry::EventValueChanged {
                        operation, current, ..
                    } => Some((*operation, *current)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            eq(&vec![
                (EventValueOperation::Replace, 2),
                (EventValueOperation::Add, 3),
                (EventValueOperation::Multiply, 9),
                (EventValueOperation::Replace, 0),
            ])
        );
    }

    #[googletest::test]
    fn damage_protection_precedes_predamage_triggers_and_zero_damage_is_not_an_event() {
        let protected = Card::minion("Protected Observer", 0, 1, 4).with_triggers(vec![
            self_event_trigger(
                EventKind::ProposedDamage,
                vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            ),
            self_event_trigger(
                EventKind::Damage,
                vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 10,
                    temporary: true,
                }],
            ),
        ]);
        let blast = Card::spell("Protection Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let zero = Card::spell("Zero Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(0),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![protected.clone(), protected.clone(), protected, blast, zero],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let mut minions = Vec::new();
        for _ in 0..3 {
            let card = hand_card(&mut simulation, PlayerId::One);
            minions.push(card);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }
        for (target, keyword) in [
            (minions[0], Keyword::Immune),
            (minions[1], Keyword::DivineShield),
        ] {
            let entity = game_entity(simulation.app.world(), target).unwrap();
            simulation
                .app
                .world_mut()
                .get_mut::<Keywords>(entity)
                .unwrap()
                .0
                .insert(keyword);
        }
        for _ in 0..2 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        let snapshot = simulation.snapshot();
        assert_that!(
            minions
                .iter()
                .map(|id| snapshot
                    .objects
                    .iter()
                    .find(|object| object.id == *id)
                    .unwrap()
                    .damage)
                .collect::<Vec<_>>(),
            eq(&vec![0, 0, 1])
        );
        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(11)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(
                    entry,
                    TraceEntry::EventCreated {
                        kind: EventKind::ProposedDamage,
                        ..
                    }
                ))
                .count(),
            eq(1)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(
                    entry,
                    TraceEntry::EventCreated {
                        kind: EventKind::Damage,
                        ..
                    }
                ))
                .count(),
            eq(1)
        );
    }

    #[googletest::test]
    fn no_op_healing_does_not_create_healing_reactions() {
        let observer =
            Card::minion("Full Health Observer", 0, 1, 4).with_triggers(vec![self_event_trigger(
                EventKind::Healing,
                vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            )]);
        let healing = Card::spell("No-op Healing", 0).with_effects(vec![Effect::Heal {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(2),
        }]);
        let zero = Card::spell("Zero Healing", 0).with_effects(vec![Effect::Heal {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(0),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![observer, healing, zero]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        for _ in 0..3 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(0)
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::Healing,
                    ..
                }
            )),
            is_false()
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(
                    entry,
                    TraceEntry::Healing {
                        proposed: 0 | 2,
                        actual: 0,
                        ..
                    }
                ))
                .count(),
            eq(2)
        );
    }

    #[googletest::test]
    fn each_predamage_queue_resolves_after_prior_damage_is_applied() {
        let first = Card::minion("First Damage Target", 0, 1, 5);
        let healer =
            Card::minion("Predamage Healer", 0, 1, 5).with_triggers(vec![self_event_trigger(
                EventKind::ProposedDamage,
                vec![Effect::Heal {
                    targets: Selector::AllMinions,
                    amount: ValueExpression::Constant(1),
                }],
            )]);
        let blast = Card::spell("Ordered Damage", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(2),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![first, healer, blast]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let mut minions = Vec::new();
        for _ in 0..3 {
            let card = hand_card(&mut simulation, PlayerId::One);
            if minions.len() < 2 {
                minions.push(card);
            }
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        let snapshot = simulation.snapshot();
        assert_that!(
            minions
                .iter()
                .map(|id| snapshot
                    .objects
                    .iter()
                    .find(|object| object.id == *id)
                    .unwrap()
                    .damage)
                .collect::<Vec<_>>(),
            eq(&vec![1, 2])
        );
        let first_damage = simulation
            .trace()
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    TraceEntry::Damage {
                        target,
                        actual: 2,
                        ..
                    } if *target == minions[0]
                )
            })
            .unwrap();
        let intervening_heal = simulation
            .trace()
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    TraceEntry::Healing {
                        target,
                        actual: 1,
                        ..
                    } if *target == minions[0]
                )
            })
            .unwrap();
        let second_damage = simulation
            .trace()
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    TraceEntry::Damage {
                        target,
                        actual: 2,
                        ..
                    } if *target == minions[1]
                )
            })
            .unwrap();
        assert_that!(first_damage < intervening_heal, is_true());
        assert_that!(intervening_heal < second_damage, is_true());
    }

    #[googletest::test]
    fn armor_loss_counts_as_actual_damage_for_traces_and_triggers() {
        let observer = Card::minion("Armor Damage Observer", 0, 1, 4).with_triggers(vec![
            crate::TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: vec![crate::TimedCondition {
                    timing: crate::ConditionTiming::QueueTime,
                    condition: crate::TriggerCondition::EventValueAtLeast(1),
                }],
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            },
        ]);
        let bolt = Card::spell("Armor Bolt", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(2),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![observer, bolt]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        for _ in 0..1 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }
        let target = hero(&mut simulation, PlayerId::Two);
        let target_entity = game_entity(simulation.app.world(), target).unwrap();
        simulation
            .app
            .world_mut()
            .entity_mut(target_entity)
            .insert(Armor(3));
        let bolt = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: bolt,
                target: Some(target),
                board_index: None,
                choice: None,
            })
            .unwrap();

        let target = simulation
            .snapshot()
            .objects
            .into_iter()
            .find(|object| object.id == target)
            .unwrap();
        assert_that!(simulation.snapshot().players[1].armor, eq(1));
        assert_that!(target.damage, eq(0));
        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(1)
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::Damage {
                    target: damaged,
                    proposed: 2,
                    actual: 2,
                    ..
                } if *damaged == target.id
            )),
            is_true()
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::Damage,
                    actual: Some(2),
                    ..
                }
            )),
            is_true()
        );
    }

    #[googletest::test]
    fn simultaneous_damage_is_applied_before_reactions_and_freezes_later_events() {
        let late_observer =
            Card::minion("Late Observer", 0, 1, 2).with_triggers(vec![self_event_trigger(
                EventKind::Damage,
                vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            )]);
        let observer =
            Card::minion("Batch Observer", 0, 1, 3).with_triggers(vec![self_event_trigger(
                EventKind::Damage,
                vec![Effect::Sequence(vec![
                    Effect::Heal {
                        targets: Selector::AllMinions,
                        amount: ValueExpression::Constant(1),
                    },
                    Effect::Summon {
                        player: PlayerSelector::Controller,
                        card: late_observer,
                        board_index: None,
                    },
                ])],
            )]);
        let target = Card::minion("Batch Target", 0, 1, 3);
        let blast = Card::spell("Simultaneous Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![observer, target, blast]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let observer = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: observer,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let target = hand_card(&mut simulation, PlayerId::One);
        for _ in 0..2 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        let snapshot = simulation.snapshot();
        for id in [observer, target] {
            assert_that!(
                snapshot
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .unwrap()
                    .damage,
                eq(0)
            );
        }
        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(0)
        );
        let last_damage = simulation
            .trace()
            .iter()
            .rposition(|entry| matches!(entry, TraceEntry::Damage { .. }))
            .unwrap();
        let first_reaction = simulation
            .trace()
            .iter()
            .position(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
            .unwrap();
        assert_that!(last_damage < first_reaction, is_true());
    }

    #[googletest::test]
    fn simultaneous_healing_is_applied_before_healing_reactions() {
        let observer = Card::minion("Healing Observer", 0, 1, 4).with_triggers(vec![
            self_event_trigger(
                EventKind::ProposedHealing,
                vec![Effect::ModifyEventValue {
                    operation: EventValueOperation::Multiply,
                    value: ValueExpression::Constant(2),
                }],
            ),
            self_event_trigger(EventKind::Healing, Vec::new()),
        ]);
        let target = Card::minion("Healing Target", 0, 1, 4);
        let blast = Card::spell("Setup Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(2),
        }]);
        let healing = Card::spell("Simultaneous Healing", 0).with_effects(vec![Effect::Heal {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![observer, target, blast, healing]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        for _ in 0..4 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        let trace = simulation.trace();
        let healing_entries = trace
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                matches!(entry, TraceEntry::Healing { .. }).then_some(index)
            })
            .collect::<Vec<_>>();
        let healing_reaction = trace
            .iter()
            .rposition(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
            .unwrap();
        assert_that!(healing_entries.len(), eq(2));
        assert_that!(
            healing_entries
                .iter()
                .all(|index| *index < healing_reaction),
            is_true()
        );
        assert_that!(
            trace
                .iter()
                .filter(|entry| matches!(
                    entry,
                    TraceEntry::EventCreated {
                        kind: EventKind::ProposedHealing,
                        ..
                    }
                ))
                .count(),
            eq(2)
        );
        let snapshot = simulation.snapshot();
        let minion_damage = snapshot
            .objects
            .iter()
            .filter(|object| object.kind == EntityKind::Minion)
            .map(|object| object.damage)
            .collect::<Vec<_>>();
        assert_that!(minion_damage, eq(&vec![0, 1]));
    }

    #[googletest::test]
    fn event_value_modifiers_are_rejected_outside_proposed_event_triggers() {
        let invalid =
            Card::spell("Invalid Modifier", 0).with_effects(vec![Effect::ModifyEventValue {
                operation: EventValueOperation::Replace,
                value: ValueExpression::Constant(0),
            }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![invalid]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut simulation, PlayerId::One);
        let before = simulation.snapshot();

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NoModifiableEventValue))
        );
        assert_that!(simulation.snapshot(), eq(&before));
    }

    #[googletest::test]
    fn lethal_hero_state_is_irreversible_before_simultaneous_deathrattle_healing() {
        let healer =
            Card::minion("Last Gasp Healer", 0, 1, 1).with_deathrattle(vec![Effect::Heal {
                targets: Selector::FriendlyCharacters,
                amount: ValueExpression::Constant(30),
            }]);
        let lethal =
            Card::spell("Lethal Friendly Blast", 0).with_effects(vec![Effect::DealDamage {
                targets: Selector::FriendlyCharacters,
                amount: ValueExpression::Constant(30),
            }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![healer, lethal]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let healer = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: healer,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let lethal = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: lethal,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();

        let snapshot = simulation.snapshot();
        assert_that!(snapshot.players[0].health, eq(30));
        assert_that!(
            snapshot.game.outcome,
            eq(Some(GameOutcome::Winner(PlayerId::Two)))
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::HeroDefeated {
                    controller: PlayerId::One,
                    ..
                }
            )),
            is_true()
        );
    }

    #[googletest::test]
    fn simultaneous_deaths_use_global_play_order_and_cache_the_turn() {
        let blast =
            Card::spell("Cross Controller Blast", 0).with_effects(vec![Effect::DealDamage {
                targets: Selector::AllMinions,
                amount: ValueExpression::Constant(1),
            }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Newer One", 0, 1, 1), blast]),
            PlayerConfig::new("Rexxar", vec![Card::minion("Older Two", 0, 1, 1)]),
        ]);
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .unwrap();
        let older = hand_card(&mut simulation, PlayerId::Two);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::Two,
                card: older,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .unwrap();
        let newer = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: newer,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let blast = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: blast,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();

        let deaths = simulation.snapshot().deaths;
        assert_that!(
            deaths
                .iter()
                .map(|record| record.entity)
                .collect::<Vec<_>>(),
            eq(&vec![older, newer])
        );
        assert_that!(
            deaths
                .iter()
                .map(|record| record.turn_of_death)
                .collect::<Vec<_>>(),
            eq(&vec![3, 3])
        );
    }

    #[googletest::test]
    fn death_event_trigger_queues_are_frozen_before_the_batch_resolves() {
        let late_observer =
            Card::minion("Late Observer", 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Death,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
                effect_program: vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            }]);
        let summoner =
            Card::minion("Observer Summoner", 0, 1, 1).with_deathrattle(vec![Effect::Summon {
                player: PlayerSelector::Controller,
                card: late_observer,
                board_index: None,
            }]);
        let blast = Card::spell("Frozen Batch Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![summoner, Card::minion("Second Death", 0, 1, 1), blast],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let summoner = hand_card(&mut simulation, PlayerId::One);
        for _ in 0..3 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(0)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter_map(|entry| match entry {
                    TraceEntry::TriggerResolved { source, .. } => Some(*source),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            eq(&vec![summoner])
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::FrameBegin { kind, .. } if kind == "EventBatch"
            )),
            is_true()
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::FrameBegin { kind, .. } if kind == "EventQueue"
            )),
            is_true()
        );
    }

    #[googletest::test]
    fn aborted_death_event_entries_do_not_block_queue_completion() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Events), QueueState::Collecting))
            .id();
        let entry = add_event_entry(
            &mut world,
            queue,
            QueuedEvent {
                event: crate::ResolutionId(1),
                event_entity: Entity::PLACEHOLDER,
                order: EventOrderKey {
                    player_bucket: 0,
                    ordinal: 0,
                    tie_breaker: 0,
                },
            },
        )
        .expect("collecting queue should accept the event");
        world.entity_mut(entry).insert(QueuedTrigger {
            source: GameEntityId(u64::MAX),
            event: crate::ResolutionId(1),
            event_entity: Entity::PLACEHOLDER,
            definition_index: 0,
            order: crate::TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order: 0,
                source: GameEntityId(u64::MAX),
                tie_breaker: 0,
            },
        });
        freeze_queue(&mut world, queue).expect("event queue should freeze");

        resolve_prepared_events(&mut world, queue)
            .expect("an aborted entry should not fail the Death Event queue");

        assert_that!(
            world.get::<QueueState>(queue),
            eq(Some(&QueueState::Complete))
        );
        assert_that!(
            world.get::<crate::QueueEntryStatus>(entry),
            eq(Some(&crate::QueueEntryStatus::Aborted))
        );
    }

    #[googletest::test]
    fn deathrattles_and_board_observers_mingle_by_play_order() {
        let deathrattle = Card::minion("Older Deathrattle", 0, 1, 1).with_deathrattle(Vec::new());
        let observer =
            Card::minion("Newer Observer", 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Death,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
                effect_program: Vec::new(),
            }]);
        let blast = Card::spell("Mingle Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![deathrattle, observer, blast]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let older = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: older,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let newer = hand_card(&mut simulation, PlayerId::One);
        for _ in 0..2 {
            let card = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card,
                    target: None,
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        assert_that!(
            simulation
                .trace()
                .iter()
                .filter_map(|entry| match entry {
                    TraceEntry::TriggerResolved { source, .. } => Some(*source),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            eq(&vec![older, newer])
        );
    }

    #[googletest::test]
    fn deathrattles_resolve_in_chained_death_phases() {
        let bomber = Card::minion("Deathrattle Bomber", 0, 1, 1).with_deathrattle(vec![
            Effect::DealDamage {
                targets: Selector::AllMinions,
                amount: ValueExpression::Constant(1),
            },
        ]);
        let blast = Card::spell("Chain Starter", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![bomber, Card::minion("Chain Target", 0, 1, 2), blast],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let bomber_id = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: bomber_id,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let target_id = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: target_id,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let blast_id = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: blast_id,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();

        let snapshot = simulation.snapshot();
        assert_that!(
            snapshot
                .deaths
                .iter()
                .map(|record| record.entity)
                .collect::<Vec<_>>(),
            eq(&vec![bomber_id, target_id])
        );
        assert_that!(
            snapshot
                .deaths
                .iter()
                .map(|record| record.remembered_zone_position)
                .collect::<Vec<_>>(),
            eq(&vec![1, 1])
        );
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == bomber_id)
                .unwrap()
                .zone,
            eq(Zone::Graveyard)
        );
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == target_id)
                .unwrap()
                .zone,
            eq(Zone::Graveyard)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(entry, TraceEntry::DeathPhaseQueued { .. }))
                .count(),
            eq(2)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(
                    entry,
                    TraceEntry::EventCreated {
                        kind: EventKind::Death,
                        ..
                    }
                ))
                .count(),
            eq(2)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
                .count(),
            eq(1)
        );
        simulation.assert_invariants().unwrap();
    }

    #[googletest::test]
    fn damage_events_freeze_and_resolve_trigger_effects_depth_first() {
        let reactive =
            Card::minion("Reactive", 0, 1, 4).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: vec![crate::TimedCondition {
                    timing: crate::ConditionTiming::QueueTime,
                    condition: crate::TriggerCondition::EventValueAtLeast(1),
                }],
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: vec![Effect::DealDamage {
                    targets: Selector::EnemyMinions,
                    amount: ValueExpression::Constant(1),
                }],
            }]);
        let blast = Card::spell("Trigger Blast", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![reactive, blast]),
            PlayerConfig::new("Rexxar", vec![Card::minion("Target", 0, 0, 5)]),
        ]);
        let reactive_id = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: reactive_id,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .unwrap();
        let target = hand_card(&mut simulation, PlayerId::Two);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::Two,
                card: target,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .unwrap();
        let blast = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: blast,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();

        let snapshot = simulation.snapshot();
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == reactive_id)
                .unwrap()
                .damage,
            eq(1)
        );
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == target)
                .unwrap()
                .damage,
            eq(3)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
                .count(),
            eq(2)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(entry, TraceEntry::TriggerAborted { .. }))
                .count(),
            eq(2)
        );
        assert_that!(
            simulation.trace().iter().any(|entry| matches!(
                entry,
                TraceEntry::QueueFrozen { entries, .. } if !entries.is_empty()
            )),
            is_true()
        );
        let frozen_ids = simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::QueueFrozen { entries, .. } => Some(entries.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_that!(
            simulation.trace().iter().all(|entry| match entry {
                TraceEntry::TriggerResolved { id, .. } => frozen_ids.contains(id),
                _ => true,
            }),
            is_true()
        );
        simulation.assert_invariants().unwrap();
    }

    #[googletest::test]
    fn resolution_time_conditions_abort_frozen_trigger_entries() {
        let reactive = Card::minion("Resolution-Time Observer", 0, 1, 4).with_triggers(vec![
            crate::TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: vec![crate::TimedCondition {
                    timing: crate::ConditionTiming::ResolutionTime,
                    condition: crate::TriggerCondition::ControllerIs(PlayerId::Two),
                }],
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            },
        ]);
        let bolt = Card::spell("Resolution-Time Bolt", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![reactive, bolt]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let reactive = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: reactive,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let bolt = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: bolt,
                target: Some(reactive),
                board_index: None,
                choice: None,
            })
            .unwrap();

        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(0)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .filter(|entry| matches!(entry, TraceEntry::TriggerAborted { .. }))
                .count(),
            eq(1)
        );
        assert_that!(
            simulation
                .trace()
                .iter()
                .any(|entry| matches!(entry, TraceEntry::TriggerResolved { .. })),
            is_false()
        );
    }

    #[googletest::test]
    fn fork_replays_to_an_equivalent_snapshot_and_trace() {
        let mut simulation = simulation();
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .expect("card should resolve");

        let mut fork = simulation.fork().expect("accepted actions should replay");

        assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
        assert_that!(simulation.trace(), eq(fork.trace()));
    }

    #[googletest::test]
    fn legal_actions_are_deterministic() {
        let mut simulation = simulation();
        let first = simulation.legal_actions();
        let second = simulation.legal_actions();

        assert_that!(first, eq(&second));
        assert_that!(first.len(), eq(2));
    }

    #[googletest::test]
    fn action_validation_reports_each_rejection_and_concede_completes_game() {
        let mut wrong_turn = simulation();
        assert_that!(
            wrong_turn.apply(GameAction::EndTurn {
                player: PlayerId::Two,
            }),
            err(eq(&SimulationError::NotPlayersTurn(PlayerId::Two)))
        );

        let mut game_over = simulation();
        game_over
            .app
            .world_mut()
            .resource_mut::<GameState>()
            .outcome = Some(GameOutcome::Winner(PlayerId::Two));
        assert_that!(
            game_over.apply(GameAction::EndTurn {
                player: PlayerId::One,
            }),
            err(eq(&SimulationError::GameOver))
        );

        let mut busy = simulation();
        busy.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Resolving;
        assert_that!(
            busy.apply(GameAction::EndTurn {
                player: PlayerId::One,
            }),
            err(eq(&SimulationError::NotAwaitingAction))
        );
        assert_that!(busy.legal_actions().is_empty(), is_true());

        let mut invalid = simulation();
        invalid.app.update();
        let card = hand_card(&mut invalid, PlayerId::One);
        let own_hero = hero(&mut invalid, PlayerId::One);
        let opposing_hero = hero(&mut invalid, PlayerId::Two);
        assert_that!(
            invalid.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: opposing_hero,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NotControlled {
                entity: opposing_hero
            }))
        );
        assert_that!(
            invalid.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: own_hero,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::WrongZone {
                entity: own_hero,
                expected: Zone::Hand,
            }))
        );
        let card_entity = game_entity(invalid.app.world(), card).unwrap();
        invalid
            .app
            .world_mut()
            .entity_mut(card_entity)
            .insert(EntityKind::Weapon);
        assert_that!(
            invalid.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NotPlayable(card)))
        );

        let mut board_full = simulation();
        board_full
            .app
            .world_mut()
            .resource_mut::<Ruleset>()
            .board_limit = 0;
        let card = hand_card(&mut board_full, PlayerId::One);
        assert_that!(
            board_full.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::BoardFull(PlayerId::One)))
        );

        let mut expensive = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::spell("Expensive", 2)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut expensive, PlayerId::One);
        assert_that!(
            expensive.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NotEnoughMana {
                player: PlayerId::One,
                required: 2,
                available: 1,
            }))
        );

        let mut concede = simulation();
        concede
            .apply(GameAction::Concede {
                player: PlayerId::One,
            })
            .unwrap();
        assert_that!(
            concede.snapshot().game.outcome,
            eq(Some(GameOutcome::Winner(PlayerId::Two)))
        );
        assert_that!(
            concede.snapshot().game.status,
            eq(SimulationStatus::Complete)
        );
    }

    #[googletest::test]
    fn legal_actions_ignore_stale_ids_and_deck_setup_spawns_cards() {
        let mut simulation = Simulation::new([
            PlayerConfig::with_deck("Jaina", vec![Card::spell("Topdeck", 0)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        simulation
            .app
            .world_mut()
            .resource_mut::<ZoneIndex>()
            .0
            .insert((PlayerId::One, Zone::Hand), vec![GameEntityId(u64::MAX)]);

        assert_that!(
            simulation.legal_actions(),
            eq(&vec![GameAction::EndTurn {
                player: PlayerId::One
            }])
        );
        assert_that!(simulation.snapshot().players[0].deck.len(), eq(1));
    }

    #[googletest::test]
    fn combat_checks_exhaustion_and_defenders_and_applies_counter_damage() {
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Attacker", 0, 2, 3)]),
            PlayerConfig::new("Rexxar", vec![Card::minion("Defender", 0, 1, 2)]),
        ]);
        let attacker = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: attacker,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let enemy_hero = hero(&mut simulation, PlayerId::Two);
        assert_that!(
            simulation.apply(GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender: enemy_hero,
            }),
            err(eq(&SimulationError::CannotAttack(attacker)))
        );
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .unwrap();
        let defender = hand_card(&mut simulation, PlayerId::Two);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::Two,
                card: defender,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .unwrap();
        let own_hero = hero(&mut simulation, PlayerId::One);
        assert_that!(
            simulation.apply(GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender: own_hero,
            }),
            err(eq(&SimulationError::InvalidDefender(own_hero)))
        );
        simulation
            .apply(GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender,
            })
            .unwrap();

        let attacker_state = simulation
            .snapshot()
            .objects
            .into_iter()
            .find(|object| object.id == attacker)
            .unwrap();
        assert_that!(attacker_state.damage, eq(1));
    }

    #[googletest::test]
    fn damage_handles_missing_targets_immunity_shields_armor_and_negative_values() {
        let mut simulation = simulation();
        let target = hero(&mut simulation, PlayerId::Two);
        let entity = game_entity(simulation.app.world(), target).unwrap();
        assert_that!(
            apply_damage(simulation.app.world_mut(), None, GameEntityId(99), 1),
            err(eq(&SimulationError::EntityNotFound(GameEntityId(99))))
        );

        simulation
            .app
            .world_mut()
            .get_mut::<Keywords>(entity)
            .unwrap()
            .0
            .insert(Keyword::Immune);
        apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
        assert_that!(
            simulation.app.world().get::<Damage>(entity),
            eq(Some(&Damage(0)))
        );
        simulation
            .app
            .world_mut()
            .get_mut::<Keywords>(entity)
            .unwrap()
            .0
            .remove(&Keyword::Immune);
        simulation
            .app
            .world_mut()
            .get_mut::<Keywords>(entity)
            .unwrap()
            .0
            .insert(Keyword::DivineShield);
        apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
        assert_that!(
            simulation
                .app
                .world()
                .get::<Keywords>(entity)
                .unwrap()
                .0
                .contains(&Keyword::DivineShield),
            is_false()
        );
        simulation
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(Armor(3));
        apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
        assert_that!(
            simulation.app.world().get::<Armor>(entity),
            eq(Some(&Armor(0)))
        );
        assert_that!(
            simulation.app.world().get::<Damage>(entity),
            eq(Some(&Damage(2)))
        );
        apply_damage(simulation.app.world_mut(), None, target, -5).unwrap();
        assert_that!(
            simulation.app.world().get::<Damage>(entity),
            eq(Some(&Damage(2)))
        );

        apply_damage_batch(
            simulation.app.world_mut(),
            Vec::new(),
            SimultaneousEventOrder::Given,
        )
        .unwrap();
        apply_healing_batch(
            simulation.app.world_mut(),
            Vec::new(),
            SimultaneousEventOrder::Given,
        )
        .unwrap();
        apply_healing_batch(
            simulation.app.world_mut(),
            vec![HealingRequest {
                source: None,
                target,
                proposed: 1,
            }],
            SimultaneousEventOrder::Given,
        )
        .unwrap();
        assert_that!(
            simulation.app.world().get::<Damage>(entity),
            eq(Some(&Damage(1)))
        );
        let context = EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: Some(target),
        };
        assert_that!(
            modify_active_event_value(
                simulation.app.world_mut(),
                &context,
                EventValueOperation::Replace,
                ValueExpression::Constant(0),
            ),
            err(eq(&SimulationError::NoModifiableEventValue))
        );
        begin_resolution(simulation.app.world_mut(), ResolutionKind::Sequence);
        assert_that!(
            modify_active_event_value(
                simulation.app.world_mut(),
                &context,
                EventValueOperation::Replace,
                ValueExpression::Constant(0),
            ),
            err(eq(&SimulationError::NoModifiableEventValue))
        );
        complete_active(simulation.app.world_mut()).unwrap();
        cleanup_resolution(simulation.app.world_mut());
    }

    #[googletest::test]
    fn effect_dispatch_covers_selectors_values_and_stateful_primitives() {
        let mut simulation = Simulation::new([
            PlayerConfig::with_deck("Jaina", vec![Card::spell("Friendly Draw", 0)]),
            PlayerConfig::with_deck("Rexxar", vec![Card::spell("Enemy Draw", 0)]),
        ]);
        let world = simulation.app.world_mut();
        let friendly = spawn_card(
            world,
            PlayerId::One,
            Card::minion("Friendly", 0, 2, 3),
            Zone::Play,
        )
        .unwrap();
        let enemy = spawn_card(
            world,
            PlayerId::Two,
            Card::minion("Enemy", 0, 1, 4),
            Zone::Play,
        )
        .unwrap();
        let context = EffectContext {
            source: Some(friendly),
            controller: PlayerId::One,
            declared_target: Some(enemy),
        };

        assert_that!(
            select_entities(world, &context, &Selector::Source),
            eq(&vec![friendly])
        );
        assert_that!(
            select_entities(world, &context, &Selector::DeclaredTarget),
            eq(&vec![enemy])
        );
        assert_that!(
            select_entities(world, &context, &Selector::Entity(enemy)),
            eq(&vec![enemy])
        );
        assert_that!(
            select_entities(
                world,
                &context,
                &Selector::InZone {
                    player: PlayerSelector::Opponent,
                    zone: Zone::Deck,
                }
            )
            .len(),
            eq(1)
        );
        assert_that!(
            select_entities(world, &context, &Selector::FriendlyMinions),
            eq(&vec![friendly])
        );
        assert_that!(
            select_entities(world, &context, &Selector::EnemyMinions),
            eq(&vec![enemy])
        );
        assert_that!(
            select_entities(world, &context, &Selector::AllMinions),
            eq(&vec![friendly, enemy])
        );
        assert_that!(
            select_entities(world, &context, &Selector::FriendlyCharacters).len(),
            eq(2)
        );
        assert_that!(
            select_entities(world, &context, &Selector::EnemyCharacters).len(),
            eq(2)
        );
        assert_that!(
            select_entities(world, &context, &Selector::AllCharacters).len(),
            eq(4)
        );
        assert_that!(
            select_entities(
                world,
                &context,
                &Selector::Random(Box::new(Selector::Entity(enemy)))
            ),
            eq(&vec![enemy])
        );
        assert_that!(
            evaluate_value(world, &context, ValueExpression::SourceAttack, 9),
            eq(2)
        );
        assert_that!(
            evaluate_value(world, &context, ValueExpression::TargetCount, 9),
            eq(9)
        );
        assert_that!(
            resolve_player(PlayerId::One, PlayerSelector::Controller),
            eq(PlayerId::One)
        );
        assert_that!(
            resolve_player(PlayerId::One, PlayerSelector::Opponent),
            eq(PlayerId::Two)
        );
        assert_that!(
            resolve_player(PlayerId::One, PlayerSelector::Player(PlayerId::Two)),
            eq(PlayerId::Two)
        );

        begin_resolution(world, ResolutionKind::Sequence);
        execute_effects(
            world,
            &context,
            &[Effect::Sequence(vec![
                Effect::DealDamage {
                    targets: Selector::DeclaredTarget,
                    amount: ValueExpression::SourceAttack,
                },
                Effect::Heal {
                    targets: Selector::DeclaredTarget,
                    amount: ValueExpression::TargetCount,
                },
                Effect::Destroy {
                    targets: Selector::DeclaredTarget,
                },
                Effect::Draw {
                    player: PlayerSelector::Opponent,
                    count: 1,
                },
                Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 2,
                    temporary: true,
                },
                Effect::GainResource {
                    player: PlayerSelector::Player(PlayerId::Two),
                    amount: 2,
                    temporary: false,
                },
                Effect::Summon {
                    player: PlayerSelector::Opponent,
                    card: Card::minion("Summoned", 0, 1, 1),
                    board_index: Some(1),
                },
                Effect::AttachStatModifier {
                    targets: Selector::Source,
                    modifier: StatModifier {
                        attack: 3,
                        health: 2,
                        silence_removable: true,
                    },
                },
                Effect::Silence {
                    targets: Selector::Source,
                },
                Effect::Transform {
                    targets: Selector::DeclaredTarget,
                    card: Card::minion("Sheep", 1, 1, 1),
                },
                Effect::Copy {
                    targets: Selector::DeclaredTarget,
                    player: PlayerSelector::Controller,
                    zone: Zone::Hand,
                },
            ])],
        )
        .unwrap();
        complete_active(world).unwrap();
        cleanup_resolution(world);

        assert_that!(
            world.get::<Damage>(game_entity(world, enemy).unwrap()),
            eq(Some(&Damage(0)))
        );
        assert_that!(
            world
                .get::<Keywords>(game_entity(world, friendly).unwrap())
                .unwrap()
                .0
                .contains(&Keyword::Taunt),
            is_false()
        );
        assert_that!(
            world
                .resource::<ZoneIndex>()
                .entities(PlayerId::One, Zone::Hand)
                .len(),
            eq(1)
        );
        assert_that!(
            player(world, PlayerId::One).unwrap().1.temporary_resources,
            eq(2)
        );
        assert_that!(
            player(world, PlayerId::Two).unwrap().1.maximum_resources,
            eq(2)
        );

        execute_effect(
            world,
            &context,
            &Effect::Summon {
                player: PlayerSelector::Opponent,
                card: Card::minion("Appended", 0, 1, 1),
                board_index: None,
            },
        )
        .unwrap();
        assert_that!(
            matches!(
                execute_effect(
                    world,
                    &context,
                    &Effect::Summon {
                        player: PlayerSelector::Opponent,
                        card: Card::minion("Bad Position", 0, 1, 1),
                        board_index: Some(999),
                    },
                ),
                Err(SimulationError::Zone(ZoneError::InvalidPosition { .. }))
            ),
            is_true()
        );
        world.resource_mut::<Ruleset>().board_limit = world
            .resource::<ZoneIndex>()
            .entities(PlayerId::Two, Zone::Play)
            .len();
        execute_effect(
            world,
            &context,
            &Effect::Summon {
                player: PlayerSelector::Opponent,
                card: Card::minion("No Room", 0, 1, 1),
                board_index: None,
            },
        )
        .unwrap();
    }

    #[derive(Resource)]
    struct NativeHandlerObservation(EffectContext);

    fn synthetic_native_handler(
        In(context): In<EffectContext>,
        mut commands: Commands,
    ) -> Vec<Effect> {
        commands.insert_resource(NativeHandlerObservation(context.clone()));
        vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(2),
        }]
    }

    #[googletest::test]
    fn native_handlers_flush_commands_and_return_nested_effect_plans() {
        let native_id = NativeEffectId::new("synthetic:native_damage");
        let spell =
            Card::spell("Native Bolt", 0).with_effects(vec![Effect::Native(native_id.clone())]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![spell]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        simulation
            .register_native_effect(native_id.clone(), synthetic_native_handler)
            .unwrap();
        assert_that!(
            simulation.register_native_effect(native_id.clone(), synthetic_native_handler),
            err(eq(&SimulationError::NativeEffectAlreadyRegistered(
                native_id.clone()
            )))
        );
        let card = hand_card(&mut simulation, PlayerId::One);
        let target = hero(&mut simulation, PlayerId::Two);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: Some(target),
                board_index: None,
                choice: None,
            })
            .unwrap();

        assert_that!(simulation.snapshot().players[1].health, eq(28));
        let mut fork = simulation.fork().unwrap();
        assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
        assert_that!(simulation.trace(), eq(fork.trace()));
        assert_that!(
            simulation
                .app
                .world()
                .resource::<NativeHandlerObservation>()
                .0
                .declared_target,
            eq(Some(target))
        );

        let missing = NativeEffectId::new("synthetic:missing");
        let world = simulation.app.world_mut();
        begin_resolution(world, ResolutionKind::Sequence);
        let context = EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
        };
        assert_that!(
            execute_effect(world, &context, &Effect::Native(missing.clone())),
            err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
        );
        complete_active(world).unwrap();
        cleanup_resolution(world);
    }

    #[googletest::test]
    fn missing_native_effects_are_rejected_before_card_play_mutates_state() {
        let missing = NativeEffectId::new("synthetic:missing");
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![
                    Card::spell("Missing Native", 1)
                        .with_effects(vec![Effect::Native(missing.clone())]),
                ],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut simulation, PlayerId::One);
        let before = simulation.snapshot();

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
        );

        assert_that!(simulation.snapshot(), eq(&before));
        let mut fork = simulation.fork().unwrap();
        assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    }

    #[googletest::test]
    fn missing_native_deathrattles_are_rejected_before_card_play_mutates_state() {
        let missing = NativeEffectId::new("synthetic:missing_deathrattle");
        let mut simulation = Simulation::new([
            PlayerConfig::new(
                "Jaina",
                vec![
                    Card::minion("Missing Native Deathrattle", 1, 1, 1)
                        .with_deathrattle(vec![Effect::Native(missing.clone())]),
                ],
            ),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut simulation, PlayerId::One);
        let before = simulation.snapshot();

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
        );

        assert_that!(simulation.snapshot(), eq(&before));
        assert_that!(
            simulation.snapshot().game.status,
            eq(SimulationStatus::AwaitingAction)
        );
        simulation
            .assert_invariants()
            .expect("rejected Deathrattle should preserve invariants");
    }

    #[googletest::test]
    fn silence_suppresses_future_triggers_but_preserves_frozen_entries() {
        let suppressor =
            Card::minion("Suppressor", 0, 1, 3).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: vec![Effect::Silence {
                    targets: Selector::DeclaredTarget,
                }],
            }]);
        let reactive =
            Card::minion("Reactive", 0, 1, 4).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: vec![Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                }],
            }]);
        let bolt = || {
            Card::spell("Bolt", 0).with_effects(vec![Effect::DealDamage {
                targets: Selector::DeclaredTarget,
                amount: ValueExpression::Constant(1),
            }])
        };
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![suppressor, reactive, bolt(), bolt()]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let suppressor = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: suppressor,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let reactive = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: reactive,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        for _ in 0..2 {
            let bolt = hand_card(&mut simulation, PlayerId::One);
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    card: bolt,
                    target: Some(reactive),
                    board_index: None,
                    choice: None,
                })
                .unwrap();
        }

        assert_that!(
            player(simulation.app.world(), PlayerId::One)
                .unwrap()
                .1
                .temporary_resources,
            eq(1)
        );
        let reactive_entity = game_entity(simulation.app.world(), reactive).unwrap();
        assert_that!(
            simulation
                .app
                .world()
                .get::<RuntimeTriggers>(reactive_entity)
                .is_some(),
            is_true()
        );
        assert_that!(
            simulation
                .app
                .world()
                .get::<TriggersSuppressed>(reactive_entity)
                .is_some(),
            is_true()
        );

        transform_entity(
            simulation.app.world_mut(),
            reactive,
            Card::minion("Transformed", 0, 2, 2).with_triggers(vec![crate::TriggerDefinition {
                event: EventKind::Healing,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: crate::SourceEligibilityPolicy::MustExist,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: Vec::new(),
            }]),
        )
        .unwrap();
        let transformed = game_entity(simulation.app.world(), reactive).unwrap();
        assert_that!(
            simulation
                .app
                .world()
                .get::<TriggersSuppressed>(transformed)
                .is_none(),
            is_true()
        );
        assert_that!(
            simulation
                .app
                .world()
                .get::<RuntimeTriggers>(transformed)
                .unwrap()
                .0[0]
                .event,
            eq(EventKind::Healing)
        );
    }

    #[googletest::test]
    fn draw_burn_fatigue_outcomes_and_private_helper_errors_are_testable() {
        let mut simulation = Simulation::new([
            PlayerConfig::with_deck("Jaina", vec![Card::spell("Burn Me", 0)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let world = simulation.app.world_mut();
        world.resource_mut::<Ruleset>().hand_limit = 0;
        draw_card(world, PlayerId::One).unwrap();
        assert_that!(
            world
                .resource::<ZoneIndex>()
                .entities(PlayerId::One, Zone::Graveyard)
                .len(),
            eq(1)
        );
        draw_card(world, PlayerId::One).unwrap();
        assert_that!(player(world, PlayerId::One).unwrap().1.fatigue, eq(1));

        let first_hero = hero_id(world, PlayerId::One).unwrap();
        let second_hero = hero_id(world, PlayerId::Two).unwrap();
        let first_entity = game_entity(world, first_hero).unwrap();
        let second_entity = game_entity(world, second_hero).unwrap();
        world.get_mut::<Damage>(first_entity).unwrap().0 = STARTING_HEALTH;
        crate::death::create_deaths(world);
        check_outcome(world);
        assert_that!(
            world.resource::<GameState>().outcome,
            eq(Some(GameOutcome::Winner(PlayerId::Two)))
        );
        world.resource_mut::<GameState>().outcome = None;
        world.get_mut::<Damage>(second_entity).unwrap().0 = STARTING_HEALTH;
        crate::death::create_deaths(world);
        check_outcome(world);
        assert_that!(
            world.resource::<GameState>().outcome,
            eq(Some(GameOutcome::Draw))
        );

        assert_that!(
            attach_stat_modifier(
                world,
                PlayerId::One,
                GameEntityId(999),
                StatModifier {
                    attack: 1,
                    health: 1,
                    silence_removable: true,
                }
            ),
            err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
        );
        assert_that!(
            silence_entity(world, GameEntityId(999)),
            err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
        );
        assert_that!(
            transform_entity(world, GameEntityId(999), Card::minion("Missing", 0, 1, 1)),
            err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
        );
        assert_that!(copy_card_data(world, GameEntityId(999)), none());
        assert_that!(hero_id(world, PlayerId::One), eq(Some(first_hero)));
    }

    #[googletest::test]
    fn spawn_and_index_helpers_report_cleanup_and_drift() {
        let mut simulation = simulation();
        let world = simulation.app.world_mut();
        world.resource_mut::<Ruleset>().hand_limit = 0;
        assert_that!(
            matches!(
                spawn_card(world, PlayerId::One, Card::spell("No Space", 0), Zone::Hand),
                Err(SimulationError::Zone(ZoneError::Full { .. }))
            ),
            is_true()
        );

        let indexed = *world.resource::<GameEntityIndex>().0.keys().next().unwrap();
        let original = world.resource::<GameEntityIndex>().0[&indexed];
        let replacement = world.spawn_empty().id();
        world
            .resource_mut::<GameEntityIndex>()
            .0
            .insert(indexed, replacement);
        assert_that!(
            assert_game_entity_index(world),
            err(eq(&format!("game entity index disagrees for {indexed:?}")))
        );
        world
            .resource_mut::<GameEntityIndex>()
            .0
            .insert(indexed, original);
        world.spawn(GameObject);
        assert_that!(
            assert_game_entity_index(world),
            err(eq(&"not every GameObject is indexed".to_string()))
        );
    }
}
