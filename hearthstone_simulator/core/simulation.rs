use std::collections::VecDeque;

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    Armor, AttachedTo, AttackState, AuraCache, BaseStats, CanonicalTrace, Card, Controller,
    CurrentStats, Damage, DefinitionId, DeterministicRng, DisplayName, Effect, EffectContext,
    EntityKind, GameEntityId, GameObject, GameOutcome, GameState, Keyword, Keywords,
    PendingDestroy, PlayOrder, Player, PlayerConfig, PlayerId, PlayerSelector, ResolutionCursor,
    ResolutionKind, ResolveFrame, ResolvePhaseBoundary, RngSnapshot, Ruleset, RulesetId,
    STARTING_HEALTH, Selector, SimulationStatus, TraceEntry, ValueExpression, Zone, ZonePosition,
    enchantment::{StatModifier, recalculate_stats},
    entity::{
        GameEntityIndex, NextGameEntityId, PlayOrderCounter, allocate_game_id, allocate_play_order,
        game_entity,
    },
    resolver::{
        NextResolutionId, ResolutionError, assert_resolution_invariants, begin_resolution,
        cleanup_resolution, complete_active, configure_resolution, consume_budget, push_resolution,
    },
    rng::choose_game_entity,
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
            .init_resource::<DeterministicRng>()
            .init_resource::<ResolutionCursor>()
            .init_resource::<NextResolutionId>()
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
        }
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
        Ok(()) => world.resource_mut::<CanonicalTrace>().entries.insert(
            0,
            TraceEntry::ActionAccepted {
                player,
                action: label,
            },
        ),
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
    push_resolution(world, ResolutionKind::PhaseBoundary)?;
    consume_budget(world)?;
    world.run_schedule(ResolvePhaseBoundary);
    check_outcome(world);
    complete_active(world)?;
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
    apply_damage(world, Some(attacker_id), defender_id, attack_value)?;
    if counter_damage > 0 {
        apply_damage(world, Some(defender_id), attacker_id, counter_damage)?;
    }
    let mut state = world
        .get_mut::<AttackState>(attacker)
        .ok_or(SimulationError::CannotAttack(attacker_id))?;
    state.attacks_this_turn += 1;
    state.exhausted = true;
    check_outcome(world);
    Ok(())
}

fn end_turn(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
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
    Ok(())
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
    let entity = game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let immune = world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::Immune));
    let shielded = world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::DivineShield));
    let proposed = proposed.max(0);
    let actual = if immune {
        0
    } else if shielded && proposed > 0 {
        world
            .get_mut::<Keywords>(entity)
            .expect("keywords were just read")
            .0
            .remove(&Keyword::DivineShield);
        0
    } else {
        let armor = world.get::<Armor>(entity).map_or(0, |armor| armor.0);
        let absorbed = armor.min(proposed);
        if absorbed > 0 {
            world
                .get_mut::<Armor>(entity)
                .expect("armor was just read")
                .0 -= absorbed;
        }
        proposed - absorbed
    };
    if actual > 0 {
        world
            .entity_mut(entity)
            .entry::<Damage>()
            .or_default()
            .into_mut()
            .0 += actual;
    }
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Damage {
            source,
            target,
            proposed,
            actual,
        });
    Ok(())
}

fn execute_effects(
    world: &mut World,
    context: &EffectContext,
    effects: &[Effect],
) -> Result<(), SimulationError> {
    for effect in effects {
        push_resolution(world, ResolutionKind::Effect)?;
        consume_budget(world)?;
        let result = match effect {
            Effect::DealDamage { targets, amount } => {
                let targets = select_entities(world, context, targets);
                let value = evaluate_value(world, context, *amount, targets.len());
                for target in targets {
                    apply_damage(world, context.source, target, value)?;
                }
                Ok(())
            }
            Effect::Heal { targets, amount } => {
                let targets = select_entities(world, context, targets);
                let value = evaluate_value(world, context, *amount, targets.len()).max(0);
                for target in targets {
                    if let Some(entity) = game_entity(world, target)
                        && let Some(mut damage) = world.get_mut::<Damage>(entity)
                    {
                        damage.0 = (damage.0 - value).max(0);
                    }
                }
                Ok(())
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
            Effect::Sequence(nested) => execute_effects(world, context, nested),
        };
        complete_active(world)?;
        result?;
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
    world.entity_mut(entity).insert(AuraCache::default());
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
    ));
    world.entity_mut(entity).remove::<PendingDestroy>();
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
    let mut defeated = Vec::new();
    for player_id in PlayerId::ALL {
        if let Some((_, _, stats, damage)) = player(world, player_id)
            && damage.0 >= stats.maximum_health
        {
            defeated.push(player_id);
        }
    }
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

    #[test]
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

        assert!(snapshot.players[0].hand.is_empty());
        assert!(snapshot.players[0].board.contains(&card));
        assert_eq!(
            snapshot
                .objects
                .iter()
                .filter(|object| object.id == card)
                .count(),
            1
        );
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[test]
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

        assert_eq!(simulation.snapshot().players[1].health, 27);
    }

    #[test]
    fn rejected_actions_leave_resolution_idle() {
        let mut simulation = simulation();
        let missing = GameEntityId(99_999);

        assert_eq!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: missing,
                target: None,
                board_index: None,
                choice: None,
            }),
            Err(SimulationError::EntityNotFound(missing))
        );
        assert_eq!(
            simulation.snapshot().game.status,
            SimulationStatus::AwaitingAction
        );
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[test]
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

        assert_eq!(living_minions, 0);
        assert_eq!(deaths, 2);
        simulation
            .assert_invariants()
            .expect("invariants should hold");
    }

    #[test]
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

        assert_eq!(simulation.snapshot(), fork.snapshot());
        assert_eq!(simulation.trace(), fork.trace());
    }

    #[test]
    fn legal_actions_are_deterministic() {
        let mut simulation = simulation();
        let first = simulation.legal_actions();
        let second = simulation.legal_actions();

        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }
}
