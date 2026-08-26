use bevy::prelude::*;

use crate::{BaseStats, CurrentStats, GameEntityId, PlayOrder, entity::game_entity};

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatModifier {
    pub attack: i32,
    pub health: i32,
    pub silence_removable: bool,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[relationship(relationship_target = AttachedEnchantments)]
pub struct AttachedTo(#[relationship] pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = AttachedTo, linked_spawn)]
pub struct AttachedEnchantments(#[relationship] Vec<Entity>);

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct AuraCache(pub Vec<AuraApplication>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuraApplication {
    pub provider: GameEntityId,
    pub attack: i32,
    pub health: i32,
}

pub(crate) fn recalculate_stats(world: &mut World, target: GameEntityId) {
    let Some(entity) = game_entity(world, target) else {
        return;
    };
    let Some(base) = world.get::<BaseStats>(entity).copied() else {
        return;
    };
    let mut modifiers = world
        .get::<AttachedEnchantments>(entity)
        .map(|attachments| {
            attachments
                .0
                .iter()
                .filter_map(|enchantment| {
                    Some((
                        world
                            .get::<PlayOrder>(*enchantment)
                            .map_or(0, |order| order.0),
                        *world.get::<StatModifier>(*enchantment)?,
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    modifiers.sort_by_key(|(order, _)| *order);
    let aura = world.get::<AuraCache>(entity).cloned().unwrap_or_default();
    let (mut attack, mut health) = (base.attack, base.health);
    for (_, modifier) in modifiers {
        attack += modifier.attack;
        health += modifier.health;
    }
    for application in aura.0 {
        attack += application.attack;
        health += application.health;
    }
    world.entity_mut(entity).insert(CurrentStats {
        attack,
        maximum_health: health.max(0),
    });
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{GameObject, entity::GameEntityIndex};

    #[googletest::test]
    fn recalculation_combines_ordered_modifiers_and_auras() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let target = world
            .spawn((
                GameObject,
                GameEntityId(1),
                BaseStats {
                    attack: 2,
                    health: 2,
                },
                AuraCache(vec![AuraApplication {
                    provider: GameEntityId(9),
                    attack: 3,
                    health: -10,
                }]),
            ))
            .id();
        world.spawn((
            GameObject,
            GameEntityId(2),
            PlayOrder(2),
            StatModifier {
                attack: 4,
                health: 1,
                silence_removable: true,
            },
            AttachedTo(target),
        ));
        world.spawn((
            GameObject,
            GameEntityId(3),
            StatModifier {
                attack: -1,
                health: 2,
                silence_removable: false,
            },
            AttachedTo(target),
        ));
        world.spawn((GameObject, GameEntityId(4), AttachedTo(target)));

        recalculate_stats(&mut world, GameEntityId(1));

        assert_that!(
            world.get::<CurrentStats>(target),
            eq(Some(&CurrentStats {
                attack: 8,
                maximum_health: 0,
            }))
        );
        recalculate_stats(&mut world, GameEntityId(99));
        let without_base = world.spawn((GameObject, GameEntityId(5))).id();
        recalculate_stats(&mut world, GameEntityId(5));
        assert_that!(world.get::<CurrentStats>(without_base).is_none(), is_true());
    }
}
