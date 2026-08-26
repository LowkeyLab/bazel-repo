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
