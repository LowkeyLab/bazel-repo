use bevy::prelude::World;

use crate::{
    CanonicalTrace, Controller, CurrentStats, Damage, EntityKind, GameEntityId, PendingDestroy,
    PlayOrder, TraceEntry, Zone, entity::game_entity, zone::move_entity,
};

pub(crate) fn create_deaths(world: &mut World) {
    let mut deaths = world
        .iter_entities()
        .filter_map(|entity| {
            let id = *entity.get::<GameEntityId>()?;
            let zone = entity.get::<Zone>()?;
            let kind = entity.get::<EntityKind>()?;
            if *zone != Zone::Play || !matches!(kind, EntityKind::Minion | EntityKind::Location) {
                return None;
            }
            let mortal = entity
                .get::<CurrentStats>()
                .zip(entity.get::<Damage>())
                .is_some_and(|(stats, damage)| damage.0 >= stats.maximum_health);
            if !mortal && !entity.contains::<PendingDestroy>() {
                return None;
            }
            Some((
                entity.get::<Controller>()?.0.bucket(),
                entity.get::<PlayOrder>().map_or(0, |order| order.0),
                id,
                entity.id(),
            ))
        })
        .collect::<Vec<_>>();
    deaths.sort_by_key(|(player, play_order, id, _)| (*player, *play_order, *id));

    // Membership is collected and ordered before any move. No trigger runs between removals.
    for (_, _, id, entity) in deaths {
        if move_entity(world, id, Zone::Graveyard, None).is_ok() {
            world.entity_mut(entity).remove::<PendingDestroy>();
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::EntityDied { entity: id });
        }
    }
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
