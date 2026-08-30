use bevy::prelude::{Component, Entity, World};

use crate::{
    Controller, DominantPlayer, Effect, EntityKind, EventContext, EventId, EventKind, GameEntityId,
    PlayOrder, PlayerId, PlayerSelector, Selector, Zone, entity::game_entity, zone::ZoneIndex,
};

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ConditionTiming {
    PreCheck,
    QueueTime,
    ResolutionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TriggerCondition {
    Always,
    SourceInPlay,
    SourceInZone(Zone),
    EventValueAtLeast(i32),
    EventSourceIsSelf,
    EventTargetsSelf,
    ControllerIs(PlayerId),
    MinimumEntityCount { selector: Selector, count: usize },
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum SourceEligibilityPolicy {
    MustExist,
    MustRemainInEligibleZone,
    RememberedSource,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum WoundedTargetPolicy {
    ExcludeMortallyWounded,
    IncludeMortallyWounded,
    IncludePendingDestroy,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TimedCondition {
    pub timing: ConditionTiming,
    pub condition: TriggerCondition,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerDefinition {
    pub event: EventKind,
    pub eligible_zones: Vec<Zone>,
    pub conditions: Vec<TimedCondition>,
    pub source_eligibility: SourceEligibilityPolicy,
    pub priority: i16,
    pub wounded_target_policy: WoundedTargetPolicy,
    pub effect_program: Vec<Effect>,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeTriggers(pub Vec<TriggerDefinition>);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TriggersSuppressed;

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct TriggerOrderKey {
    pub player_bucket: u8,
    pub zone_bucket: u8,
    pub priority: i16,
    pub play_order: u64,
    pub source: GameEntityId,
    pub tie_breaker: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerSeed {
    pub source: GameEntityId,
    pub definition_index: u32,
    pub definition: TriggerDefinition,
    pub controller: PlayerId,
    pub zone: Zone,
    pub play_order: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerCandidate {
    pub source: GameEntityId,
    pub event: EventId,
    pub definition_index: u32,
    pub definition: TriggerDefinition,
    pub controller: PlayerId,
    pub order: TriggerOrderKey,
}

pub(crate) fn collect_trigger_seeds(world: &World, event: &EventContext) -> Vec<TriggerSeed> {
    let mut seeds = Vec::new();
    for entity in world.iter_entities() {
        let (Some(source), Some(triggers), Some(zone), Some(controller)) = (
            entity.get::<GameEntityId>(),
            entity
                .get::<RuntimeTriggers>()
                .filter(|_| !entity.contains::<TriggersSuppressed>()),
            entity.get::<Zone>(),
            entity.get::<Controller>(),
        ) else {
            continue;
        };
        let play_order = entity.get::<PlayOrder>().map_or(0, |order| order.0);
        for (definition_index, definition) in triggers.0.iter().enumerate() {
            if definition.event != event.kind || !definition.eligible_zones.contains(zone) {
                continue;
            }
            let definition_index =
                u32::try_from(definition_index).expect("entity has more than u32::MAX triggers");
            let seed = TriggerSeed {
                source: *source,
                definition_index,
                definition: definition.clone(),
                controller: controller.0,
                zone: *zone,
                play_order,
            };
            if trigger_seed_is_eligible(world, &seed, event, ConditionTiming::PreCheck) {
                seeds.push(seed);
            }
        }
    }
    seeds.sort_by_key(|seed| (seed.play_order, seed.source, seed.definition_index));
    seeds
}

pub(crate) fn collect_trigger_candidates(
    world: &World,
    event_id: EventId,
    event: &EventContext,
    seeds: &[TriggerSeed],
) -> Vec<TriggerCandidate> {
    let mut candidates = seeds
        .iter()
        .filter(|seed| trigger_seed_is_eligible(world, seed, event, ConditionTiming::QueueTime))
        .map(|seed| {
            let controller = game_entity(world, seed.source)
                .and_then(|source| world.get::<Controller>(source))
                .map_or(seed.controller, |controller| controller.0);
            let player_bucket = u8::from(controller != world.resource::<DominantPlayer>().0);
            let zone_bucket = if event.kind == EventKind::Death {
                0
            } else {
                zone_bucket(seed.zone)
            };
            TriggerCandidate {
                source: seed.source,
                event: event_id,
                definition_index: seed.definition_index,
                definition: seed.definition.clone(),
                controller,
                order: TriggerOrderKey {
                    player_bucket,
                    zone_bucket,
                    priority: seed.definition.priority,
                    play_order: seed.play_order,
                    source: seed.source,
                    tie_breaker: seed.definition_index,
                },
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| candidate.order);
    candidates
}

pub(crate) fn trigger_is_eligible(
    world: &World,
    candidate: &TriggerCandidate,
    event: &EventContext,
    timing: ConditionTiming,
) -> bool {
    trigger_seed_is_eligible(
        world,
        &TriggerSeed {
            source: candidate.source,
            definition_index: candidate.definition_index,
            definition: candidate.definition.clone(),
            controller: candidate.controller,
            zone: world
                .get::<Zone>(game_entity(world, candidate.source).unwrap_or(Entity::PLACEHOLDER))
                .copied()
                .unwrap_or(Zone::RemovedFromGame),
            play_order: candidate.order.play_order,
        },
        event,
        timing,
    )
}

fn trigger_seed_is_eligible(
    world: &World,
    seed: &TriggerSeed,
    event: &EventContext,
    timing: ConditionTiming,
) -> bool {
    let source_entity = game_entity(world, seed.source);
    match seed.definition.source_eligibility {
        SourceEligibilityPolicy::MustExist if source_entity.is_none() => return false,
        SourceEligibilityPolicy::MustRemainInEligibleZone
            if source_entity.is_none_or(|source| {
                world
                    .get::<Zone>(source)
                    .is_none_or(|zone| !seed.definition.eligible_zones.contains(zone))
            }) =>
        {
            return false;
        }
        _ => {}
    }
    seed.definition
        .conditions
        .iter()
        .filter(|condition| condition.timing == timing)
        .all(|condition| {
            evaluate_condition(
                world,
                seed.source,
                source_entity,
                seed.controller,
                Some(event),
                &condition.condition,
            )
        })
}

fn evaluate_condition(
    world: &World,
    source_id: GameEntityId,
    source: Option<Entity>,
    controller: PlayerId,
    event: Option<&EventContext>,
    condition: &TriggerCondition,
) -> bool {
    let current_controller = source
        .and_then(|source| world.get::<Controller>(source))
        .map_or(controller, |controller| controller.0);
    match condition {
        TriggerCondition::Always => true,
        TriggerCondition::SourceInPlay => {
            source.and_then(|source| world.get::<Zone>(source)) == Some(&Zone::Play)
        }
        TriggerCondition::SourceInZone(zone) => {
            source.and_then(|source| world.get::<Zone>(source)) == Some(zone)
        }
        TriggerCondition::EventValueAtLeast(value) => event
            .and_then(|event| event.actual_value.or(event.proposed_value))
            .is_some_and(|actual| actual >= *value),
        TriggerCondition::EventSourceIsSelf => {
            event.and_then(|event| event.source) == Some(source_id)
        }
        TriggerCondition::EventTargetsSelf => {
            event.is_some_and(|event| event.targets.contains(&source_id))
        }
        TriggerCondition::ControllerIs(player) => current_controller == *player,
        TriggerCondition::MinimumEntityCount { selector, count } => {
            selector_count(world, source_id, current_controller, event, selector) >= *count
        }
    }
}

fn selector_count(
    world: &World,
    source: GameEntityId,
    controller: PlayerId,
    event: Option<&EventContext>,
    selector: &Selector,
) -> usize {
    match selector {
        Selector::Source => usize::from(game_entity(world, source).is_some()),
        Selector::DeclaredTarget => usize::from(
            event
                .and_then(|event| event.targets.first())
                .is_some_and(|target| game_entity(world, *target).is_some()),
        ),
        Selector::Entity(entity) => usize::from(game_entity(world, *entity).is_some()),
        Selector::InZone { player, zone } => {
            let player = match player {
                PlayerSelector::Controller => controller,
                PlayerSelector::Opponent => controller.opponent(),
                PlayerSelector::Player(player) => *player,
            };
            world.resource::<ZoneIndex>().entities(player, *zone).len()
        }
        Selector::FriendlyMinions
        | Selector::EnemyMinions
        | Selector::AllMinions
        | Selector::FriendlyCharacters
        | Selector::EnemyCharacters
        | Selector::AllCharacters => world
            .resource::<ZoneIndex>()
            .0
            .iter()
            .filter(|((player, zone), _)| {
                *zone == Zone::Play
                    && match selector {
                        Selector::FriendlyMinions | Selector::FriendlyCharacters => {
                            *player == controller
                        }
                        Selector::EnemyMinions | Selector::EnemyCharacters => {
                            *player == controller.opponent()
                        }
                        _ => true,
                    }
            })
            .flat_map(|(_, entities)| entities)
            .filter(|id| {
                let kind =
                    game_entity(world, **id).and_then(|entity| world.get::<EntityKind>(entity));
                match selector {
                    Selector::FriendlyMinions | Selector::EnemyMinions | Selector::AllMinions => {
                        kind == Some(&EntityKind::Minion)
                    }
                    _ => matches!(kind, Some(EntityKind::Hero | EntityKind::Minion)),
                }
            })
            .count(),
        Selector::Random(inner) => {
            usize::from(selector_count(world, source, controller, event, inner) > 0)
        }
    }
}

const fn zone_bucket(zone: Zone) -> u8 {
    match zone {
        Zone::Play => 0,
        Zone::Secret => 1,
        Zone::Hand => 2,
        Zone::Deck => 3,
        Zone::Graveyard => 4,
        Zone::SetAside => 5,
        Zone::RemovedFromGame => 6,
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{GameObject, entity::GameEntityIndex, zone::ZoneIndex};

    fn definition() -> TriggerDefinition {
        TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: Vec::new(),
        }
    }

    #[googletest::test]
    fn candidate_snapshot_uses_complete_stable_order_keys() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.init_resource::<DominantPlayer>();
        for id in [9, 7] {
            world.spawn((
                GameObject,
                GameEntityId(id),
                Controller(PlayerId::One),
                Zone::Play,
                RuntimeTriggers(vec![definition()]),
            ));
        }
        let event = EventContext {
            kind: EventKind::Damage,
            source: None,
            targets: Vec::new(),
            controller: PlayerId::One,
            proposed_value: None,
            actual_value: Some(1),
            simultaneous_ordinal: 0,
        };

        let seeds = collect_trigger_seeds(&world, &event);
        let candidates = collect_trigger_candidates(&world, EventId(3), &event, &seeds);

        assert_that!(
            candidates
                .iter()
                .map(|candidate| candidate.source)
                .collect::<Vec<_>>(),
            eq(&vec![GameEntityId(7), GameEntityId(9)])
        );
    }

    #[googletest::test]
    fn resolution_time_controller_conditions_use_the_live_controller() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.init_resource::<DominantPlayer>();
        let mut trigger = definition();
        trigger.conditions.push(TimedCondition {
            timing: ConditionTiming::ResolutionTime,
            condition: TriggerCondition::ControllerIs(PlayerId::Two),
        });
        let source = world
            .spawn((
                GameObject,
                GameEntityId(1),
                Controller(PlayerId::One),
                Zone::Play,
                RuntimeTriggers(vec![trigger]),
            ))
            .id();
        let event = EventContext {
            kind: EventKind::Damage,
            source: None,
            targets: Vec::new(),
            controller: PlayerId::One,
            proposed_value: None,
            actual_value: Some(1),
            simultaneous_ordinal: 0,
        };
        let seeds = collect_trigger_seeds(&world, &event);
        let candidates = collect_trigger_candidates(&world, EventId(1), &event, &seeds);
        assert_that!(candidates[0].controller, eq(PlayerId::One));

        world.entity_mut(source).insert(Controller(PlayerId::Two));

        assert_that!(
            trigger_is_eligible(
                &world,
                &candidates[0],
                &event,
                ConditionTiming::ResolutionTime
            ),
            is_true()
        );
    }

    #[googletest::test]
    fn conditions_selectors_source_policies_and_zone_buckets_cover_the_domain() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.init_resource::<ZoneIndex>();
        let source = world
            .spawn((
                GameObject,
                GameEntityId(1),
                Controller(PlayerId::One),
                EntityKind::Minion,
                Zone::Play,
            ))
            .id();
        world.spawn((
            GameObject,
            GameEntityId(2),
            Controller(PlayerId::One),
            EntityKind::Hero,
            Zone::Play,
        ));
        world.spawn((
            GameObject,
            GameEntityId(3),
            Controller(PlayerId::Two),
            EntityKind::Minion,
            Zone::Play,
        ));
        world.spawn((
            GameObject,
            GameEntityId(4),
            Controller(PlayerId::Two),
            EntityKind::Hero,
            Zone::Play,
        ));
        world.resource_mut::<ZoneIndex>().0.insert(
            (PlayerId::One, Zone::Play),
            vec![GameEntityId(1), GameEntityId(2)],
        );
        world.resource_mut::<ZoneIndex>().0.insert(
            (PlayerId::Two, Zone::Play),
            vec![GameEntityId(3), GameEntityId(4)],
        );
        let event = EventContext {
            kind: EventKind::Damage,
            source: Some(GameEntityId(1)),
            targets: vec![GameEntityId(1)],
            controller: PlayerId::One,
            proposed_value: Some(1),
            actual_value: None,
            simultaneous_ordinal: 0,
        };

        let counts = [
            (Selector::Source, 1),
            (Selector::DeclaredTarget, 1),
            (Selector::Entity(GameEntityId(3)), 1),
            (Selector::Entity(GameEntityId(99)), 0),
            (
                Selector::InZone {
                    player: PlayerSelector::Controller,
                    zone: Zone::Play,
                },
                2,
            ),
            (
                Selector::InZone {
                    player: PlayerSelector::Opponent,
                    zone: Zone::Play,
                },
                2,
            ),
            (
                Selector::InZone {
                    player: PlayerSelector::Player(PlayerId::One),
                    zone: Zone::Play,
                },
                2,
            ),
            (Selector::FriendlyMinions, 1),
            (Selector::EnemyMinions, 1),
            (Selector::AllMinions, 2),
            (Selector::FriendlyCharacters, 2),
            (Selector::EnemyCharacters, 2),
            (Selector::AllCharacters, 4),
            (Selector::Random(Box::new(Selector::AllMinions)), 1),
            (
                Selector::Random(Box::new(Selector::Entity(GameEntityId(99)))),
                0,
            ),
        ];
        for (selector, expected) in counts {
            assert_that!(
                selector_count(
                    &world,
                    GameEntityId(1),
                    PlayerId::One,
                    Some(&event),
                    &selector
                ),
                eq(expected)
            );
        }

        for condition in [
            TriggerCondition::Always,
            TriggerCondition::SourceInPlay,
            TriggerCondition::SourceInZone(Zone::Play),
            TriggerCondition::EventValueAtLeast(1),
            TriggerCondition::EventSourceIsSelf,
            TriggerCondition::EventTargetsSelf,
            TriggerCondition::ControllerIs(PlayerId::One),
            TriggerCondition::MinimumEntityCount {
                selector: Selector::FriendlyCharacters,
                count: 2,
            },
        ] {
            assert_that!(
                evaluate_condition(
                    &world,
                    GameEntityId(1),
                    Some(source),
                    PlayerId::One,
                    Some(&event),
                    &condition
                ),
                is_true()
            );
        }
        assert_that!(
            evaluate_condition(
                &world,
                GameEntityId(1),
                Some(source),
                PlayerId::One,
                Some(&event),
                &TriggerCondition::SourceInZone(Zone::Hand)
            ),
            is_false()
        );

        let mut remembered = definition();
        remembered.source_eligibility = SourceEligibilityPolicy::RememberedSource;
        let missing = TriggerSeed {
            source: GameEntityId(99),
            definition_index: 0,
            definition: remembered,
            controller: PlayerId::One,
            zone: Zone::Play,
            play_order: 0,
        };
        assert_that!(
            trigger_seed_is_eligible(&world, &missing, &event, ConditionTiming::QueueTime),
            is_true()
        );
        let mut must_exist = missing.clone();
        must_exist.definition.source_eligibility = SourceEligibilityPolicy::MustExist;
        assert_that!(
            trigger_seed_is_eligible(&world, &must_exist, &event, ConditionTiming::QueueTime),
            is_false()
        );
        let mut must_remain = missing;
        must_remain.source = GameEntityId(1);
        must_remain.definition.source_eligibility =
            SourceEligibilityPolicy::MustRemainInEligibleZone;
        world.entity_mut(source).insert(Zone::Graveyard);
        assert_that!(
            trigger_seed_is_eligible(&world, &must_remain, &event, ConditionTiming::QueueTime),
            is_false()
        );

        assert_that!(
            [
                Zone::Play,
                Zone::Secret,
                Zone::Hand,
                Zone::Deck,
                Zone::Graveyard,
                Zone::SetAside,
                Zone::RemovedFromGame,
            ]
            .map(zone_bucket),
            eq([0, 1, 2, 3, 4, 5, 6])
        );
    }
}
