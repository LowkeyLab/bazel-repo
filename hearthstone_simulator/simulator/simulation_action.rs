use std::collections::VecDeque;

use bevy::prelude::*;

use crate::{
    AttachedTo, AttackState, CanonicalTrace, ChoiceId, Controller, CurrentResolutionOp,
    CurrentStats, DamageRequest, EffectContext, EntityKind, EventContext, EventKind, GameAction,
    GameEntityId, GameOutcome, GameState, HeroPowerState, PhaseBoundaryPlan, PlayerId,
    ResolutionOp, ResolutionWork, ResolveFrame, Ruleset, RuntimeTriggers, ScheduledTurnKind,
    SequenceStep, SimulationStatus, TemporaryDuration, TraceEntry, TurnSchedule, Zone,
    ZoneMoveRequest, ZoneMovementKind,
    enchantment::{recalculate_cost, recalculate_keywords, recalculate_stats},
    entity::game_entity,
    resolver::{
        abandon_sequence, begin_sequence, consume_budget, finish_sequence, pop_resolution_op,
        push_resolution_ops,
    },
    zone::{
        ZoneIndex, ZoneMoveOutcome, assert_zone_invariants, board_is_full, move_entity,
        move_entity_with_request, validate_board_position, validate_zone_position,
    },
};

use super::{
    card_runtime::CardRuntime,
    effect_executor::validate_effect_program,
    error::SimulationError,
    event_resolver::OperationFailure,
    player::{assert_player_role_invariants, controlled_entity_in_zone, player, player_mut},
    snapshot::assert_game_entity_index,
};

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

pub(super) fn submit_choice(app: &mut App, option: ChoiceId) -> Result<(), SimulationError> {
    answer_choice(app.world_mut(), option)?;
    if let Err(error) = drive_resolution(app.world_mut()) {
        abandon_sequence(app.world_mut());
        let status = if app.world().resource::<GameState>().outcome.is_some() {
            SimulationStatus::Complete
        } else {
            SimulationStatus::AwaitingAction
        };
        app.world_mut().resource_mut::<GameState>().status = status;
        return Err(error);
    }
    finish_resolution_if_idle(app.world_mut())
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
        let cost = world
            .get::<CardRuntime>(entity)
            .map_or(0, |card| card.cost.max(0));
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
    let result = apply_action(world, &action);
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
                });
        }
    }
    world.resource_mut::<ActionResults>().0.push_back(result);
}

fn apply_action(world: &mut World, action: &GameAction) -> Result<(), SimulationError> {
    validate_action(world, action)?;
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    begin_sequence(world)?;
    let step = match action {
        GameAction::PlayCard {
            player,
            card,
            target,
            board_index,
            ..
        } => SequenceStep::PlayCard {
            player: *player,
            card: *card,
            target: *target,
            board_index: *board_index,
        },
        GameAction::Attack {
            player,
            attacker,
            defender,
        } => SequenceStep::Attack {
            player: *player,
            attacker: *attacker,
            defender: *defender,
        },
        GameAction::EndTurn { player } => SequenceStep::EndTurn { player: *player },
        GameAction::Concede { player } => SequenceStep::Concede { player: *player },
    };
    push_resolution_ops(
        world,
        [
            ResolutionOp::RunSequenceStep(step),
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::CheckOutcome,
        ],
    );
    if let Err(error) = drive_resolution(world) {
        abandon_sequence(world);
        world.resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
        return Err(error);
    }
    finish_resolution_if_idle(world)
}

fn validate_action(world: &World, action: &GameAction) -> Result<(), SimulationError> {
    let game = world.resource::<GameState>();
    if game.outcome.is_some() {
        return Err(SimulationError::GameOver);
    }
    if game.status != SimulationStatus::AwaitingAction {
        return Err(SimulationError::NotAwaitingAction);
    }
    if game.active_player != action.player() {
        return Err(SimulationError::NotPlayersTurn(action.player()));
    }
    match action {
        GameAction::PlayCard {
            player,
            card,
            target: _,
            board_index,
            ..
        } => validate_play_card(world, *player, *card, *board_index),
        GameAction::Attack {
            player,
            attacker,
            defender,
        } => validate_attack(world, *player, *attacker, *defender),
        GameAction::EndTurn { .. } | GameAction::Concede { .. } => Ok(()),
    }
}

fn validate_play_card(
    world: &World,
    player_id: PlayerId,
    card_id: GameEntityId,
    board_index: Option<usize>,
) -> Result<(), SimulationError> {
    let card_entity = controlled_entity_in_zone(world, player_id, card_id, Zone::Hand)?;
    let kind = *world
        .get::<EntityKind>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?;
    if !matches!(kind, EntityKind::Minion | EntityKind::Spell) {
        return Err(SimulationError::NotPlayable(card_id));
    }
    if kind == EntityKind::Minion && board_is_full(world, player_id) {
        return Err(SimulationError::BoardFull(player_id));
    }
    let runtime = world
        .get::<CardRuntime>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?;
    validate_effect_program(world, &runtime.program, None)?;
    for trigger in &world
        .get::<RuntimeTriggers>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?
        .0
    {
        validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
    }
    let available = player(world, player_id)
        .ok_or(SimulationError::PlayerNotFound(player_id))?
        .1
        .available_resources();
    let cost = runtime.cost.max(0);
    if available < cost {
        return Err(SimulationError::NotEnoughMana {
            player: player_id,
            required: cost,
            available,
        });
    }
    if kind == EntityKind::Minion {
        validate_board_position(world, player_id, board_index)?;
    } else {
        validate_zone_position(world, player_id, Zone::Graveyard, board_index)?;
    }
    Ok(())
}

fn validate_attack(
    world: &World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) -> Result<(), SimulationError> {
    let attacker = controlled_entity_in_zone(world, player_id, attacker_id, Zone::Play)?;
    let attack_state = world
        .get::<AttackState>(attacker)
        .copied()
        .ok_or(SimulationError::CannotAttack(attacker_id))?;
    if attack_state.exhausted
        || world
            .get::<CurrentStats>(attacker)
            .is_none_or(|stats| stats.attack <= 0)
    {
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
    Ok(())
}

pub(super) fn drive_resolution(world: &mut World) -> Result<(), SimulationError> {
    while let Some(operation) = pop_resolution_op(world) {
        consume_budget(world, operation.id)?;
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::OperationPopped {
                id: operation.id,
                kind: operation.operation.kind().to_string(),
            });
        world.resource_mut::<CurrentResolutionOp>().0 = Some(operation);
        world.run_schedule(ResolveFrame);
        if let Some(error) = world.resource_mut::<OperationFailure>().0.take() {
            return Err(error);
        }
        if world.resource::<GameState>().status == SimulationStatus::AwaitingChoice {
            break;
        }
    }
    Ok(())
}

fn finish_resolution_if_idle(world: &mut World) -> Result<(), SimulationError> {
    if world.resource::<GameState>().status == SimulationStatus::AwaitingChoice {
        return Ok(());
    }
    if !world.resource::<ResolutionWork>().stack.is_empty() {
        return Ok(());
    }
    finish_sequence(world);
    world.resource_mut::<GameState>().status = if world.resource::<GameState>().outcome.is_some() {
        SimulationStatus::Complete
    } else {
        SimulationStatus::AwaitingAction
    };
    assert_zone_invariants(world).map_err(SimulationError::Invariant)?;
    assert_player_role_invariants(world).map_err(SimulationError::Invariant)?;
    assert_game_entity_index(world).map_err(SimulationError::Invariant)
}

fn answer_choice(world: &mut World, option: ChoiceId) -> Result<(), SimulationError> {
    let pending = world
        .resource_mut::<ResolutionWork>()
        .pending_choice
        .take()
        .ok_or(crate::resolver::ResolutionError::NoPendingChoice)?;
    let Some(selected) = pending
        .request
        .options
        .iter()
        .find(|candidate| candidate.id == option)
        .cloned()
    else {
        world.resource_mut::<ResolutionWork>().pending_choice = Some(pending);
        return Err(crate::resolver::ResolutionError::InvalidChoice(option).into());
    };
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ChoiceAnswered {
            choice: pending.request.id,
            option,
        });
    push_resolution_ops(world, selected.operations);
    Ok(())
}

pub(super) fn run_sequence_step(
    world: &mut World,
    step: &SequenceStep,
) -> Result<(), SimulationError> {
    match step {
        SequenceStep::PlayCard {
            player,
            card,
            target,
            board_index,
        } => play_card(world, *player, *card, *target, *board_index),
        SequenceStep::Attack {
            player,
            attacker,
            defender,
        } => {
            attack(world, *player, *attacker, *defender);
            Ok(())
        }
        SequenceStep::FinishAttack {
            player,
            attacker,
            defender,
        } => {
            finish_attack(world, *player, *attacker, *defender);
            Ok(())
        }
        SequenceStep::EndTurn { player } => {
            end_turn(world, *player);
            Ok(())
        }
        SequenceStep::AdvanceTurn { ending_player } => {
            advance_turn(world, *ending_player);
            Ok(())
        }
        SequenceStep::StartTurn { player, kind } => start_turn(world, *player, *kind),
        SequenceStep::Concede { player } => {
            concede(world, *player);
            Ok(())
        }
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
    let runtime = world
        .get::<CardRuntime>(card_entity)
        .cloned()
        .ok_or(SimulationError::NotPlayable(card_id))?;
    spend_resources(world, player_id, runtime.cost)?;
    let destination = if kind == EntityKind::Minion {
        Zone::Play
    } else {
        Zone::Graveyard
    };
    let outcome = move_entity(world, card_id, destination, board_index)?;
    let ZoneMoveOutcome::Moved { from, .. } = outcome else {
        return Err(SimulationError::Invariant(format!(
            "validated card move produced {outcome:?}"
        )));
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: card_id,
            from,
            to: destination,
        });
    let context = EffectContext {
        source: Some(card_id),
        controller: player_id,
        declared_target,
        origin: if kind == EntityKind::Spell {
            crate::EffectOrigin::Spell
        } else {
            crate::EffectOrigin::Other
        },
    };
    let mut operations = vec![ResolutionOp::PrepareEvent(EventContext {
        kind: EventKind::CardPlayed,
        source: Some(card_id),
        targets: declared_target.into_iter().collect(),
        controller: player_id,
        proposed_value: None,
        actual_value: None,
        simultaneous_ordinal: 0,
    })];
    if kind == EntityKind::Minion {
        operations.push(ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::Summoned,
            source: Some(card_id),
            targets: vec![card_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        }));
        operations.push(ResolutionOp::RefreshAuras(
            crate::AuraRefreshPlan::PlayedProvider(card_id),
        ));
    }
    operations.extend(
        runtime
            .program
            .into_iter()
            .map(|effect| ResolutionOp::RunEffect {
                context: context.clone(),
                effect,
                event: None,
            }),
    );
    push_resolution_ops(world, operations);
    Ok(())
}

fn attack(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) {
    let attacker = game_entity(world, attacker_id).expect("validated attacker remains indexed");
    let defender = game_entity(world, defender_id).expect("validated defender remains indexed");
    let attack_value = world
        .get::<CurrentStats>(attacker)
        .map_or(0, |stats| stats.attack);
    let counter_damage = world
        .get::<CurrentStats>(defender)
        .map_or(0, |stats| stats.attack);
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
    push_resolution_ops(
        world,
        [
            ResolutionOp::PrepareEvent(EventContext {
                kind: EventKind::Attack,
                source: Some(attacker_id),
                targets: vec![defender_id],
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            }),
            ResolutionOp::ProcessDamageBatch(damage),
            ResolutionOp::RunSequenceStep(SequenceStep::FinishAttack {
                player: player_id,
                attacker: attacker_id,
                defender: defender_id,
            }),
        ],
    );
}

fn finish_attack(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) {
    if let Some(attacker) = game_entity(world, attacker_id)
        && let Some(mut state) = world.get_mut::<AttackState>(attacker)
    {
        state.attacks_this_turn += 1;
        state.exhausted = true;
    }
    push_resolution_ops(
        world,
        [ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::AfterAttack,
            source: Some(attacker_id),
            targets: vec![defender_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        })],
    );
}

fn end_turn(world: &mut World, player_id: PlayerId) {
    push_resolution_ops(
        world,
        [
            ResolutionOp::PrepareEvent(EventContext {
                kind: EventKind::TurnEnded,
                source: None,
                targets: Vec::new(),
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            }),
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::CheckOutcome,
            ResolutionOp::RunSequenceStep(SequenceStep::AdvanceTurn {
                ending_player: player_id,
            }),
        ],
    );
}

fn advance_turn(world: &mut World, ending_player: PlayerId) {
    if world.resource::<GameState>().outcome.is_some() {
        return;
    }
    let next = world
        .resource_mut::<TurnSchedule>()
        .next_turn(ending_player);
    expire_temporary_effects(world, ending_player, next.player);
    push_resolution_ops(
        world,
        [ResolutionOp::RunSequenceStep(SequenceStep::StartTurn {
            player: next.player,
            kind: next.kind,
        })],
    );
}

fn expire_temporary_effects(world: &mut World, ending_player: PlayerId, next_player: PlayerId) {
    let expiring = world
        .iter_entities()
        .filter_map(|entity| {
            let duration = *entity.get::<TemporaryDuration>()?;
            let expires = match duration {
                TemporaryDuration::EndOfTurn(player) => player == ending_player,
                TemporaryDuration::EndOfTurnSeries(player) => {
                    player == ending_player && next_player != ending_player
                }
            };
            if !expires {
                return None;
            }
            Some((
                *entity.get::<GameEntityId>()?,
                entity.get::<Controller>()?.0,
                entity.get::<AttachedTo>().map(|attached| attached.0),
                entity.id(),
            ))
        })
        .collect::<Vec<_>>();
    for (id, controller, attached_to, entity) in expiring {
        let target = attached_to.and_then(|target| world.get::<GameEntityId>(target).copied());
        world.entity_mut(entity).remove::<AttachedTo>();
        let _ = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: id,
                destination_controller: controller,
                destination: Zone::RemovedFromGame,
                position: None,
                kind: ZoneMovementKind::DetachEnchantment,
            },
        );
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::TemporaryEffectExpired { entity: id });
        if let Some(target) = target {
            recalculate_stats(world, target);
            recalculate_keywords(world, target);
            recalculate_cost(world, target);
        }
    }
}

fn start_turn(
    world: &mut World,
    player_id: PlayerId,
    turn_kind: ScheduledTurnKind,
) -> Result<(), SimulationError> {
    let maximum_mana = world.resource::<Ruleset>().maximum_mana;
    {
        let mut game = world.resource_mut::<GameState>();
        game.active_player = player_id;
        game.turn_number += 1;
    }
    let (_, mut player, _, _) = player_mut(world, player_id)?;
    player.maximum_resources = (player.maximum_resources + 1).min(maximum_mana);
    player.used_resources = 0;
    player.temporary_resources = 0;
    player.locked_overload = player.pending_overload;
    player.pending_overload = 0;
    let turn_entities = world
        .resource::<ZoneIndex>()
        .entities(player_id, Zone::Play)
        .to_vec();
    for id in turn_entities {
        if let Some(entity) = game_entity(world, id) {
            if let Some(mut state) = world.get_mut::<AttackState>(entity) {
                state.attacks_this_turn = 0;
                state.exhausted = false;
            }
            if let Some(mut state) = world.get_mut::<HeroPowerState>(entity) {
                state.uses_this_turn = 0;
                state.exhausted = false;
            }
        }
    }
    let turn = world.resource::<GameState>().turn_number;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TurnChanged {
            active_player: player_id,
            turn,
            kind: turn_kind,
        });
    push_resolution_ops(
        world,
        [ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::TurnStarted,
            source: None,
            targets: Vec::new(),
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        })],
    );
    Ok(())
}

fn concede(world: &mut World, player: PlayerId) {
    let winner = player.opponent();
    world.resource_mut::<GameState>().outcome = Some(GameOutcome::Winner(winner));
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Outcome {
            winner: Some(winner),
        });
}

fn spend_resources(
    world: &mut World,
    player_id: PlayerId,
    amount: i32,
) -> Result<(), SimulationError> {
    let amount = amount.max(0);
    let (_, mut player, _, _) = player_mut(world, player_id)?;
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
