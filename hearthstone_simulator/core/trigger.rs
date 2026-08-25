use bevy::prelude::{Component, Entity, World};

use crate::{
    Controller, EntityKind, EventContext, EventKind, GameEntityId, PlayOrder, PlayerId,
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
    pub effect_program: String,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeTriggers(pub Vec<TriggerDefinition>);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggerExecution {
    pub source: GameEntityId,
    pub controller: PlayerId,
    pub source_kind: EntityKind,
}

pub(crate) fn collect_trigger_candidates(
    world: &mut World,
    queue: Entity,
    event_entity: Entity,
) -> Result<Vec<Entity>, QueueMutationError> {
    let Some(event) = world.get::<EventContext>(event_entity).cloned() else {
        return Ok(Vec::new());
    };
    let event_id = world
        .get::<ResolutionIdentity>(event_entity)
        .map(|identity| identity.id)
        .unwrap_or(ResolutionId(u64::MAX));
    let mut candidates = Vec::new();
    for entity in world.iter_entities() {
        let (Some(source), Some(triggers), Some(zone), Some(controller)) = (
            entity.get::<GameEntityId>(),
            entity.get::<RuntimeTriggers>(),
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
            let queued = QueuedTrigger {
                source: *source,
                event: event_id,
                definition_index: definition_index as u32,
                order: TriggerOrderKey {
                    player_bucket: controller.0.bucket(),
                    zone_bucket: zone_bucket(*zone),
                    priority: definition.priority,
                    play_order,
                    tie_breaker: definition_index as u32,
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
    let event = world.iter_entities().find_map(|entity| {
        (entity
            .get::<ResolutionIdentity>()
            .is_some_and(|identity| identity.id == queued.event))
        .then(|| entity.get::<EventContext>())
        .flatten()
    });
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

    use super::*;
    use crate::{
        GameObject, QueueKind, QueueState, ResolutionKind, ResolutionQueue, entity::GameEntityIndex,
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
            effect_program: "synthetic:test".to_string(),
        }
    }

    #[test]
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
                effect_program: "synthetic:test".to_string(),
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

        assert_eq!(entries.len(), 1);
        assert_eq!(
            world.get::<QueuedTrigger>(entries[0]).unwrap().source,
            GameEntityId(7)
        );
    }

    #[test]
    fn collection_skips_missing_events_and_nonmatching_definitions() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();
        let not_an_event = world.spawn_empty().id();
        assert!(
            collect_trigger_candidates(&mut world, queue, not_an_event)
                .unwrap()
                .is_empty()
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
        assert!(
            collect_trigger_candidates(&mut world, queue, event)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
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
        world.spawn((
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
        ));
        let queued = QueuedTrigger {
            source: GameEntityId(7),
            event: ResolutionId(3),
            definition_index: 0,
            order: TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order: 0,
                tie_breaker: 0,
            },
        };

        assert!(trigger_is_eligible(
            &world,
            &queued,
            ConditionTiming::ResolutionTime
        ));
        assert!(!trigger_is_eligible(
            &world,
            &QueuedTrigger {
                source: GameEntityId(99),
                ..queued
            },
            ConditionTiming::ResolutionTime
        ));
        assert!(!trigger_is_eligible(
            &world,
            &QueuedTrigger {
                definition_index: 9,
                ..queued
            },
            ConditionTiming::ResolutionTime
        ));
        world.entity_mut(source).insert(Zone::Hand);
        assert!(!trigger_is_eligible(
            &world,
            &queued,
            ConditionTiming::ResolutionTime
        ));
    }

    #[test]
    fn condition_and_zone_helpers_cover_all_rule_variants() {
        let mut world = World::new();
        let source = world.spawn((Controller(PlayerId::Two), Zone::Secret)).id();
        let event = EventContext {
            kind: EventKind::Damage,
            source: None,
            targets: Vec::new(),
            controller: PlayerId::One,
            proposed_value: Some(2),
            actual_value: Some(5),
            simultaneous_ordinal: 0,
        };

        assert!(evaluate_condition(
            &world,
            source,
            None,
            &TriggerCondition::Always
        ));
        assert!(!evaluate_condition(
            &world,
            source,
            None,
            &TriggerCondition::SourceInPlay
        ));
        assert!(evaluate_condition(
            &world,
            source,
            None,
            &TriggerCondition::SourceInZone(Zone::Secret)
        ));
        assert!(evaluate_condition(
            &world,
            source,
            Some(&event),
            &TriggerCondition::EventValueAtLeast(4)
        ));
        assert!(!evaluate_condition(
            &world,
            source,
            None,
            &TriggerCondition::EventValueAtLeast(1)
        ));
        assert!(evaluate_condition(
            &world,
            source,
            None,
            &TriggerCondition::ControllerIs(PlayerId::Two)
        ));
        assert_eq!(
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
            [0, 1, 2, 3, 4, 5, 6]
        );
    }
}
