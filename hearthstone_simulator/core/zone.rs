use std::collections::BTreeMap;

use bevy::prelude::*;
use thiserror::Error;

use crate::{Controller, GameEntityId, PlayerId, Ruleset, entity::game_entity};

#[derive(Component, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Zone {
    Deck,
    Hand,
    Play,
    Secret,
    Graveyard,
    SetAside,
    RemovedFromGame,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ZonePosition(pub usize);

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
pub struct ZoneIndex(pub BTreeMap<(PlayerId, Zone), Vec<GameEntityId>>);

impl ZoneIndex {
    pub fn entities(&self, player: PlayerId, zone: Zone) -> &[GameEntityId] {
        self.0
            .get(&(player, zone))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ZoneError {
    #[error("game entity {0:?} does not exist")]
    EntityNotFound(GameEntityId),
    #[error("game entity {entity:?} is absent from authoritative {zone:?} index")]
    MissingIndexEntry { entity: GameEntityId, zone: Zone },
    #[error("{zone:?} is full for player {player:?}")]
    Full { player: PlayerId, zone: Zone },
    #[error("position {position} is outside {zone:?} of length {length}")]
    InvalidPosition {
        zone: Zone,
        position: usize,
        length: usize,
    },
}

pub(crate) fn zone_limit(ruleset: &Ruleset, zone: Zone) -> Option<usize> {
    match zone {
        Zone::Hand => Some(ruleset.hand_limit),
        Zone::Play => Some(ruleset.board_limit),
        _ => None,
    }
}

pub(crate) fn insert_into_zone(
    world: &mut World,
    id: GameEntityId,
    player: PlayerId,
    zone: Zone,
    position: Option<usize>,
) -> Result<(), ZoneError> {
    let entity = game_entity(world, id).ok_or(ZoneError::EntityNotFound(id))?;
    let limit = zone_limit(world.resource::<Ruleset>(), zone);
    let mut index = world.resource_mut::<ZoneIndex>();
    let entries = index.0.entry((player, zone)).or_default();
    if limit.is_some_and(|limit| entries.len() >= limit) {
        return Err(ZoneError::Full { player, zone });
    }
    let position = position.unwrap_or(entries.len());
    if position > entries.len() {
        return Err(ZoneError::InvalidPosition {
            zone,
            position,
            length: entries.len(),
        });
    }
    entries.insert(position, id);
    world.entity_mut(entity).insert((Controller(player), zone));
    refresh_positions(world, player, zone);
    Ok(())
}

pub(crate) fn move_entity(
    world: &mut World,
    id: GameEntityId,
    destination: Zone,
    position: Option<usize>,
) -> Result<(Zone, usize), ZoneError> {
    let entity = game_entity(world, id).ok_or(ZoneError::EntityNotFound(id))?;
    let source = *world
        .get::<Zone>(entity)
        .ok_or(ZoneError::EntityNotFound(id))?;
    let player = world
        .get::<Controller>(entity)
        .map(|controller| controller.0)
        .ok_or(ZoneError::EntityNotFound(id))?;
    let source_position = remove_from_index(world, id, player, source)?;

    if let Err(error) = insert_into_zone(world, id, player, destination, position) {
        insert_into_zone(world, id, player, source, Some(source_position))
            .expect("rolling back a valid zone move must succeed");
        return Err(error);
    }
    Ok((source, source_position))
}

fn remove_from_index(
    world: &mut World,
    id: GameEntityId,
    player: PlayerId,
    zone: Zone,
) -> Result<usize, ZoneError> {
    let mut index = world.resource_mut::<ZoneIndex>();
    let entries = index.0.entry((player, zone)).or_default();
    let position = entries
        .iter()
        .position(|candidate| *candidate == id)
        .ok_or(ZoneError::MissingIndexEntry { entity: id, zone })?;
    entries.remove(position);
    refresh_positions(world, player, zone);
    Ok(position)
}

fn refresh_positions(world: &mut World, player: PlayerId, zone: Zone) {
    let entries = world
        .resource::<ZoneIndex>()
        .entities(player, zone)
        .to_vec();
    for (position, id) in entries.into_iter().enumerate() {
        if let Some(entity) = game_entity(world, id) {
            world.entity_mut(entity).insert(ZonePosition(position));
        }
    }
}

pub(crate) fn assert_zone_invariants(world: &World) -> Result<(), String> {
    let index = world.resource::<ZoneIndex>();
    for ((player, zone), entries) in &index.0 {
        for (position, id) in entries.iter().enumerate() {
            let entity = game_entity(world, *id)
                .ok_or_else(|| format!("zone index references missing {id:?}"))?;
            let actual_zone = world
                .get::<Zone>(entity)
                .ok_or_else(|| format!("indexed entity {id:?} has no Zone"))?;
            let controller = world
                .get::<Controller>(entity)
                .ok_or_else(|| format!("indexed entity {id:?} has no Controller"))?;
            let actual_position = world
                .get::<ZonePosition>(entity)
                .ok_or_else(|| format!("indexed entity {id:?} has no ZonePosition"))?;
            if actual_zone != zone || controller.0 != *player || actual_position.0 != position {
                return Err(format!("zone index disagrees for {id:?}"));
            }
        }
    }
    Ok(())
}
