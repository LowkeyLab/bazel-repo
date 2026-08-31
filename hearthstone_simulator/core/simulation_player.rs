use bevy::prelude::*;

use crate::{
    Armor, AttackState, CanonicalTrace, Controller, CurrentStats, Damage, EntityKind, GameEntityId,
    GameOutcome, GameState, HeroMetadata, HeroPowerState, Player, PlayerId, TraceEntry, Zone,
    death::DefeatedHeroes,
    entity::game_entity,
    zone::{
        ZoneIndex, ZoneMoveOutcome, ZoneMoveRequest, ZoneMovementKind, move_entity_with_request,
    },
};

use super::{card_runtime::CardRuntime, error::SimulationError, health::apply_damage};

pub(super) fn assert_player_role_invariants(world: &World) -> Result<(), String> {
    for player_id in PlayerId::ALL {
        let players = world
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<Player>()
                    .is_some_and(|player| player.id == player_id)
            })
            .collect::<Vec<_>>();
        if players.len() != 1 {
            return Err(format!(
                "player {player_id:?} has {} Player entities instead of one",
                players.len()
            ));
        }
        let player_entity = players[0];
        if player_entity
            .get::<Controller>()
            .map(|controller| controller.0)
            != Some(player_id)
            || player_entity.get::<EntityKind>() != Some(&EntityKind::Player)
        {
            return Err(format!(
                "player {player_id:?} has invalid Player components"
            ));
        }

        let active = world
            .resource::<ZoneIndex>()
            .entities(player_id, Zone::Play);
        let heroes = active_kind(world, active, EntityKind::Hero);
        if heroes.len() != 1 {
            return Err(format!(
                "player {player_id:?} has {} active Heroes instead of one",
                heroes.len()
            ));
        }
        let hero = game_entity(world, heroes[0])
            .ok_or_else(|| format!("player {player_id:?} Hero is missing"))?;
        if world.get::<CurrentStats>(hero).is_none()
            || world.get::<Damage>(hero).is_none()
            || world.get::<Armor>(hero).is_none()
            || world.get::<AttackState>(hero).is_none()
            || world.get::<HeroMetadata>(hero).is_none()
        {
            return Err(format!(
                "player {player_id:?} Hero lacks required components"
            ));
        }

        let powers = active_kind(world, active, EntityKind::HeroPower);
        if powers.len() != 1 {
            return Err(format!(
                "player {player_id:?} has {} active Hero Powers instead of one",
                powers.len()
            ));
        }
        let power = game_entity(world, powers[0])
            .ok_or_else(|| format!("player {player_id:?} Hero Power is missing"))?;
        if world.get::<HeroPowerState>(power).is_none() || world.get::<CardRuntime>(power).is_none()
        {
            return Err(format!(
                "player {player_id:?} Hero Power lacks required components"
            ));
        }
    }
    Ok(())
}

fn active_kind(world: &World, active: &[GameEntityId], kind: EntityKind) -> Vec<GameEntityId> {
    active
        .iter()
        .copied()
        .filter(|id| {
            game_entity(world, *id).and_then(|entity| world.get::<EntityKind>(entity))
                == Some(&kind)
        })
        .collect()
}

pub(super) fn draw_card(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
    let card = world
        .resource::<ZoneIndex>()
        .entities(player_id, Zone::Deck)
        .first()
        .copied();
    if let Some(card) = card {
        let outcome = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: card,
                destination_controller: player_id,
                destination: Zone::Hand,
                position: None,
                kind: ZoneMovementKind::Draw,
            },
        )?;
        let (from, destination) = match outcome {
            ZoneMoveOutcome::Moved { from, .. } => (from, Zone::Hand),
            ZoneMoveOutcome::FullZoneRemoval { from, .. } => (from, Zone::Graveyard),
            ZoneMoveOutcome::PreventedByFullZone => {
                return Err(SimulationError::Invariant(
                    "draw was unexpectedly prevented by a full hand".to_string(),
                ));
            }
        };
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

pub(super) fn hero_id(world: &World, player: PlayerId) -> Option<GameEntityId> {
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

pub(super) fn check_outcome(world: &mut World) {
    if world.resource::<GameState>().outcome.is_some() {
        return;
    }
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

pub(super) fn controlled_entity_in_zone(
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

pub(super) fn player(
    world: &World,
    id: PlayerId,
) -> Option<(Entity, &Player, &CurrentStats, &Damage)> {
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

pub(super) fn player_mut(
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
