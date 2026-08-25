use bevy::prelude::*;

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
    pub tie_breaker: u32,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct QueuedTrigger {
    pub source: GameEntityId,
    pub event: ResolutionId,
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
    world.entity_mut(entry).insert(QueueEntryStatus::Resolved);
    world.get_mut::<QueueCursor>(queue).unwrap().0 += 1;
    world.entity_mut(queue).insert(QueueState::Frozen);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueMutationError {
    MissingQueue,
    NotCollecting,
    NotFrozen,
    WrongEntry,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QueuedIn;

    fn trigger(source: u64, play_order: u64, tie_breaker: u32) -> QueuedTrigger {
        QueuedTrigger {
            source: GameEntityId(source),
            event: ResolutionId(1),
            definition_index: 0,
            order: TriggerOrderKey {
                player_bucket: 0,
                zone_bucket: 0,
                priority: 0,
                play_order,
                tie_breaker,
            },
        }
    }

    #[test]
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

        assert_eq!(
            select_next(&mut world, queue).expect("queue should select"),
            QueueSelection::Selected(event)
        );
        assert_eq!(world.get::<QueueCursor>(queue).unwrap().0, 0);
        finish_selected(&mut world, queue, event).expect("selected event should finish");
        assert_eq!(world.get::<QueueCursor>(queue).unwrap().0, 1);
        assert_eq!(
            select_next(&mut world, queue).expect("queue should complete"),
            QueueSelection::Complete
        );
    }

    #[test]
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

        assert_eq!(frozen, vec![first, later]);
        assert_eq!(
            world.get::<FrozenQueueEntries>(queue).unwrap().entries,
            vec![first, later]
        );
        assert_eq!(
            add_trigger_entry(&mut world, queue, trigger(3, 5, 0)),
            Err(QueueMutationError::NotCollecting)
        );
        assert_eq!(world.get::<QueuedIn>(first).unwrap().0, queue);
    }
}
