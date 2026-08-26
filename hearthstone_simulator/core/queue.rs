use bevy::prelude::*;
use thiserror::Error;

use crate::{
    ConditionTiming, GameEntityId, QueuedIn, ResolutionId, entity::game_entity,
    trigger::trigger_is_eligible,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QueueKind {
    Events,
    Triggers,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct ResolutionQueue(pub QueueKind);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueState {
    Collecting,
    Frozen,
    Resolving,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TriggerOrderKey {
    pub player_bucket: u8,
    pub zone_bucket: u8,
    pub priority: i16,
    pub play_order: u64,
    pub source: GameEntityId,
    pub tie_breaker: u32,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct QueuedTrigger {
    pub source: GameEntityId,
    pub event: ResolutionId,
    #[entities]
    pub event_entity: Entity,
    pub definition_index: u32,
    pub order: TriggerOrderKey,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventOrderKey {
    pub player_bucket: u8,
    pub ordinal: u32,
    pub tie_breaker: u32,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct QueuedEvent {
    pub event: ResolutionId,
    pub order: EventOrderKey,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueEntryStatus {
    Pending,
    Resolving,
    Resolved,
    Aborted,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct FrozenQueueEntries {
    #[entities]
    pub entries: Vec<Entity>,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueueCursor(pub usize);

pub(crate) fn add_trigger_entry(
    world: &mut World,
    queue: Entity,
    trigger: QueuedTrigger,
) -> Result<Entity, QueueMutationError> {
    if world.get::<QueueState>(queue) != Some(&QueueState::Collecting) {
        return Err(QueueMutationError::NotCollecting);
    }
    Ok(world
        .spawn((trigger, QueueEntryStatus::Pending, QueuedIn(queue)))
        .id())
}

pub(crate) fn add_event_entry(
    world: &mut World,
    queue: Entity,
    event: QueuedEvent,
) -> Result<Entity, QueueMutationError> {
    if world.get::<QueueState>(queue) != Some(&QueueState::Collecting) {
        return Err(QueueMutationError::NotCollecting);
    }
    Ok(world
        .spawn((event, QueueEntryStatus::Pending, QueuedIn(queue)))
        .id())
}

pub(crate) fn freeze_queue(
    world: &mut World,
    queue: Entity,
) -> Result<Vec<Entity>, QueueMutationError> {
    if world.get::<QueueState>(queue) != Some(&QueueState::Collecting) {
        return Err(QueueMutationError::NotCollecting);
    }
    let kind = world
        .get::<ResolutionQueue>(queue)
        .ok_or(QueueMutationError::MissingQueue)?
        .0;
    let entries: Vec<Entity> = match kind {
        QueueKind::Triggers => {
            let mut query = world.query::<(Entity, &QueuedTrigger, &QueuedIn)>();
            let mut entries = query
                .iter(world)
                .filter_map(|(entity, trigger, owner)| {
                    (owner.0 == queue).then_some((trigger.order, entity))
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|(order, _)| *order);
            entries.into_iter().map(|(_, entity)| entity).collect()
        }
        QueueKind::Events => {
            let mut query = world.query::<(Entity, &QueuedEvent, &QueuedIn)>();
            let mut entries = query
                .iter(world)
                .filter_map(|(entity, event, owner)| {
                    (owner.0 == queue).then_some((event.order, entity))
                })
                .collect::<Vec<_>>();
            entries.sort_by_key(|(order, _)| *order);
            entries.into_iter().map(|(_, entity)| entity).collect()
        }
    };
    world.entity_mut(queue).insert((
        FrozenQueueEntries {
            entries: entries.clone(),
        },
        QueueCursor::default(),
        QueueState::Frozen,
    ));
    Ok(entries)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueSelection {
    Selected(Entity),
    Aborted(Entity),
    Complete,
}

pub(crate) fn select_next(
    world: &mut World,
    queue: Entity,
) -> Result<QueueSelection, QueueMutationError> {
    if !matches!(
        world.get::<QueueState>(queue),
        Some(QueueState::Frozen | QueueState::Resolving)
    ) {
        return Err(QueueMutationError::NotFrozen);
    }
    let cursor = world
        .get::<QueueCursor>(queue)
        .ok_or(QueueMutationError::NotFrozen)?
        .0;
    let entries = world
        .get::<FrozenQueueEntries>(queue)
        .ok_or(QueueMutationError::NotFrozen)?;
    let Some(entry) = entries.entries.get(cursor).copied() else {
        world.entity_mut(queue).insert(QueueState::Complete);
        return Ok(QueueSelection::Complete);
    };
    let source_is_valid = world.get::<QueuedTrigger>(entry).is_none_or(|trigger| {
        game_entity(world, trigger.source).is_some()
            && trigger_is_eligible(world, trigger, ConditionTiming::ResolutionTime)
    });
    if !source_is_valid {
        world.entity_mut(entry).insert(QueueEntryStatus::Aborted);
        world.get_mut::<QueueCursor>(queue).unwrap().0 += 1;
        return Ok(QueueSelection::Aborted(entry));
    }
    world.entity_mut(entry).insert(QueueEntryStatus::Resolving);
    world.entity_mut(queue).insert(QueueState::Resolving);
    Ok(QueueSelection::Selected(entry))
}

pub(crate) fn finish_selected(
    world: &mut World,
    queue: Entity,
    entry: Entity,
) -> Result<(), QueueMutationError> {
    finish_selected_with_status(world, queue, entry, QueueEntryStatus::Resolved)
}

pub(crate) fn abort_selected(
    world: &mut World,
    queue: Entity,
    entry: Entity,
) -> Result<(), QueueMutationError> {
    finish_selected_with_status(world, queue, entry, QueueEntryStatus::Aborted)
}

fn finish_selected_with_status(
    world: &mut World,
    queue: Entity,
    entry: Entity,
    status: QueueEntryStatus,
) -> Result<(), QueueMutationError> {
    let cursor = world
        .get::<QueueCursor>(queue)
        .ok_or(QueueMutationError::NotFrozen)?
        .0;
    if world
        .get::<FrozenQueueEntries>(queue)
        .and_then(|entries| entries.entries.get(cursor))
        != Some(&entry)
    {
        return Err(QueueMutationError::WrongEntry);
    }
    world.entity_mut(entry).insert(status);
    world.get_mut::<QueueCursor>(queue).unwrap().0 += 1;
    world.entity_mut(queue).insert(QueueState::Frozen);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QueueMutationError {
    #[error("resolution queue is missing its queue kind")]
    MissingQueue,
    #[error("resolution queue is not collecting entries")]
    NotCollecting,
    #[error("resolution queue is not frozen")]
    NotFrozen,
    #[error("queue entry is not the currently selected entry")]
    WrongEntry,
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{
        Controller, EntityKind, GameObject, PlayerId, RuntimeTriggers, SourceEligibilityPolicy,
        TriggerDefinition, WoundedTargetPolicy, Zone, entity::GameEntityIndex,
    };

    fn trigger(source: u64, play_order: u64, tie_breaker: u32) -> QueuedTrigger {
        QueuedTrigger {
            source: GameEntityId(source),
            event: ResolutionId(1),
            event_entity: Entity::PLACEHOLDER,
            definition_index: 0,
            order: TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order,
                source: GameEntityId(source),
                tie_breaker,
            },
        }
    }

    #[googletest::test]
    fn event_queue_does_not_advance_until_selected_entry_finishes() {
        let mut world = World::new();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Events), QueueState::Collecting))
            .id();
        let event = add_event_entry(
            &mut world,
            queue,
            QueuedEvent {
                event: ResolutionId(4),
                order: EventOrderKey {
                    player_bucket: 0,
                    ordinal: 0,
                    tie_breaker: 0,
                },
            },
        )
        .expect("collecting queue accepts events");
        freeze_queue(&mut world, queue).expect("queue should freeze");

        assert_that!(
            select_next(&mut world, queue).expect("queue should select"),
            eq(QueueSelection::Selected(event))
        );
        assert_that!(world.get::<QueueCursor>(queue).unwrap().0, eq(0));
        abort_selected(&mut world, queue, event).expect("selected event should abort");
        assert_that!(
            world.get::<QueueEntryStatus>(event),
            eq(Some(&QueueEntryStatus::Aborted))
        );
        assert_that!(world.get::<QueueCursor>(queue).unwrap().0, eq(1));
        assert_that!(
            select_next(&mut world, queue).expect("queue should complete"),
            eq(QueueSelection::Complete)
        );
    }

    #[googletest::test]
    fn freeze_sorts_complete_keys_and_rejects_late_entries() {
        let mut world = World::new();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();
        let later = add_trigger_entry(&mut world, queue, trigger(2, 20, 0))
            .expect("collecting queue accepts candidates");
        let first = add_trigger_entry(&mut world, queue, trigger(1, 10, 0))
            .expect("collecting queue accepts candidates");

        let frozen = freeze_queue(&mut world, queue).expect("queue should freeze");

        assert_that!(frozen, eq(&vec![first, later]));
        assert_that!(
            world.get::<FrozenQueueEntries>(queue).unwrap().entries,
            eq(&vec![first, later])
        );
        assert_that!(
            add_trigger_entry(&mut world, queue, trigger(3, 5, 0)),
            err(eq(QueueMutationError::NotCollecting))
        );
        assert_that!(world.get::<QueuedIn>(first).unwrap().0, eq(queue));
    }

    #[googletest::test]
    fn invalid_trigger_sources_abort_without_mutating_frozen_membership() {
        let mut world = World::new();
        world.init_resource::<crate::entity::GameEntityIndex>();
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();
        let entry = add_trigger_entry(&mut world, queue, trigger(404, 0, 0)).unwrap();
        freeze_queue(&mut world, queue).unwrap();

        assert_that!(
            select_next(&mut world, queue),
            ok(eq(QueueSelection::Aborted(entry)))
        );
        assert_that!(
            world.get::<QueueEntryStatus>(entry),
            eq(Some(&QueueEntryStatus::Aborted))
        );
        assert_that!(world.get::<QueueCursor>(queue), eq(Some(&QueueCursor(1))));
    }

    #[googletest::test]
    fn eligible_trigger_sources_can_be_selected() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        world.spawn((
            GameObject,
            GameEntityId(1),
            EntityKind::Minion,
            Controller(PlayerId::One),
            Zone::Play,
            RuntimeTriggers(vec![TriggerDefinition {
                event: crate::EventKind::Damage,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: SourceEligibilityPolicy::MustExist,
                priority: 0,
                allow_repeated_event: false,
                allow_direct_self_nesting: false,
                wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
                effect_program: Vec::new(),
            }]),
        ));
        let queue = world
            .spawn((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting))
            .id();
        let entry = add_trigger_entry(&mut world, queue, trigger(1, 0, 0)).unwrap();
        freeze_queue(&mut world, queue).unwrap();

        assert_that!(
            select_next(&mut world, queue),
            ok(eq(QueueSelection::Selected(entry)))
        );
    }

    #[googletest::test]
    fn queue_operations_report_invalid_states_and_entries() {
        let mut world = World::new();
        let incomplete = world.spawn(QueueState::Collecting).id();
        assert_that!(
            freeze_queue(&mut world, incomplete),
            err(eq(&QueueMutationError::MissingQueue))
        );

        let queue = world
            .spawn((ResolutionQueue(QueueKind::Events), QueueState::Collecting))
            .id();
        let later = add_event_entry(
            &mut world,
            queue,
            QueuedEvent {
                event: ResolutionId(2),
                order: EventOrderKey {
                    player_bucket: 1,
                    ordinal: 0,
                    tie_breaker: 0,
                },
            },
        )
        .unwrap();
        let first = add_event_entry(
            &mut world,
            queue,
            QueuedEvent {
                event: ResolutionId(1),
                order: EventOrderKey {
                    player_bucket: 0,
                    ordinal: 0,
                    tie_breaker: 0,
                },
            },
        )
        .unwrap();
        assert_that!(
            freeze_queue(&mut world, queue).unwrap(),
            eq(&vec![first, later])
        );
        assert_that!(
            add_event_entry(
                &mut world,
                queue,
                QueuedEvent {
                    event: ResolutionId(3),
                    order: EventOrderKey {
                        player_bucket: 0,
                        ordinal: 1,
                        tie_breaker: 0,
                    },
                },
            ),
            err(eq(QueueMutationError::NotCollecting))
        );
        let wrong = world.spawn_empty().id();
        assert_that!(
            finish_selected(&mut world, queue, wrong),
            err(eq(QueueMutationError::WrongEntry))
        );

        let collecting = world.spawn(QueueState::Collecting).id();
        assert_that!(
            select_next(&mut world, collecting),
            err(eq(QueueMutationError::NotFrozen))
        );
        assert_that!(
            finish_selected(&mut world, collecting, wrong),
            err(eq(QueueMutationError::NotFrozen))
        );
        world.entity_mut(queue).remove::<FrozenQueueEntries>();
        assert_that!(
            select_next(&mut world, queue),
            err(eq(QueueMutationError::NotFrozen))
        );
        assert_that!(
            freeze_queue(&mut world, queue),
            err(eq(&QueueMutationError::NotCollecting))
        );
    }
}
