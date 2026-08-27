use bevy::prelude::*;

use crate::{
    CanonicalTrace, Controller, DeathRecord, EffectContext, EntityKind, EventContext, EventKind,
    EventOrderKey, NestedUnder, QueueKind, QueueState, QueuedEvent, QueuedTrigger,
    ResolutionCursor, ResolutionIdentity, ResolutionKind, ResolutionQueue, ResolvePhaseBoundary,
    RuntimeTriggers, TraceEntry, TriggerExecution,
    death::take_pending_deaths,
    entity::game_entity,
    queue::{
        QueueSelection, abort_selected, add_event_entry, finish_selected, freeze_queue, select_next,
    },
    resolver::{
        ResolutionError, activate_resolution_child, allocate_resolution_id, complete_active,
        consume_budget, push_resolution, spawn_resolution_child,
    },
    trigger::{begin_trigger_execution, collect_trigger_candidates, finish_trigger_execution},
};

use super::{effect_executor::execute_effects, error::SimulationError};

pub(super) fn resolve_phase_boundaries(world: &mut World) -> Result<(), SimulationError> {
    loop {
        push_resolution(world, ResolutionKind::PhaseBoundary)?;
        consume_budget(world)?;
        world.run_schedule(ResolvePhaseBoundary);
        complete_active(world)?;

        let deaths = take_pending_deaths(world);
        if deaths.is_empty() {
            return Ok(());
        }
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::DeathPhaseQueued {
                deaths: deaths.iter().map(|record| record.entity).collect(),
            });
        push_resolution(world, ResolutionKind::DeathPhase)?;
        consume_budget(world)?;
        resolve_death_event_batch(world, deaths)?;
        complete_active(world)?;
    }
}

fn resolve_death_event_batch(
    world: &mut World,
    deaths: Vec<DeathRecord>,
) -> Result<(), SimulationError> {
    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;

    // Every Death Event and its trigger queue is created and frozen before the first event starts
    // resolving. Effects introduced by an earlier Deathrattle therefore cannot observe a later
    // death from this simultaneous batch.
    let events = deaths
        .into_iter()
        .map(|record| {
            (
                EventContext {
                    kind: EventKind::Death,
                    source: Some(record.entity),
                    targets: vec![record.entity],
                    controller: record.controller,
                    proposed_value: None,
                    actual_value: None,
                    simultaneous_ordinal: record.simultaneous_ordinal,
                },
                Some(record),
            )
        })
        .collect();
    let (queue, _) = prepare_event_queue(world, events)?;
    resolve_prepared_events(world, queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
    Ok(())
}

fn prepare_event_queue(
    world: &mut World,
    events: Vec<(EventContext, Option<DeathRecord>)>,
) -> Result<(Entity, Vec<Entity>), SimulationError> {
    let parent = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let queue = prepare_collecting_event_queue(world, parent)?;
    activate_resolution_child(world, queue)?;

    let mut event_entities = Vec::with_capacity(events.len());
    for (event, death_record) in events {
        event_entities.push(add_prepared_event(world, queue, event, death_record)?);
    }
    freeze_prepared_event_queue(world, queue)?;
    Ok((queue, event_entities))
}

pub(super) fn prepare_collecting_event_queue(
    world: &mut World,
    parent: Entity,
) -> Result<Entity, SimulationError> {
    let queue = spawn_resolution_child(world, parent, ResolutionKind::EventQueue);
    consume_budget(world)?;
    world
        .entity_mut(queue)
        .insert((ResolutionQueue(QueueKind::Events), QueueState::Collecting));
    Ok(queue)
}

pub(super) fn add_prepared_event(
    world: &mut World,
    queue: Entity,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<Entity, SimulationError> {
    let ordinal = event.simultaneous_ordinal;
    let event_entity = prepare_event_child(world, queue, event, death_record)?;
    let event_id = world
        .get::<ResolutionIdentity>(event_entity)
        .expect("prepared event has an identity")
        .id;
    add_event_entry(
        world,
        queue,
        QueuedEvent {
            event: event_id,
            event_entity,
            order: EventOrderKey {
                player_bucket: 0,
                ordinal,
                tie_breaker: 0,
            },
        },
    )
    .expect("new event queue remains collecting while entries are prepared");
    Ok(event_entity)
}

pub(super) fn freeze_prepared_event_queue(
    world: &mut World,
    queue: Entity,
) -> Result<(), SimulationError> {
    let queue_id = world
        .get::<ResolutionIdentity>(queue)
        .expect("event queue has an identity")
        .id;
    let frozen_ids = freeze_queue(world, queue)?
        .iter()
        .map(|entry| {
            world
                .get::<QueuedEvent>(*entry)
                .expect("event queue entry has a payload")
                .event
        })
        .collect();
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::QueueFrozen {
            queue: queue_id,
            entries: frozen_ids,
        });
    Ok(())
}

pub(super) fn resolve_prepared_events(
    world: &mut World,
    queue: Entity,
) -> Result<(), SimulationError> {
    loop {
        match select_next(world, queue)? {
            QueueSelection::Complete => return Ok(()),
            QueueSelection::Aborted(_) => {}
            QueueSelection::Selected(entry) => {
                let event_entity = world
                    .get::<QueuedEvent>(entry)
                    .expect("selected event entry has a payload")
                    .event_entity;
                activate_resolution_child(world, event_entity)?;
                resolve_prepared_event(world, event_entity)?;
                complete_active(world)?;
                finish_selected(world, queue, entry)?;
            }
        }
    }
}

pub(super) fn resolve_event_if_active(
    world: &mut World,
    event: EventContext,
) -> Result<(), SimulationError> {
    if world.resource::<ResolutionCursor>().active.is_none() {
        return Ok(());
    }
    resolve_event(world, event)
}

fn resolve_event(world: &mut World, event: EventContext) -> Result<(), SimulationError> {
    resolve_event_with_death_record(world, event, None)
}

fn resolve_event_with_death_record(
    world: &mut World,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<(), SimulationError> {
    let parent = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let event_entity = prepare_event_child(world, parent, event, death_record)?;
    activate_resolution_child(world, event_entity)?;
    resolve_prepared_event(world, event_entity)?;
    complete_active(world)?;
    Ok(())
}

pub(super) fn prepare_event_child(
    world: &mut World,
    parent: Entity,
    event: EventContext,
    death_record: Option<DeathRecord>,
) -> Result<Entity, SimulationError> {
    let event_entity = spawn_resolution_child(world, parent, ResolutionKind::Event);
    consume_budget(world)?;
    let event_identity = *world
        .get::<ResolutionIdentity>(event_entity)
        .expect("new event frame has an identity");
    let trace = TraceEntry::EventCreated {
        id: event_identity.id,
        kind: event.kind,
        source: event.source,
        targets: event.targets.clone(),
        proposed: event.proposed_value,
        actual: event.actual_value,
    };
    world.entity_mut(event_entity).insert(event);
    if let Some(record) = death_record {
        world.entity_mut(event_entity).insert(record);
    }
    world.resource_mut::<CanonicalTrace>().entries.push(trace);

    let queue = spawn_resolution_child(world, event_entity, ResolutionKind::TriggerQueue);
    consume_budget(world)?;
    let queue_identity = *world
        .get::<ResolutionIdentity>(queue)
        .expect("new trigger queue has an identity");
    world
        .entity_mut(queue)
        .insert((ResolutionQueue(QueueKind::Triggers), QueueState::Collecting));
    let entries = collect_trigger_candidates(world, queue, event_entity)?;
    for entry in entries {
        let id = allocate_resolution_id(world);
        world.entity_mut(entry).insert(ResolutionIdentity {
            id,
            kind: ResolutionKind::Trigger,
        });
    }
    let frozen = freeze_queue(world, queue)?;
    let frozen_ids = frozen
        .iter()
        .map(|entry| {
            world
                .get::<ResolutionIdentity>(*entry)
                .expect("collected queue entry has an identity")
                .id
        })
        .collect();
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::QueueFrozen {
            queue: queue_identity.id,
            entries: frozen_ids,
        });
    Ok(event_entity)
}

pub(super) fn resolve_prepared_event(
    world: &mut World,
    event_entity: Entity,
) -> Result<(), SimulationError> {
    let event = world
        .get::<EventContext>(event_entity)
        .expect("prepared event has event context")
        .clone();
    let queue = world
        .iter_entities()
        .find_map(|entity| {
            (entity.get::<NestedUnder>().map(|parent| parent.0) == Some(event_entity)
                && entity.get::<ResolutionQueue>() == Some(&ResolutionQueue(QueueKind::Triggers)))
            .then_some(entity.id())
        })
        .expect("prepared event has a trigger queue");
    activate_resolution_child(world, queue)?;
    loop {
        match select_next(world, queue)? {
            QueueSelection::Complete => break,
            QueueSelection::Aborted(entry) => trace_trigger_aborted(world, entry),
            QueueSelection::Selected(entry) => {
                resolve_selected_trigger(world, queue, entry, &event)?;
            }
        }
    }
    complete_active(world)?;
    Ok(())
}

fn resolve_selected_trigger(
    world: &mut World,
    queue: Entity,
    entry: Entity,
    event: &EventContext,
) -> Result<(), SimulationError> {
    let queued = *world
        .get::<QueuedTrigger>(entry)
        .expect("selected trigger entry has a payload");
    let entry_id = world
        .get::<ResolutionIdentity>(entry)
        .expect("selected trigger entry has an identity")
        .id;
    let Some(source_entity) = game_entity(world, queued.source) else {
        abort_selected(world, queue, entry)?;
        trace_trigger_aborted(world, entry);
        return Ok(());
    };
    let Some(definition) = world
        .get::<RuntimeTriggers>(source_entity)
        .and_then(|triggers| triggers.0.get(queued.definition_index as usize))
        .cloned()
    else {
        abort_selected(world, queue, entry)?;
        trace_trigger_aborted(world, entry);
        return Ok(());
    };
    if !begin_trigger_execution(world, &queued, &definition) {
        abort_selected(world, queue, entry)?;
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::TriggerAborted {
                id: entry_id,
                source: queued.source,
            });
        return Ok(());
    }

    let controller = world
        .get::<Controller>(source_entity)
        .expect("trigger source has a controller")
        .0;
    let source_kind = *world
        .get::<EntityKind>(source_entity)
        .expect("trigger source has an entity kind");
    let trigger = push_resolution(world, ResolutionKind::Trigger)?;
    consume_budget(world)?;
    world.entity_mut(trigger).insert(TriggerExecution {
        source: queued.source,
        controller,
        source_kind,
    });
    let result = execute_effects(
        world,
        &EffectContext {
            source: Some(queued.source),
            controller,
            declared_target: event.targets.first().copied(),
        },
        &definition.effect_program,
    );
    finish_trigger_execution(world, &queued);
    complete_active(world)?;
    finish_selected(world, queue, entry)?;
    result?;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TriggerResolved {
            id: entry_id,
            source: queued.source,
        });
    Ok(())
}

fn trace_trigger_aborted(world: &mut World, entry: Entity) {
    let (Some(identity), Some(trigger)) = (
        world.get::<ResolutionIdentity>(entry),
        world.get::<QueuedTrigger>(entry),
    ) else {
        return;
    };
    let (id, source) = (identity.id, trigger.source);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TriggerAborted { id, source });
}
