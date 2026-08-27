use bevy::prelude::*;

use crate::{
    CanonicalTrace, Controller, CurrentStats, Damage, EntityKind, GameEntityId, GameOutcome,
    GameState, Player, PlayerId, Ruleset, TraceEntry, Zone,
    death::DefeatedHeroes,
    entity::game_entity,
    zone::{ZoneIndex, move_entity},
};

use super::{error::SimulationError, health::apply_damage};

pub(super) fn draw_card(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
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
