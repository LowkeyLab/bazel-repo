use std::collections::BTreeSet;

use bevy::prelude::{Component, Resource, World};

use crate::{
    CanonicalTrace, Controller, CurrentStats, Damage, EntityKind, GameEntityId, GameState,
    PendingDestroy, PlayOrder, PlayerId, TraceEntry, Zone, ZonePosition, entity::game_entity,
    zone::move_entity,
};

#[derive(Component, Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[component(immutable)]
pub struct DeathRecord {
    pub entity: GameEntityId,
    pub controller: PlayerId,
    pub kind: EntityKind,
    pub play_order: u64,
    pub remembered_zone_position: usize,
    pub simultaneous_ordinal: u32,
    pub turn_of_death: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct DeathEventCache {
    pub records: Vec<DeathRecord>,
}

#[derive(Default, Resource)]
pub(crate) struct PendingDeaths(pub Vec<DeathRecord>);

#[derive(Default, Resource)]
pub(crate) struct DefeatedHeroes(pub BTreeSet<PlayerId>);

pub(crate) fn create_deaths(world: &mut World) {
    let mortally_wounded = world
        .iter_entities()
        .filter_map(|entity| {
            let id = *entity.get::<GameEntityId>()?;
            let kind = *entity.get::<EntityKind>()?;
            (*entity.get::<Zone>()? == Zone::Play && is_mortally_wounded(world, id)).then_some((
                entity.get::<PlayOrder>().map_or(0, |order| order.0),
                id,
                entity.id(),
                entity.get::<Controller>()?.0,
                kind,
                entity
                    .get::<ZonePosition>()
                    .map_or(0, |position| position.0),
            ))
        })
        .collect::<Vec<_>>();

    // A Hero found mortally wounded during Death Creation is irreversibly defeated even if a
    // later Deathrattle heals it before the next outcome check. Heroes remain in play and do not
    // create ordinary Death Events.
    let defeated_heroes = mortally_wounded
        .iter()
        .filter_map(|(_, id, _, controller, kind, _)| {
            (*kind == EntityKind::Hero).then_some((*id, *controller))
        })
        .collect::<Vec<_>>();
    for (entity, controller) in defeated_heroes {
        if world.resource_mut::<DefeatedHeroes>().0.insert(controller) {
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::HeroDefeated { entity, controller });
        }
    }

    let mut deaths = mortally_wounded
        .into_iter()
        .filter(|(_, _, _, _, kind, _)| matches!(kind, EntityKind::Minion | EntityKind::Location))
        .collect::<Vec<_>>();
    deaths.sort_by_key(|(play_order, id, ..)| (*play_order, *id));
    let turn = world.resource::<GameState>().turn_number;

    // Membership and remembered state are collected before any move. No trigger runs between
    // removals, so every death in this boundary observes the same simultaneous collection step.
    let mut records = Vec::with_capacity(deaths.len());
    for (ordinal, (play_order, id, entity, controller, kind, zone_position)) in
        deaths.into_iter().enumerate()
    {
        if move_entity(world, id, Zone::Graveyard, None).is_ok() {
            world.entity_mut(entity).remove::<PendingDestroy>();
            let record = DeathRecord {
                entity: id,
                controller,
                kind,
                play_order,
                remembered_zone_position: zone_position,
                simultaneous_ordinal: u32::try_from(ordinal).expect("death batch exceeds u32"),
                turn_of_death: turn,
            };
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::EntityDied { entity: id });
            records.push(record);
        }
    }
    world
        .resource_mut::<DeathEventCache>()
        .records
        .extend(records.iter().cloned());
    world.resource_mut::<PendingDeaths>().0.extend(records);
}

pub(crate) fn take_pending_deaths(world: &mut World) -> Vec<DeathRecord> {
    std::mem::take(&mut world.resource_mut::<PendingDeaths>().0)
}

pub(crate) fn is_mortally_wounded(world: &World, id: GameEntityId) -> bool {
    game_entity(world, id).is_some_and(|entity| {
        world
            .get::<CurrentStats>(entity)
            .zip(world.get::<Damage>(entity))
            .is_some_and(|(stats, damage)| damage.0 >= stats.maximum_health)
            || world.get::<PendingDestroy>(entity).is_some()
    })
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{GameObject, entity::GameEntityIndex};

    #[googletest::test]
    fn mortality_includes_lethal_damage_and_pending_destroy() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.spawn((
            GameObject,
            GameEntityId(1),
            CurrentStats {
                attack: 0,
                maximum_health: 3,
            },
            Damage(3),
        ));
        world.spawn((GameObject, GameEntityId(2), PendingDestroy));
        world.spawn((
            GameObject,
            GameEntityId(3),
            CurrentStats {
                attack: 0,
                maximum_health: 3,
            },
            Damage(2),
        ));

        assert_that!(is_mortally_wounded(&world, GameEntityId(1)), is_true());
        assert_that!(is_mortally_wounded(&world, GameEntityId(2)), is_true());
        assert_that!(is_mortally_wounded(&world, GameEntityId(3)), is_false());
        assert_that!(is_mortally_wounded(&world, GameEntityId(99)), is_false());
    }
}
