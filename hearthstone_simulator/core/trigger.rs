use std::collections::BTreeSet;

use bevy::prelude::{Component, Entity, Resource, World};

use crate::{
    Controller, Effect, EntityKind, EventContext, EventKind, GameEntityId, PlayOrder, PlayerId,
    QueuedTrigger, ResolutionId, ResolutionIdentity, TriggerOrderKey, Zone,
    entity::game_entity,
    queue::{QueueMutationError, add_trigger_entry},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConditionTiming {
    PreCheck,
    QueueTime,
    ResolutionTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TriggerCondition {
    Always,
    SourceInPlay,
    SourceInZone(Zone),
    EventValueAtLeast(i32),
    EventSourceIsSelf,
    EventTargetsSelf,
    ControllerIs(PlayerId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceEligibilityPolicy {
    MustExist,
    MustRemainInEligibleZone,
    RememberedSource,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WoundedTargetPolicy {
    ExcludeMortallyWounded,
    IncludeMortallyWounded,
    IncludePendingDestroy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimedCondition {
    pub timing: ConditionTiming,
    pub condition: TriggerCondition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TriggerDefinition {
    pub event: EventKind,
    pub eligible_zones: Vec<Zone>,
    pub conditions: Vec<TimedCondition>,
    pub source_eligibility: SourceEligibilityPolicy,
    pub priority: i16,
    pub allow_repeated_event: bool,
    pub allow_direct_self_nesting: bool,
    pub wounded_target_policy: WoundedTargetPolicy,
    pub effect_program: Vec<Effect>,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeTriggers(pub Vec<TriggerDefinition>);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TriggersSuppressed;

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggerExecution {
    pub source: GameEntityId,
    pub controller: PlayerId,
    pub source_kind: EntityKind,
}

#[derive(Clone, Debug, Default, Resource)]
pub(crate) struct TriggerGuards {
    executed: BTreeSet<(GameEntityId, ResolutionId, u32)>,
    active: BTreeSet<(GameEntityId, u32)>,
}

pub(crate) fn begin_trigger_execution(
    world: &mut World,
    queued: &QueuedTrigger,
    definition: &TriggerDefinition,
) -> bool {
    let key = (queued.source, queued.definition_index);
    let event_key = (queued.source, queued.event, queued.definition_index);
    let mut guards = world.resource_mut::<TriggerGuards>();
    if (!definition.allow_repeated_event && guards.executed.contains(&event_key))
        || (!definition.allow_direct_self_nesting && guards.active.contains(&key))
    {
        return false;
    }
    guards.executed.insert(event_key);
    guards.active.insert(key);
    true
}

pub(crate) fn finish_trigger_execution(world: &mut World, queued: &QueuedTrigger) {
    world
        .resource_mut::<TriggerGuards>()
        .active
        .remove(&(queued.source, queued.definition_index));
}

pub(crate) fn reset_trigger_guards(world: &mut World) {
    *world.resource_mut::<TriggerGuards>() = TriggerGuards::default();
}

pub(crate) fn collect_trigger_candidates(
    world: &mut World,
    queue: Entity,
    event_entity: Entity,
) -> Result<Vec<Entity>, QueueMutationError> {
    let Some(event) = world.get::<EventContext>(event_entity).cloned() else {
        return Ok(Vec::new());
    };
    let Some(event_id) = world
        .get::<ResolutionIdentity>(event_entity)
        .map(|identity| identity.id)
    else {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
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
            // Death Event triggers mingle globally by named priority and play order. In
            // particular, a Deathrattle in Graveyard must not be delayed behind a newer observer
            // in Play merely because normal event queues group sources by controller and zone.
            let (player_bucket, zone_bucket) = if event.kind == EventKind::Death {
                (0, 0)
            } else {
                (controller.0.bucket(), zone_bucket(*zone))
            };
            let queued = QueuedTrigger {
                source: *source,
                event: event_id,
                event_entity,
                definition_index,
                order: TriggerOrderKey {
                    player_bucket,
                    zone_bucket,
                    priority: definition.priority,
                    play_order,
                    source: *source,
                    tie_breaker: definition_index,
                },
            };
            if trigger_is_eligible(world, &queued, ConditionTiming::PreCheck)
                && trigger_is_eligible(world, &queued, ConditionTiming::QueueTime)
            {
                candidates.push(queued);
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.order);
    candidates
        .into_iter()
        .map(|candidate| add_trigger_entry(world, queue, candidate))
        .collect()
}

pub(crate) fn trigger_is_eligible(
    world: &World,
    queued: &QueuedTrigger,
    timing: ConditionTiming,
) -> bool {
    let Some(source_entity) = game_entity(world, queued.source) else {
        return false;
    };
    let Some(definition) = world
        .get::<RuntimeTriggers>(source_entity)
        .and_then(|triggers| triggers.0.get(queued.definition_index as usize))
    else {
        return false;
    };
    if definition.source_eligibility == SourceEligibilityPolicy::MustRemainInEligibleZone
        && world
            .get::<Zone>(source_entity)
            .is_none_or(|zone| !definition.eligible_zones.contains(zone))
    {
        return false;
    }
    let event = world.get::<EventContext>(queued.event_entity);
    definition
        .conditions
        .iter()
        .filter(|condition| condition.timing == timing)
        .all(|condition| evaluate_condition(world, source_entity, event, &condition.condition))
}

fn evaluate_condition(
    world: &World,
    source: Entity,
    event: Option<&EventContext>,
    condition: &TriggerCondition,
) -> bool {
    match condition {
        TriggerCondition::Always => true,
        TriggerCondition::SourceInPlay => world.get::<Zone>(source) == Some(&Zone::Play),
        TriggerCondition::SourceInZone(zone) => world.get::<Zone>(source) == Some(zone),
        TriggerCondition::EventValueAtLeast(value) => event
            .and_then(|event| event.actual_value.or(event.proposed_value))
            .is_some_and(|actual| actual >= *value),
        TriggerCondition::EventSourceIsSelf => world
            .get::<GameEntityId>(source)
            .zip(event.and_then(|event| event.source.as_ref()))
            .is_some_and(|(source, event_source)| source == event_source),
        TriggerCondition::EventTargetsSelf => world
            .get::<GameEntityId>(source)
            .zip(event)
            .is_some_and(|(source, event)| event.targets.contains(source)),
        TriggerCondition::ControllerIs(player) => world
            .get::<Controller>(source)
            .is_some_and(|controller| controller.0 == *player),
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
    use bevy::prelude::*;
    use googletest::prelude::*;

    use super::*;
    use crate::{
        GameObject, QueueKind, QueueState, ResolutionId, ResolutionKind, ResolutionQueue,
        entity::GameEntityIndex,
    };

    fn definition(conditions: Vec<TimedCondition>) -> TriggerDefinition {
        TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions,
            source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: Vec::new(),
        }
    }

    #[googletest::test]
    fn collection_applies_precheck_and_queue_time_conditions() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.spawn((
            GameObject,
            GameEntityId(7),
            EntityKind::Minion,
            Controller(PlayerId::One),
            Zone::Play,
            PlayOrder(12),
            RuntimeTriggers(vec![TriggerDefinition {
                event: EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: vec![
                    TimedCondition {
                        timing: ConditionTiming::PreCheck,
                        condition: TriggerCondition::Always,
                    },
                    TimedCondition {
                        timing: ConditionTiming::QueueTime,
                        condition: TriggerCondition::EventValueAtLeast(2),
                    },
                ],
                source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: -1,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: Vec::new(),
            }]),
        ));
        let event = world
            .spawn((
                ResolutionIdentity {
                    id: ResolutionId(3),
                    kind: ResolutionKind::Event,
                },
                EventContext {
                    kind: EventKind::Damage,
                    source: None,
                    targets: Vec::new(),
                    controller: PlayerId::Two,
                    proposed_value: Some(2),
                    actual_value: Some(2),
                    simultaneous_ordinal: 0,
                },
            ))
            .id();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();

        let entries = collect_trigger_candidates(&mut world, queue, event)
            .expect("candidate collection should succeed");

        assert_that!(entries.len(), eq(1));
        assert_that!(
            world.get::<QueuedTrigger>(entries[0]).unwrap().source,
            eq(GameEntityId(7))
        );
    }

    #[googletest::test]
    fn collection_uses_stable_source_ids_to_break_equal_order_keys() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        for source in [GameEntityId(9), GameEntityId(7)] {
            world.spawn((
                GameObject,
                source,
                EntityKind::Minion,
                Controller(PlayerId::One),
                Zone::Hand,
                RuntimeTriggers(vec![TriggerDefinition {
                    event: EventKind::Damage,
                    eligible_zones: vec![Zone::Hand],
                    ..definition(Vec::new())
                }]),
            ));
        }
        let event = world
            .spawn((
                ResolutionIdentity {
                    id: ResolutionId(3),
                    kind: ResolutionKind::Event,
                },
                EventContext {
                    kind: EventKind::Damage,
                    source: None,
                    targets: Vec::new(),
                    controller: PlayerId::One,
                    proposed_value: None,
                    actual_value: None,
                    simultaneous_ordinal: 0,
                },
            ))
            .id();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();

        let entries = collect_trigger_candidates(&mut world, queue, event).unwrap();
        let sources = entries
            .iter()
            .map(|entry| world.get::<QueuedTrigger>(*entry).unwrap().source)
            .collect::<Vec<_>>();

        assert_that!(sources, eq(&[GameEntityId(7), GameEntityId(9)]));
    }

    #[googletest::test]
    fn collection_skips_missing_events_and_nonmatching_definitions() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();
        let not_an_event = world.spawn_empty().id();
        assert_that!(
            collect_trigger_candidates(&mut world, queue, not_an_event)
                .unwrap()
                .is_empty(),
            is_true()
        );

        world.spawn((
            GameObject,
            GameEntityId(1),
            EntityKind::Minion,
            Controller(PlayerId::One),
            Zone::Play,
            RuntimeTriggers(vec![TriggerDefinition {
                event: EventKind::Healing,
                ..definition(Vec::new())
            }]),
        ));
        let event = world
            .spawn(EventContext {
                kind: EventKind::Damage,
                source: None,
                targets: Vec::new(),
                controller: PlayerId::One,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            })
            .id();
        assert_that!(
            collect_trigger_candidates(&mut world, queue, event)
                .unwrap()
                .is_empty(),
            is_true()
        );
        world.entity_mut(event).insert(ResolutionIdentity {
            id: ResolutionId(4),
            kind: ResolutionKind::Event,
        });
        assert_that!(
            collect_trigger_candidates(&mut world, queue, event)
                .unwrap()
                .is_empty(),
            is_true()
        );
    }

    #[googletest::test]
    fn trigger_eligibility_checks_source_definition_zone_and_each_condition() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let source = world
            .spawn((
                GameObject,
                GameEntityId(7),
                EntityKind::Minion,
                Controller(PlayerId::One),
                Zone::Play,
                RuntimeTriggers(vec![definition(vec![
                    TimedCondition {
                        timing: ConditionTiming::ResolutionTime,
                        condition: TriggerCondition::SourceInPlay,
                    },
                    TimedCondition {
                        timing: ConditionTiming::ResolutionTime,
                        condition: TriggerCondition::SourceInZone(Zone::Play),
                    },
                    TimedCondition {
                        timing: ConditionTiming::ResolutionTime,
                        condition: TriggerCondition::ControllerIs(PlayerId::One),
                    },
                    TimedCondition {
                        timing: ConditionTiming::ResolutionTime,
                        condition: TriggerCondition::EventValueAtLeast(3),
                    },
                ])]),
            ))
            .id();
        let event_entity = world
            .spawn((
                ResolutionIdentity {
                    id: ResolutionId(3),
                    kind: ResolutionKind::Event,
                },
                EventContext {
                    kind: EventKind::Damage,
                    source: None,
                    targets: Vec::new(),
                    controller: PlayerId::Two,
                    proposed_value: Some(3),
                    actual_value: None,
                    simultaneous_ordinal: 0,
                },
            ))
            .id();
        let queued = QueuedTrigger {
            source: GameEntityId(7),
            event: ResolutionId(3),
            event_entity,
            definition_index: 0,
            order: TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order: 0,
                source: GameEntityId(7),
                tie_breaker: 0,
            },
        };

        assert_that!(
            trigger_is_eligible(&world, &queued, ConditionTiming::ResolutionTime),
            is_true()
        );
        assert_that!(
            trigger_is_eligible(
                &world,
                &QueuedTrigger {
                    source: GameEntityId(99),
                    ..queued
                },
                ConditionTiming::ResolutionTime
            ),
            is_false()
        );
        assert_that!(
            trigger_is_eligible(
                &world,
                &QueuedTrigger {
                    definition_index: 9,
                    ..queued
                },
                ConditionTiming::ResolutionTime
            ),
            is_false()
        );
        world.entity_mut(source).insert(Zone::Hand);
        assert_that!(
            trigger_is_eligible(&world, &queued, ConditionTiming::ResolutionTime),
            is_false()
        );
    }

    #[googletest::test]
    fn trigger_guards_block_repeated_events_and_direct_self_nesting() {
        let mut world = World::new();
        world.init_resource::<TriggerGuards>();
        let event_entity = world.spawn_empty().id();
        let queued = QueuedTrigger {
            source: GameEntityId(7),
            event: ResolutionId(3),
            event_entity,
            definition_index: 0,
            order: TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order: 0,
                source: GameEntityId(7),
                tie_breaker: 0,
            },
        };
        let definition = definition(Vec::new());

        assert_that!(
            begin_trigger_execution(&mut world, &queued, &definition),
            is_true()
        );
        assert_that!(
            begin_trigger_execution(&mut world, &queued, &definition),
            is_false()
        );
        finish_trigger_execution(&mut world, &queued);
        assert_that!(
            begin_trigger_execution(&mut world, &queued, &definition),
            is_false()
        );

        let nested_event = QueuedTrigger {
            event: ResolutionId(4),
            ..queued
        };
        assert_that!(
            begin_trigger_execution(&mut world, &nested_event, &definition),
            is_true()
        );
        assert_that!(
            begin_trigger_execution(
                &mut world,
                &QueuedTrigger {
                    event: ResolutionId(5),
                    ..queued
                },
                &definition
            ),
            is_false()
        );
        finish_trigger_execution(&mut world, &nested_event);
        reset_trigger_guards(&mut world);
        assert_that!(
            begin_trigger_execution(&mut world, &queued, &definition),
            is_true()
        );
    }

    #[googletest::test]
    fn condition_and_zone_helpers_cover_all_rule_variants() {
        let mut world = World::new();
        world.init_resource::<crate::entity::GameEntityIndex>();
        let source = world
            .spawn((
                crate::GameObject,
                GameEntityId(7),
                Controller(PlayerId::Two),
                Zone::Secret,
            ))
            .id();
        let event = EventContext {
            kind: EventKind::Damage,
            source: None,
            targets: Vec::new(),
            controller: PlayerId::One,
            proposed_value: Some(2),
            actual_value: Some(5),
            simultaneous_ordinal: 0,
        };

        assert_that!(
            evaluate_condition(&world, source, None, &TriggerCondition::Always),
            is_true()
        );
        assert_that!(
            evaluate_condition(&world, source, None, &TriggerCondition::SourceInPlay),
            is_false()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                None,
                &TriggerCondition::SourceInZone(Zone::Secret)
            ),
            is_true()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                Some(&event),
                &TriggerCondition::EventValueAtLeast(4)
            ),
            is_true()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                None,
                &TriggerCondition::EventValueAtLeast(1)
            ),
            is_false()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                Some(&EventContext {
                    source: Some(GameEntityId(7)),
                    ..event.clone()
                }),
                &TriggerCondition::EventSourceIsSelf
            ),
            is_true()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                Some(&EventContext {
                    targets: vec![GameEntityId(7)],
                    ..event.clone()
                }),
                &TriggerCondition::EventTargetsSelf
            ),
            is_true()
        );
        assert_that!(
            evaluate_condition(
                &world,
                source,
                None,
                &TriggerCondition::ControllerIs(PlayerId::Two)
            ),
            is_true()
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
