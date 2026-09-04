use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use crate::{
    AttachedTo, AttackAuraCache, AuraApplication, AuraCategory, AuraModifier, AuraTarget,
    BaseStats, CanonicalTrace, ContinuousModifier, Controller, EntityKind, GameEntityId,
    HealthAuraCache, Keyword, Keywords, OtherAuraCache, OtherAuraModifier, PlayOrder,
    PlayerAudience, PlayerId, RuntimeAuras, RuntimeContinuousEffects, Silenced, TraceEntry, Zone,
    enchantment::recalculate_stats, entity::game_entity,
};

pub(crate) fn refresh_health_attack_auras(world: &mut World) {
    let discovered = discover_aura_applications(world, None);
    replace_category(world, AuraCategory::Health, &discovered);
    replace_category(world, AuraCategory::Attack, &discovered);
}

pub(crate) fn refresh_post_death_auras(world: &mut World) {
    let discovered = discover_aura_applications(world, None);
    // Health is deliberately not refreshed after Death Creation. Attack and Other effects from
    // removed providers cease before Death Event work begins.
    replace_category(world, AuraCategory::Attack, &discovered);
    replace_category(world, AuraCategory::Other, &discovered);
}

pub(crate) fn refresh_all_auras(world: &mut World) {
    let discovered = discover_aura_applications(world, None);
    replace_category(world, AuraCategory::Health, &discovered);
    replace_category(world, AuraCategory::Attack, &discovered);
    replace_category(world, AuraCategory::Other, &discovered);
}

pub(crate) fn refresh_played_provider(world: &mut World, provider: GameEntityId) {
    let discovered = discover_aura_applications(world, Some(provider));
    for category in [
        AuraCategory::Health,
        AuraCategory::Attack,
        AuraCategory::Other,
    ] {
        merge_provider_category(world, category, provider, &discovered);
    }
}

#[must_use]
pub(crate) fn current_spell_damage(world: &World, player: PlayerId) -> i32 {
    let mut providers = world
        .iter_entities()
        .filter_map(|entity| {
            (!entity.contains::<Silenced>() && continuous_source_is_active(world, entity.id()))
                .then_some((
                    entity.get::<PlayOrder>().map_or(0, |order| order.0),
                    *entity.get::<GameEntityId>()?,
                    entity.get::<Controller>()?.0,
                    entity.get::<RuntimeContinuousEffects>()?.0.clone(),
                ))
        })
        .collect::<Vec<_>>();
    providers.sort_by_key(|(play_order, id, ..)| (*play_order, *id));
    providers
        .into_iter()
        .fold(0_i32, |total, (_, _, controller, definitions)| {
            definitions.into_iter().fold(total, |total, definition| {
                let recipient_matches = match definition.recipients {
                    PlayerAudience::Controller => player == controller,
                    PlayerAudience::Opponent => player == controller.opponent(),
                    PlayerAudience::Both => true,
                };
                if recipient_matches {
                    match definition.modifier {
                        ContinuousModifier::SpellDamage(amount) => total.saturating_add(amount),
                    }
                } else {
                    total
                }
            })
        })
}

fn continuous_source_is_active(world: &World, source: Entity) -> bool {
    if let Some(attached) = world.get::<AttachedTo>(source) {
        return world.get::<Zone>(attached.0) == Some(&Zone::Play);
    }
    world.get::<Zone>(source) == Some(&Zone::Play)
}

#[must_use]
pub(crate) fn has_keyword(world: &World, target: Entity, keyword: Keyword) -> bool {
    world
        .get::<Keywords>(target)
        .is_some_and(|keywords| keywords.0.contains(&keyword))
        || (keyword == Keyword::Immune
            && world.get::<OtherAuraCache>(target).is_some_and(|cache| {
                cache
                    .0
                    .iter()
                    .any(|application| application.modifier == AuraModifier::Immune)
            }))
}

#[must_use]
pub(crate) fn hero_power_damage_bonus(world: &World, player: PlayerId) -> i32 {
    world
        .iter_entities()
        .find(|entity| {
            entity.get::<EntityKind>() == Some(&EntityKind::Player)
                && entity.get::<Controller>().map(|controller| controller.0) == Some(player)
        })
        .and_then(|entity| entity.get::<OtherAuraCache>())
        .map_or(0, |cache| {
            cache.0.iter().fold(0_i32, |total, application| {
                if let AuraModifier::HeroPowerDamage(amount) = application.modifier {
                    total.saturating_add(amount)
                } else {
                    total
                }
            })
        })
}

fn discover_aura_applications(
    world: &World,
    only_provider: Option<GameEntityId>,
) -> BTreeMap<(AuraCategory, GameEntityId), Vec<AuraApplication>> {
    let mut providers = world
        .iter_entities()
        .filter_map(|entity| {
            let id = *entity.get::<GameEntityId>()?;
            (only_provider.is_none_or(|provider| provider == id)
                && entity.get::<Zone>() == Some(&Zone::Play)
                && !entity.contains::<Silenced>())
            .then_some((
                entity.get::<PlayOrder>().map_or(0, |order| order.0),
                id,
                entity.get::<Controller>()?.0,
                entity.get::<RuntimeAuras>()?.0.clone(),
            ))
        })
        .collect::<Vec<_>>();
    providers.sort_by_key(|(play_order, id, ..)| (*play_order, *id));

    let mut targets = world
        .iter_entities()
        .filter_map(|entity| {
            let kind = *entity.get::<EntityKind>()?;
            let eligible = kind == EntityKind::Player
                || (entity.get::<Zone>() == Some(&Zone::Play) && entity.contains::<BaseStats>());
            eligible.then_some((
                *entity.get::<GameEntityId>()?,
                entity.get::<Controller>()?.0,
                kind,
            ))
        })
        .collect::<Vec<_>>();
    targets.sort_by_key(|(id, ..)| *id);

    let mut applications = BTreeMap::new();
    for (_, provider, controller, definitions) in providers {
        for (definition_index, definition) in definitions.into_iter().enumerate() {
            let definition_index =
                u32::try_from(definition_index).expect("entity has more than u32::MAX auras");
            for (target, target_controller, target_kind) in &targets {
                if !aura_matches(
                    definition.targets,
                    provider,
                    controller,
                    *target,
                    *target_controller,
                    *target_kind,
                ) {
                    continue;
                }
                if definition.health != 0 {
                    push_application(
                        &mut applications,
                        AuraCategory::Health,
                        *target,
                        AuraApplication {
                            provider,
                            definition_index,
                            modifier: AuraModifier::MaximumHealth(definition.health),
                        },
                    );
                }
                if definition.attack != 0 {
                    push_application(
                        &mut applications,
                        AuraCategory::Attack,
                        *target,
                        AuraApplication {
                            provider,
                            definition_index,
                            modifier: AuraModifier::Attack(definition.attack),
                        },
                    );
                }
                for modifier in &definition.other {
                    let modifier = match modifier {
                        OtherAuraModifier::Immune => AuraModifier::Immune,
                        OtherAuraModifier::HeroPowerDamage(amount) => {
                            AuraModifier::HeroPowerDamage(*amount)
                        }
                    };
                    push_application(
                        &mut applications,
                        AuraCategory::Other,
                        *target,
                        AuraApplication {
                            provider,
                            definition_index,
                            modifier,
                        },
                    );
                }
            }
        }
    }
    applications
}

fn push_application(
    applications: &mut BTreeMap<(AuraCategory, GameEntityId), Vec<AuraApplication>>,
    category: AuraCategory,
    target: GameEntityId,
    application: AuraApplication,
) {
    applications
        .entry((category, target))
        .or_default()
        .push(application);
}

fn replace_category(
    world: &mut World,
    category: AuraCategory,
    discovered: &BTreeMap<(AuraCategory, GameEntityId), Vec<AuraApplication>>,
) {
    let mut targets = discovered
        .keys()
        .filter_map(|(application_category, target)| {
            (*application_category == category).then_some(*target)
        })
        .collect::<BTreeSet<_>>();
    for entity in world.iter_entities() {
        let has_cache = match category {
            AuraCategory::Health => entity.contains::<HealthAuraCache>(),
            AuraCategory::Attack => entity.contains::<AttackAuraCache>(),
            AuraCategory::Other => entity.contains::<OtherAuraCache>(),
        };
        if has_cache && let Some(id) = entity.get::<GameEntityId>() {
            targets.insert(*id);
        }
    }
    for target in targets {
        set_cache(
            world,
            category,
            target,
            discovered
                .get(&(category, target))
                .cloned()
                .unwrap_or_default(),
        );
    }
}

fn merge_provider_category(
    world: &mut World,
    category: AuraCategory,
    provider: GameEntityId,
    discovered: &BTreeMap<(AuraCategory, GameEntityId), Vec<AuraApplication>>,
) {
    let mut targets = discovered
        .keys()
        .filter_map(|(application_category, target)| {
            (*application_category == category).then_some(*target)
        })
        .collect::<BTreeSet<_>>();
    for entity in world.iter_entities() {
        let includes_provider = cache(world, entity.id(), category).is_some_and(|cache| {
            cache
                .iter()
                .any(|application| application.provider == provider)
        });
        if includes_provider && let Some(id) = entity.get::<GameEntityId>() {
            targets.insert(*id);
        }
    }
    for target in targets {
        let Some(entity) = game_entity(world, target) else {
            continue;
        };
        let mut applications = cache(world, entity, category).unwrap_or_default();
        applications.retain(|application| application.provider != provider);
        applications.extend(
            discovered
                .get(&(category, target))
                .into_iter()
                .flatten()
                .copied(),
        );
        set_cache(world, category, target, applications);
    }
}

fn cache(world: &World, entity: Entity, category: AuraCategory) -> Option<Vec<AuraApplication>> {
    match category {
        AuraCategory::Health => world
            .get::<HealthAuraCache>(entity)
            .map(|cache| cache.0.clone()),
        AuraCategory::Attack => world
            .get::<AttackAuraCache>(entity)
            .map(|cache| cache.0.clone()),
        AuraCategory::Other => world
            .get::<OtherAuraCache>(entity)
            .map(|cache| cache.0.clone()),
    }
}

fn set_cache(
    world: &mut World,
    category: AuraCategory,
    target: GameEntityId,
    applications: Vec<AuraApplication>,
) {
    let Some(entity) = game_entity(world, target) else {
        return;
    };
    let previous = cache(world, entity, category).unwrap_or_default();
    let had_cache = cache(world, entity, category).is_some();
    if previous == applications {
        return;
    }
    match category {
        AuraCategory::Health => world
            .entity_mut(entity)
            .insert(HealthAuraCache(applications.clone())),
        AuraCategory::Attack => world
            .entity_mut(entity)
            .insert(AttackAuraCache(applications.clone())),
        AuraCategory::Other => world
            .entity_mut(entity)
            .insert(OtherAuraCache(applications.clone())),
    };
    if matches!(category, AuraCategory::Health | AuraCategory::Attack) {
        recalculate_stats(world, target);
    }
    if had_cache || !applications.is_empty() {
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::AuraUpdated {
                target,
                category,
                applications,
            });
    }
}

fn aura_matches(
    selector: AuraTarget,
    provider: GameEntityId,
    controller: PlayerId,
    target: GameEntityId,
    target_controller: PlayerId,
    target_kind: EntityKind,
) -> bool {
    let friendly = controller == target_controller;
    let minion = target_kind == EntityKind::Minion;
    let character = matches!(target_kind, EntityKind::Hero | EntityKind::Minion);
    let player = target_kind == EntityKind::Player;
    let other = provider != target;
    match selector {
        AuraTarget::FriendlyMinions => friendly && minion,
        AuraTarget::OtherFriendlyMinions => friendly && minion && other,
        AuraTarget::EnemyMinions => !friendly && minion,
        AuraTarget::AllMinions => minion,
        AuraTarget::FriendlyCharacters => friendly && character,
        AuraTarget::OtherFriendlyCharacters => friendly && character && other,
        AuraTarget::EnemyCharacters => !friendly && character,
        AuraTarget::AllCharacters => character,
        AuraTarget::ControllerPlayer => friendly && player,
        AuraTarget::OpponentPlayer => !friendly && player,
        AuraTarget::BothPlayers => player,
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{ContinuousEffectDefinition, GameObject, entity::GameEntityIndex};

    #[googletest::test]
    fn continuous_effects_and_stale_aura_cache_targets_are_handled() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.init_resource::<CanonicalTrace>();
        world.spawn((
            GameObject,
            GameEntityId(1),
            Controller(PlayerId::One),
            Zone::Play,
            RuntimeContinuousEffects(vec![ContinuousEffectDefinition {
                recipients: PlayerAudience::Opponent,
                modifier: ContinuousModifier::SpellDamage(2),
            }]),
        ));
        world.spawn((
            GameObject,
            GameEntityId(2),
            AttackAuraCache(vec![AuraApplication {
                provider: GameEntityId(99),
                definition_index: 0,
                modifier: AuraModifier::Attack(1),
            }]),
        ));

        assert_that!(current_spell_damage(&world, PlayerId::One), eq(0));
        assert_that!(current_spell_damage(&world, PlayerId::Two), eq(2));

        merge_provider_category(
            &mut world,
            AuraCategory::Attack,
            GameEntityId(99),
            &BTreeMap::new(),
        );
        let target = game_entity(&world, GameEntityId(2)).unwrap();
        assert_that!(
            world.get::<AttackAuraCache>(target).unwrap().0.is_empty(),
            is_true()
        );

        let mut discovered = BTreeMap::new();
        discovered.insert((AuraCategory::Attack, GameEntityId(100)), Vec::new());
        merge_provider_category(
            &mut world,
            AuraCategory::Attack,
            GameEntityId(99),
            &discovered,
        );
        set_cache(
            &mut world,
            AuraCategory::Attack,
            GameEntityId(100),
            Vec::new(),
        );
    }

    #[googletest::test]
    fn aura_target_categories_distinguish_controller_kind_and_source() {
        let provider = GameEntityId(1);
        let friendly = GameEntityId(2);
        let enemy = GameEntityId(3);
        let matches = |selector, target, controller, kind| {
            aura_matches(selector, provider, PlayerId::One, target, controller, kind)
        };

        assert_that!(
            matches(
                AuraTarget::FriendlyMinions,
                provider,
                PlayerId::One,
                EntityKind::Minion
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::OtherFriendlyMinions,
                provider,
                PlayerId::One,
                EntityKind::Minion
            ),
            is_false()
        );
        assert_that!(
            matches(
                AuraTarget::OtherFriendlyMinions,
                friendly,
                PlayerId::One,
                EntityKind::Minion
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::EnemyMinions,
                enemy,
                PlayerId::Two,
                EntityKind::Minion
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::AllMinions,
                enemy,
                PlayerId::Two,
                EntityKind::Hero
            ),
            is_false()
        );
        assert_that!(
            matches(
                AuraTarget::FriendlyCharacters,
                friendly,
                PlayerId::One,
                EntityKind::Hero
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::OtherFriendlyCharacters,
                provider,
                PlayerId::One,
                EntityKind::Hero
            ),
            is_false()
        );
        assert_that!(
            matches(
                AuraTarget::OtherFriendlyCharacters,
                friendly,
                PlayerId::One,
                EntityKind::Hero
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::EnemyCharacters,
                enemy,
                PlayerId::Two,
                EntityKind::Hero
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::AllCharacters,
                friendly,
                PlayerId::One,
                EntityKind::Weapon
            ),
            is_false()
        );
        assert_that!(
            matches(
                AuraTarget::ControllerPlayer,
                friendly,
                PlayerId::One,
                EntityKind::Player
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::OpponentPlayer,
                enemy,
                PlayerId::Two,
                EntityKind::Player
            ),
            is_true()
        );
        assert_that!(
            matches(
                AuraTarget::BothPlayers,
                enemy,
                PlayerId::Two,
                EntityKind::Player
            ),
            is_true()
        );
    }
}
