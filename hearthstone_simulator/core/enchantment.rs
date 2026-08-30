use bevy::prelude::*;

use crate::{
    AttackAuraCache, AuraModifier, BaseKeywords, BaseStats, CurrentStats, Damage, GameEntityId,
    HealthAuraCache, Keyword, Keywords, PlayOrder, PlayerId, Silenced, entity::game_entity,
};

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TemporaryDuration {
    EndOfTurn(PlayerId),
    EndOfTurnSeries(PlayerId),
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct KeywordModifier {
    pub keyword: Keyword,
    pub granted: bool,
    pub silence_removable: bool,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
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

pub(crate) fn recalculate_keywords(world: &mut World, target: GameEntityId) {
    let Some(entity) = game_entity(world, target) else {
        return;
    };
    let silenced = world.get::<Silenced>(entity).is_some();
    let mut keywords = if silenced {
        std::collections::BTreeSet::new()
    } else {
        world
            .get::<BaseKeywords>(entity)
            .map_or_else(std::collections::BTreeSet::new, |keywords| {
                keywords.0.clone()
            })
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
                        *world.get::<KeywordModifier>(*enchantment)?,
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    modifiers.sort_by_key(|(order, _)| *order);
    for (_, modifier) in modifiers {
        if silenced && modifier.silence_removable {
            continue;
        }
        if modifier.granted {
            keywords.insert(modifier.keyword);
        } else {
            keywords.remove(&modifier.keyword);
        }
    }
    world.entity_mut(entity).insert(Keywords(keywords));
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
    let attack_auras = world
        .get::<AttackAuraCache>(entity)
        .cloned()
        .unwrap_or_default();
    let health_auras = world
        .get::<HealthAuraCache>(entity)
        .cloned()
        .unwrap_or_default();
    let (mut attack, mut health) = (base.attack, base.health);
    for (_, modifier) in modifiers {
        attack += modifier.attack;
        health += modifier.health;
    }
    for application in attack_auras.0 {
        if let AuraModifier::Attack(amount) = application.modifier {
            attack += amount;
        }
    }
    for application in health_auras.0 {
        if let AuraModifier::MaximumHealth(amount) = application.modifier {
            health += amount;
        }
    }
    let maximum_health = health.max(0);
    let previous_maximum = world
        .get::<CurrentStats>(entity)
        .map(|stats| stats.maximum_health);
    let previous_damage = world.get::<Damage>(entity).copied();
    if let (Some(previous_maximum), Some(previous_damage)) = (previous_maximum, previous_damage)
        && maximum_health < previous_maximum
    {
        let previous_health = previous_maximum.saturating_sub(previous_damage.0);
        world.entity_mut(entity).insert(Damage(
            maximum_health.saturating_sub(previous_health).max(0),
        ));
    }
    world.entity_mut(entity).insert(CurrentStats {
        attack,
        maximum_health,
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
                AttackAuraCache(vec![crate::AuraApplication {
                    provider: GameEntityId(9),
                    definition_index: 0,
                    modifier: AuraModifier::Attack(3),
                }]),
                HealthAuraCache(vec![crate::AuraApplication {
                    provider: GameEntityId(9),
                    definition_index: 0,
                    modifier: AuraModifier::MaximumHealth(-10),
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

    #[googletest::test]
    fn maximum_health_changes_follow_h1_and_h2() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let target = world
            .spawn((
                GameObject,
                GameEntityId(1),
                BaseStats {
                    attack: 1,
                    health: 2,
                },
                CurrentStats {
                    attack: 1,
                    maximum_health: 2,
                },
                Damage(1),
                HealthAuraCache(vec![crate::AuraApplication {
                    provider: GameEntityId(2),
                    definition_index: 0,
                    modifier: AuraModifier::MaximumHealth(2),
                }]),
            ))
            .id();

        recalculate_stats(&mut world, GameEntityId(1));
        assert_that!(
            world.get::<CurrentStats>(target),
            eq(Some(&CurrentStats {
                attack: 1,
                maximum_health: 4,
            }))
        );
        assert_that!(world.get::<Damage>(target), eq(Some(&Damage(1))));

        world.entity_mut(target).insert(HealthAuraCache::default());
        recalculate_stats(&mut world, GameEntityId(1));
        assert_that!(
            world.get::<CurrentStats>(target),
            eq(Some(&CurrentStats {
                attack: 1,
                maximum_health: 2,
            }))
        );
        assert_that!(world.get::<Damage>(target), eq(Some(&Damage(0))));

        world.entity_mut(target).insert((
            CurrentStats {
                attack: 1,
                maximum_health: 4,
            },
            Damage(3),
        ));
        recalculate_stats(&mut world, GameEntityId(1));
        assert_that!(world.get::<Damage>(target), eq(Some(&Damage(1))));
    }
}
