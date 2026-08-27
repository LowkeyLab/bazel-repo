use std::collections::VecDeque;

use bevy::prelude::*;

use crate::{
    AttackState, CanonicalTrace, Controller, CurrentStats, EffectContext, EntityKind, EventContext,
    EventKind, GameEntityId, GameOutcome, GameState, PlayerId, ResolutionKind, ResolveFrame,
    Ruleset, RuntimeTriggers, SimulationStatus, TraceEntry, Zone,
    entity::{allocate_play_order, game_entity},
    resolver::{
        begin_resolution, cleanup_resolution, complete_active, consume_budget, push_resolution,
    },
    trigger::reset_trigger_guards,
    zone::{ZoneIndex, assert_zone_invariants, move_entity},
};

use super::{
    card_runtime::CardRuntime,
    effect_executor::{execute_effects, validate_effect_program},
    error::SimulationError,
    event_resolver::{resolve_event_if_active, resolve_phase_boundaries},
    health::{DamageRequest, SimultaneousEventOrder, apply_damage_batch},
    player::{check_outcome, controlled_entity_in_zone, player, player_mut},
    snapshot::assert_game_entity_index,
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

#[derive(Default, Resource)]
struct PendingActions(VecDeque<GameAction>);

#[derive(Default, Resource)]
struct ActionResults(VecDeque<Result<(), SimulationError>>);

pub(super) fn configure_actions(app: &mut App) {
    app.init_resource::<PendingActions>()
        .init_resource::<ActionResults>()
        .add_systems(Update, process_next_action);
}

pub(super) fn submit_action(app: &mut App, action: GameAction) -> Result<(), SimulationError> {
    app.world_mut()
        .resource_mut::<PendingActions>()
        .0
        .push_back(action);
    app.update();
    app.world_mut()
        .resource_mut::<ActionResults>()
        .0
        .pop_front()
        .ok_or(SimulationError::MissingActionResult)?
}

pub(super) fn legal_actions(world: &mut World) -> Vec<GameAction> {
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
