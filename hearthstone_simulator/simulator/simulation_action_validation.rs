use bevy::prelude::*;
use hearthstone_simulator_core::{TargetAudience, TargetFilter, TargetKind};

use super::{
    card_runtime::CardRuntime,
    effect_executor::{validate_effect_program, validate_play_effect_program},
    error::SimulationError,
    player::{controlled_entity_in_zone, player},
};
use crate::{
    AttackState, Controller, CurrentStats, EntityKind, GameAction, GameEntityId, GameState,
    PlayerId, RuntimeTriggers, SimulationStatus, TargetRequirement, Zone,
    entity::game_entity,
    zone::{board_entities, board_is_full, validate_board_position},
};

pub(super) fn eligible_targets(
    world: &World,
    player: PlayerId,
    filter: TargetFilter,
) -> Vec<GameEntityId> {
    let players = match filter.audience {
        TargetAudience::Friendly => vec![player],
        TargetAudience::Enemy => vec![player.opponent()],
        TargetAudience::Either => PlayerId::ALL.to_vec(),
    };
    let mut targets = players
        .iter()
        .flat_map(|candidate_player| {
            world
                .resource::<crate::zone::ZoneIndex>()
                .entities(*candidate_player, Zone::Play)
                .iter()
                .copied()
        })
        .filter(|candidate| target_matches(world, *candidate, &players, filter.kind))
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();
    targets
}

#[allow(dead_code, reason = "consumed by action enumeration in the next task")]
pub(super) fn target_options(
    world: &World,
    player: PlayerId,
    requirement: TargetRequirement,
) -> Vec<Option<GameEntityId>> {
    match requirement {
        TargetRequirement::None => vec![None],
        TargetRequirement::Required(filter) => eligible_targets(world, player, filter)
            .into_iter()
            .map(Some)
            .collect(),
        TargetRequirement::Optional(filter) => std::iter::once(None)
            .chain(
                eligible_targets(world, player, filter)
                    .into_iter()
                    .map(Some),
            )
            .collect(),
        TargetRequirement::RequiredIfAvailable(filter) => {
            let targets = eligible_targets(world, player, filter);
            if targets.is_empty() {
                vec![None]
            } else {
                targets.into_iter().map(Some).collect()
            }
        }
    }
}

fn target_matches(
    world: &World,
    candidate: GameEntityId,
    players: &[PlayerId],
    kind: TargetKind,
) -> bool {
    let Some(entity) = game_entity(world, candidate) else {
        return false;
    };
    if world.get::<Zone>(entity) != Some(&Zone::Play)
        || world
            .get::<Controller>(entity)
            .is_none_or(|controller| !players.contains(&controller.0))
    {
        return false;
    }
    world
        .get::<EntityKind>(entity)
        .is_some_and(|candidate_kind| {
            matches!(
                (kind, *candidate_kind),
                (TargetKind::Minion, EntityKind::Minion)
                    | (TargetKind::Hero, EntityKind::Hero)
                    | (TargetKind::Character, EntityKind::Minion | EntityKind::Hero)
            )
        })
}

pub(super) fn validate_action(
    world: &World,
    action: &GameAction,
) -> Result<GameAction, SimulationError> {
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
            target,
            board_index,
            choice,
        } => {
            let board_index =
                validate_play_card(world, *player, *card, *target, *board_index, *choice)?;
            Ok(GameAction::PlayCard {
                player: *player,
                card: *card,
                target: *target,
                board_index,
                choice: *choice,
            })
        }
        GameAction::Attack {
            player,
            attacker,
            defender,
        } => {
            validate_attack(world, *player, *attacker, *defender)?;
            Ok(action.clone())
        }
        GameAction::EndTurn { .. } | GameAction::Concede { .. } => Ok(action.clone()),
    }
}

fn validate_play_card(
    world: &World,
    player_id: PlayerId,
    card_id: GameEntityId,
    target: Option<GameEntityId>,
    board_index: Option<usize>,
    choice: Option<crate::ChoiceId>,
) -> Result<Option<usize>, SimulationError> {
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
    if kind == EntityKind::Minion {
        validate_play_effect_program(world, &runtime.program)?;
    } else {
        validate_effect_program(world, &runtime.program, None)?;
    }
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
    if let Some(choice) = choice {
        return Err(SimulationError::UnsupportedActionChoice(choice));
    }
    validate_target(world, player_id, card_id, target, runtime.targeting)?;
    if kind == EntityKind::Minion {
        validate_board_position(world, player_id, board_index)?;
        Ok(Some(
            board_index.unwrap_or_else(|| board_entities(world, player_id).len()),
        ))
    } else {
        if board_index.is_some() {
            return Err(SimulationError::UnexpectedBoardPosition(card_id));
        }
        Ok(None)
    }
}

fn validate_target(
    world: &World,
    player: PlayerId,
    card: GameEntityId,
    target: Option<GameEntityId>,
    requirement: TargetRequirement,
) -> Result<(), SimulationError> {
    match requirement {
        TargetRequirement::None => match target {
            None => Ok(()),
            Some(_) => Err(SimulationError::UnexpectedTarget(card)),
        },
        TargetRequirement::Required(filter) => match target {
            None => Err(SimulationError::MissingTarget(card)),
            Some(target) => validate_supplied_target(world, player, card, target, filter),
        },
        TargetRequirement::Optional(filter) => match target {
            None => Ok(()),
            Some(target) => validate_supplied_target(world, player, card, target, filter),
        },
        TargetRequirement::RequiredIfAvailable(filter) => match target {
            None if eligible_targets(world, player, filter).is_empty() => Ok(()),
            None => Err(SimulationError::MissingTarget(card)),
            Some(target) => validate_supplied_target(world, player, card, target, filter),
        },
    }
}

fn validate_supplied_target(
    world: &World,
    player: PlayerId,
    card: GameEntityId,
    target: GameEntityId,
    filter: TargetFilter,
) -> Result<(), SimulationError> {
    if eligible_targets(world, player, filter).contains(&target) {
        Ok(())
    } else {
        Err(SimulationError::InvalidTarget { card, target })
    }
}

fn validate_attack(
    world: &World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) -> Result<(), SimulationError> {
    let attacker = controlled_entity_in_zone(world, player_id, attacker_id, Zone::Play)?;
    if world
        .get::<EntityKind>(attacker)
        .is_none_or(|kind| !matches!(kind, EntityKind::Hero | EntityKind::Minion))
    {
        return Err(SimulationError::CannotAttack(attacker_id));
    }
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
        || world
            .get::<EntityKind>(defender)
            .is_none_or(|kind| !matches!(kind, EntityKind::Hero | EntityKind::Minion))
    {
        return Err(SimulationError::InvalidDefender(defender_id));
    }
    Ok(())
}
