use std::collections::BTreeMap;

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    Armor, AttachedTo, AttackAuraCache, AttackState, BaseKeywords, BaseStats, Controller,
    CurrentStats, Damage, DefinitionId, EntityKind, GameEntityId, HealthAuraCache,
    KeepEnchantments, Keywords, OtherAuraCache, PendingDestroy, PlayerId, Ruleset, Silenced,
    TemporaryDuration,
    enchantment::{recalculate_keywords, recalculate_stats},
    entity::{allocate_play_order, game_entity},
};

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
)]
pub enum Zone {
    Deck,
    Hand,
    Play,
    Secret,
    Graveyard,
    SetAside,
    RemovedFromGame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ZoneMovementKind {
    Normal,
    Draw,
    ForcePlay,
    Death,
    Discard,
    DetachEnchantment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ZoneMoveRequest {
    pub entity: GameEntityId,
    pub destination_controller: PlayerId,
    pub destination: Zone,
    pub position: Option<usize>,
    pub kind: ZoneMovementKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneMoveOutcome {
    Moved {
        from: Zone,
        from_controller: PlayerId,
        from_position: usize,
    },
    PreventedByFullZone,
    FullZoneRemoval {
        from: Zone,
        from_controller: PlayerId,
        from_position: usize,
    },
}

#[derive(
    Component, Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize,
)]
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
        Zone::Deck => Some(ruleset.deck_limit),
        Zone::Hand => Some(ruleset.hand_limit),
        Zone::Secret => Some(ruleset.secret_limit),
        _ => None,
    }
}

pub(crate) fn board_entities(world: &World, player: PlayerId) -> Vec<GameEntityId> {
    world
        .resource::<ZoneIndex>()
        .entities(player, Zone::Play)
        .iter()
        .copied()
        .filter(|id| {
            game_entity(world, *id)
                .and_then(|entity| world.get::<EntityKind>(entity))
                .is_some_and(|kind| is_board_entity(*kind))
        })
        .collect()
}

pub(crate) fn board_is_full(world: &World, player: PlayerId) -> bool {
    board_entities(world, player).len() >= world.resource::<Ruleset>().board_limit
}

const fn is_board_entity(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Minion | EntityKind::Location | EntityKind::Permanent | EntityKind::Dormant
    )
}

pub(crate) fn validate_generation_capacity(
    world: &World,
    player: PlayerId,
    zone: Zone,
    kind: EntityKind,
    definition_id: &str,
) -> Result<(), ZoneError> {
    if generated_destination_is_full(world, player, zone, kind, definition_id) {
        return Err(ZoneError::Full { player, zone });
    }
    Ok(())
}

pub(crate) fn validate_board_position(
    world: &World,
    player: PlayerId,
    position: Option<usize>,
) -> Result<(), ZoneError> {
    let length = board_entities(world, player).len();
    if let Some(position) = position
        && position > length
    {
        return Err(ZoneError::InvalidPosition {
            zone: Zone::Play,
            position,
            length,
        });
    }
    Ok(())
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
    if destination_is_full(world, entity, player, zone) {
        return Err(ZoneError::Full { player, zone });
    }
    validate_zone_position(world, player, zone, position)?;
    insert_into_zone_unchecked(world, id, player, zone, position);
    Ok(())
}

fn insert_into_zone_unchecked(
    world: &mut World,
    id: GameEntityId,
    player: PlayerId,
    zone: Zone,
    position: Option<usize>,
) {
    let entity = game_entity(world, id).expect("unchecked zone insertion requires an entity");
    let mut index = world.resource_mut::<ZoneIndex>();
    let entries = index.0.entry((player, zone)).or_default();
    let position = position.unwrap_or(entries.len());
    entries.insert(position, id);
    world.entity_mut(entity).insert((Controller(player), zone));
    refresh_positions(world, player, zone);
}

pub(crate) fn move_entity(
    world: &mut World,
    id: GameEntityId,
    destination: Zone,
    position: Option<usize>,
) -> Result<ZoneMoveOutcome, ZoneError> {
    let entity = game_entity(world, id).ok_or(ZoneError::EntityNotFound(id))?;
    let controller = world
        .get::<Controller>(entity)
        .map(|controller| controller.0)
        .ok_or(ZoneError::EntityNotFound(id))?;
    move_entity_with_request(
        world,
        ZoneMoveRequest {
            entity: id,
            destination_controller: controller,
            destination,
            position,
            kind: ZoneMovementKind::Normal,
        },
    )
}

pub(crate) fn move_entity_with_request(
    world: &mut World,
    request: ZoneMoveRequest,
) -> Result<ZoneMoveOutcome, ZoneError> {
    let entity =
        game_entity(world, request.entity).ok_or(ZoneError::EntityNotFound(request.entity))?;
    let source = *world
        .get::<Zone>(entity)
        .ok_or(ZoneError::EntityNotFound(request.entity))?;
    let source_controller = world
        .get::<Controller>(entity)
        .map(|controller| controller.0)
        .ok_or(ZoneError::EntityNotFound(request.entity))?;
    let same_zone =
        source == request.destination && source_controller == request.destination_controller;

    if !same_zone
        && destination_is_full(
            world,
            entity,
            request.destination_controller,
            request.destination,
        )
    {
        return match request.kind {
            ZoneMovementKind::ForcePlay => Ok(ZoneMoveOutcome::PreventedByFullZone),
            ZoneMovementKind::Draw => move_to_graveyard_after_failed_move(
                world,
                request.entity,
                source,
                source_controller,
                false,
            ),
            _ => move_to_graveyard_after_failed_move(
                world,
                request.entity,
                source,
                source_controller,
                source == Zone::Play,
            ),
        };
    }

    let remembered_position =
        semantic_zone_position(world, request.entity, source_controller, source).ok_or(
            ZoneError::MissingIndexEntry {
                entity: request.entity,
                zone: source,
            },
        )?;
    let source_position = remove_from_index(world, request.entity, source_controller, source)?;
    let destination_position = match resolve_destination_position(
        world,
        entity,
        request.destination_controller,
        request.destination,
        request.position,
        same_zone.then_some(source_position),
    ) {
        Ok(position) => position,
        Err(error) => {
            insert_into_zone_unchecked(
                world,
                request.entity,
                source_controller,
                source,
                Some(source_position),
            );
            return Err(error);
        }
    };
    let insertion = if same_zone {
        insert_into_zone_unchecked(
            world,
            request.entity,
            request.destination_controller,
            request.destination,
            destination_position,
        );
        Ok(())
    } else {
        insert_into_zone(
            world,
            request.entity,
            request.destination_controller,
            request.destination,
            destination_position,
        )
    };
    if let Err(error) = insertion {
        insert_into_zone_unchecked(
            world,
            request.entity,
            source_controller,
            source,
            Some(source_position),
        );
        return Err(error);
    }

    apply_movement_state_policy(world, request, source);
    Ok(ZoneMoveOutcome::Moved {
        from: source,
        from_controller: source_controller,
        from_position: remembered_position,
    })
}

fn destination_is_full(world: &World, entity: Entity, player: PlayerId, zone: Zone) -> bool {
    let entries = world.resource::<ZoneIndex>().entities(player, zone);
    if zone_limit(world.resource::<Ruleset>(), zone).is_some_and(|limit| entries.len() >= limit) {
        return true;
    }
    let Some(kind) = world.get::<EntityKind>(entity).copied() else {
        return false;
    };
    let definition_id = world
        .get::<DefinitionId>(entity)
        .map_or("", |definition| definition.0.as_str());
    generated_destination_is_full(world, player, zone, kind, definition_id)
}

fn generated_destination_is_full(
    world: &World,
    player: PlayerId,
    zone: Zone,
    kind: EntityKind,
    definition_id: &str,
) -> bool {
    let entries = world.resource::<ZoneIndex>().entities(player, zone);
    let ruleset = world.resource::<Ruleset>();
    if zone_limit(ruleset, zone).is_some_and(|limit| entries.len() >= limit) {
        return true;
    }
    if zone == Zone::Play {
        let kind_limit = match kind {
            EntityKind::Hero => Some(ruleset.hero_limit),
            EntityKind::Weapon => Some(ruleset.weapon_limit),
            EntityKind::HeroPower => Some(ruleset.hero_power_limit),
            _ => None,
        };
        if (is_board_entity(kind) && board_is_full(world, player))
            || kind_limit.is_some_and(|limit| count_kind(world, entries, kind) >= limit)
        {
            return true;
        }
    }
    if zone == Zone::Secret {
        if kind == EntityKind::Quest
            && count_kind(world, entries, EntityKind::Quest) >= ruleset.quest_limit
        {
            return true;
        }
        if matches!(kind, EntityKind::Secret | EntityKind::Sidequest)
            && entries.iter().any(|id| {
                game_entity(world, *id)
                    .and_then(|entity| world.get::<DefinitionId>(entity))
                    .is_some_and(|definition| definition.0 == definition_id)
            })
        {
            return true;
        }
    }
    false
}

fn count_kind(world: &World, entries: &[GameEntityId], kind: EntityKind) -> usize {
    entries
        .iter()
        .filter(|id| {
            game_entity(world, **id).and_then(|entity| world.get::<EntityKind>(entity))
                == Some(&kind)
        })
        .count()
}

pub(crate) fn semantic_zone_position(
    world: &World,
    id: GameEntityId,
    player: PlayerId,
    zone: Zone,
) -> Option<usize> {
    if zone == Zone::Play
        && game_entity(world, id)
            .and_then(|entity| world.get::<EntityKind>(entity))
            .is_some_and(|kind| is_board_entity(*kind))
    {
        return board_entities(world, player)
            .iter()
            .position(|candidate| *candidate == id);
    }
    world
        .resource::<ZoneIndex>()
        .entities(player, zone)
        .iter()
        .position(|candidate| *candidate == id)
}

fn resolve_destination_position(
    world: &World,
    entity: Entity,
    player: PlayerId,
    zone: Zone,
    requested: Option<usize>,
    preserved_index: Option<usize>,
) -> Result<Option<usize>, ZoneError> {
    if zone == Zone::Play
        && world
            .get::<EntityKind>(entity)
            .is_some_and(|kind| is_board_entity(*kind))
        && let Some(board_position) = requested
    {
        validate_board_position(world, player, Some(board_position))?;
        let entries = world.resource::<ZoneIndex>().entities(player, Zone::Play);
        let flat_position = entries
            .iter()
            .enumerate()
            .filter(|(_, id)| {
                game_entity(world, **id)
                    .and_then(|candidate| world.get::<EntityKind>(candidate))
                    .is_some_and(|kind| is_board_entity(*kind))
            })
            .nth(board_position)
            .map_or_else(
                || {
                    entries
                        .iter()
                        .rposition(|id| {
                            game_entity(world, *id)
                                .and_then(|candidate| world.get::<EntityKind>(candidate))
                                .is_some_and(|kind| is_board_entity(*kind))
                        })
                        .map_or(entries.len(), |position| position + 1)
                },
                |(position, _)| position,
            );
        return Ok(Some(flat_position));
    }
    let position = requested.or(preserved_index);
    validate_zone_position(world, player, zone, position)?;
    Ok(position)
}

fn move_to_graveyard_after_failed_move(
    world: &mut World,
    id: GameEntityId,
    source: Zone,
    controller: PlayerId,
    record_death: bool,
) -> Result<ZoneMoveOutcome, ZoneError> {
    let remembered_position = semantic_zone_position(world, id, controller, source).ok_or(
        ZoneError::MissingIndexEntry {
            entity: id,
            zone: source,
        },
    )?;
    remove_from_index(world, id, controller, source)?;
    insert_into_zone(world, id, controller, Zone::Graveyard, None)?;
    apply_movement_state_policy(
        world,
        ZoneMoveRequest {
            entity: id,
            destination_controller: controller,
            destination: Zone::Graveyard,
            position: None,
            kind: if record_death {
                ZoneMovementKind::Death
            } else {
                ZoneMovementKind::Discard
            },
        },
        source,
    );
    if record_death {
        crate::death::record_full_zone_death(world, id, remembered_position);
    }
    Ok(ZoneMoveOutcome::FullZoneRemoval {
        from: source,
        from_controller: controller,
        from_position: remembered_position,
    })
}

fn apply_movement_state_policy(world: &mut World, request: ZoneMoveRequest, source: Zone) {
    if request.kind == ZoneMovementKind::DetachEnchantment
        && let Some(entity) = game_entity(world, request.entity)
    {
        world.entity_mut(entity).remove::<TemporaryDuration>();
    }

    if request.destination == Zone::Play && source != Zone::Play {
        let order = allocate_play_order(world);
        if let Some(entity) = game_entity(world, request.entity) {
            world.entity_mut(entity).insert(order);
        }
    }

    let leaving_play = source == Zone::Play && request.destination != Zone::Play;
    if leaving_play {
        clear_post_death_auras(world, request.entity);
    }

    if world
        .resource::<Ruleset>()
        .resets_runtime_state(request.kind, source, request.destination)
    {
        reset_runtime_state(world, request.entity);
    } else if source == Zone::Hand
        && request.destination == Zone::Play
        && let Some(entity) = game_entity(world, request.entity)
    {
        world.entity_mut(entity).remove::<PendingDestroy>();
    }

    if source == request.destination
        && request.destination == Zone::Play
        && let Some(entity) = game_entity(world, request.entity)
        && let Some(mut attack) = world.get_mut::<AttackState>(entity)
    {
        attack.exhausted = true;
    }
}

fn clear_post_death_auras(world: &mut World, id: GameEntityId) {
    if let Some(entity) = game_entity(world, id) {
        world
            .entity_mut(entity)
            .remove::<(AttackAuraCache, OtherAuraCache)>();
        recalculate_stats(world, id);
    }
}

fn clear_all_received_auras(world: &mut World, id: GameEntityId) {
    if let Some(entity) = game_entity(world, id) {
        world
            .entity_mut(entity)
            .remove::<(HealthAuraCache, AttackAuraCache, OtherAuraCache)>();
    }
}

fn reset_runtime_state(world: &mut World, id: GameEntityId) {
    let Some(entity) = game_entity(world, id) else {
        return;
    };
    let base = world.get::<BaseStats>(entity).copied();
    let base_keywords = world
        .get::<BaseKeywords>(entity)
        .cloned()
        .unwrap_or_default();
    let has_attack_state = world.get::<AttackState>(entity).is_some();
    let has_armor = world.get::<Armor>(entity).is_some();
    let attachments = if world.get::<KeepEnchantments>(entity).is_some() {
        Vec::new()
    } else {
        world
            .iter_entities()
            .filter(|candidate| {
                candidate.get::<AttachedTo>().map(|attached| attached.0) == Some(entity)
            })
            .map(|candidate| {
                (
                    *candidate
                        .get::<GameEntityId>()
                        .expect("attached enchantment is a game entity"),
                    candidate.id(),
                )
            })
            .collect::<Vec<_>>()
    };
    for (attachment_id, attachment) in attachments {
        world.entity_mut(attachment).remove::<AttachedTo>();
        let _ = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: attachment_id,
                destination_controller: world
                    .get::<Controller>(attachment)
                    .map_or(PlayerId::One, |controller| controller.0),
                destination: Zone::RemovedFromGame,
                position: None,
                kind: ZoneMovementKind::DetachEnchantment,
            },
        );
    }

    let Some(entity) = game_entity(world, id) else {
        return;
    };
    world
        .entity_mut(entity)
        .remove::<(PendingDestroy, Silenced)>();
    world
        .entity_mut(entity)
        .insert((Damage::default(), Keywords(base_keywords.0)));
    clear_all_received_auras(world, id);
    if let Some(base) = base {
        world.entity_mut(entity).insert(CurrentStats {
            attack: base.attack,
            maximum_health: base.health,
        });
    }
    if has_attack_state {
        world.entity_mut(entity).insert(AttackState {
            attacks_this_turn: 0,
            exhausted: true,
        });
    }
    if has_armor {
        world.entity_mut(entity).insert(Armor::default());
    }
    recalculate_stats(world, id);
    recalculate_keywords(world, id);
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
        assert_that!(
            zone_limit(world.resource::<Ruleset>(), Zone::Deck),
            eq(Some(99))
        );
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
