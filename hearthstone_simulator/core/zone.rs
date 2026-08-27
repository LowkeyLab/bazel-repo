use std::collections::BTreeMap;

use bevy::prelude::*;
use thiserror::Error;

use crate::{Controller, EntityKind, GameEntityId, PlayerId, Ruleset, entity::game_entity};

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
        _ => None,
    }
}

pub(crate) fn board_is_full(world: &World, player: PlayerId) -> bool {
    world
        .resource::<ZoneIndex>()
        .entities(player, Zone::Play)
        .iter()
        .filter(|id| {
            game_entity(world, **id).and_then(|entity| world.get::<EntityKind>(entity))
                == Some(&EntityKind::Minion)
        })
        .count()
        >= world.resource::<Ruleset>().board_limit
}

pub(crate) fn validate_zone_position(
    world: &World,
    player: PlayerId,
    zone: Zone,
    position: Option<usize>,
) -> Result<(), ZoneError> {
    let length = world.resource::<ZoneIndex>().entities(player, zone).len();
    if let Some(position) = position
        && position > length
    {
        return Err(ZoneError::InvalidPosition {
            zone,
            position,
            length,
        });
    }
    Ok(())
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
    let entries = world.resource::<ZoneIndex>().entities(player, zone);
    if limit.is_some_and(|limit| entries.len() >= limit)
        || (zone == Zone::Play
            && world.get::<EntityKind>(entity) == Some(&EntityKind::Minion)
            && board_is_full(world, player))
    {
        return Err(ZoneError::Full { player, zone });
    }
    validate_zone_position(world, player, zone, position)?;
    let mut index = world.resource_mut::<ZoneIndex>();
    let entries = index.0.entry((player, zone)).or_default();
    let position = position.unwrap_or(entries.len());
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

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{GameObject, entity::GameEntityIndex};

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.init_resource::<ZoneIndex>();
        world.init_resource::<Ruleset>();
        world
    }

    #[googletest::test]
    fn insertion_validates_entity_capacity_and_position() {
        let mut world = world();
        world.resource_mut::<Ruleset>().hand_limit = 1;
        world.spawn((GameObject, GameEntityId(1)));
        world.spawn((GameObject, GameEntityId(2)));

        assert_that!(
            insert_into_zone(
                &mut world,
                GameEntityId(99),
                PlayerId::One,
                Zone::Hand,
                None
            ),
            err(eq(&ZoneError::EntityNotFound(GameEntityId(99))))
        );
        assert_that!(
            insert_into_zone(
                &mut world,
                GameEntityId(1),
                PlayerId::One,
                Zone::Deck,
                Some(1)
            ),
            err(eq(&ZoneError::InvalidPosition {
                zone: Zone::Deck,
                position: 1,
                length: 0,
            }))
        );
        insert_into_zone(&mut world, GameEntityId(1), PlayerId::One, Zone::Hand, None).unwrap();
        assert_that!(
            insert_into_zone(&mut world, GameEntityId(2), PlayerId::One, Zone::Hand, None),
            err(eq(&ZoneError::Full {
                player: PlayerId::One,
                zone: Zone::Hand,
            }))
        );
        assert_that!(zone_limit(world.resource::<Ruleset>(), Zone::Play), none());
        assert_that!(zone_limit(world.resource::<Ruleset>(), Zone::Deck), none());
    }

    #[googletest::test]
    fn play_zone_capacity_counts_only_minions() {
        let mut world = world();
        world.resource_mut::<Ruleset>().board_limit = 1;
        world.spawn((GameObject, GameEntityId(1), EntityKind::Hero));
        world.spawn((GameObject, GameEntityId(2), EntityKind::Minion));
        world.spawn((GameObject, GameEntityId(3), EntityKind::Minion));

        insert_into_zone(&mut world, GameEntityId(1), PlayerId::One, Zone::Play, None).unwrap();
        assert_that!(board_is_full(&world, PlayerId::One), is_false());
        insert_into_zone(&mut world, GameEntityId(2), PlayerId::One, Zone::Play, None).unwrap();
        assert_that!(board_is_full(&world, PlayerId::One), is_true());
        assert_that!(
            insert_into_zone(&mut world, GameEntityId(3), PlayerId::One, Zone::Play, None,),
            err(eq(&ZoneError::Full {
                player: PlayerId::One,
                zone: Zone::Play,
            }))
        );
        assert_that!(
            validate_zone_position(&world, PlayerId::One, Zone::Play, Some(3)),
            err(eq(&ZoneError::InvalidPosition {
                zone: Zone::Play,
                position: 3,
                length: 2,
            }))
        );
    }

    #[googletest::test]
    fn failed_moves_restore_the_source_index_and_positions() {
        let mut world = world();
        world.spawn((GameObject, GameEntityId(1)));
        world.spawn((GameObject, GameEntityId(2)));
        insert_into_zone(&mut world, GameEntityId(1), PlayerId::One, Zone::Deck, None).unwrap();
        insert_into_zone(&mut world, GameEntityId(2), PlayerId::One, Zone::Deck, None).unwrap();

        assert_that!(
            matches!(
                move_entity(&mut world, GameEntityId(1), Zone::Hand, Some(2)),
                Err(ZoneError::InvalidPosition { .. })
            ),
            is_true()
        );
        assert_that!(
            world
                .resource::<ZoneIndex>()
                .entities(PlayerId::One, Zone::Deck),
            eq(&[GameEntityId(1), GameEntityId(2)])
        );
        assert_that!(
            world.get::<ZonePosition>(game_entity(&world, GameEntityId(2)).unwrap()),
            eq(Some(&ZonePosition(1)))
        );

        world
            .resource_mut::<ZoneIndex>()
            .0
            .get_mut(&(PlayerId::One, Zone::Deck))
            .unwrap()
            .clear();
        assert_that!(
            matches!(
                move_entity(&mut world, GameEntityId(1), Zone::Hand, None),
                Err(ZoneError::MissingIndexEntry { .. })
            ),
            is_true()
        );
    }

    #[googletest::test]
    fn invariant_errors_identify_each_kind_of_index_drift() {
        let mut world = world();
        world
            .resource_mut::<ZoneIndex>()
            .0
            .insert((PlayerId::One, Zone::Deck), vec![GameEntityId(99)]);
        assert_that!(
            assert_zone_invariants(&world),
            err(eq(
                &"zone index references missing GameEntityId(99)".to_string()
            ))
        );

        world.spawn((GameObject, GameEntityId(99)));
        assert_that!(
            assert_zone_invariants(&world),
            err(eq(
                &"indexed entity GameEntityId(99) has no Zone".to_string()
            ))
        );
        let entity = game_entity(&world, GameEntityId(99)).unwrap();
        world.entity_mut(entity).insert(Zone::Deck);
        assert_that!(
            assert_zone_invariants(&world),
            err(eq(
                &"indexed entity GameEntityId(99) has no Controller".to_string()
            ))
        );
        world.entity_mut(entity).insert(Controller(PlayerId::One));
        assert_that!(
            assert_zone_invariants(&world),
            err(eq(
                &"indexed entity GameEntityId(99) has no ZonePosition".to_string()
            ))
        );
        world.entity_mut(entity).insert(ZonePosition(1));
        assert_that!(
            assert_zone_invariants(&world),
            err(eq(&"zone index disagrees for GameEntityId(99)".to_string()))
        );
        world.entity_mut(entity).insert(ZonePosition(0));
        assert_zone_invariants(&world).unwrap();
    }
}
