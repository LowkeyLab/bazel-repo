use bevy::prelude::*;

use crate::{
    Armor, CanonicalTrace, Controller, Damage, DamageRequest, EventContext, EventKind, EventSlotId,
    GameEntityId, HealingRequest, Keyword, Keywords, PlayOrder, PlayerId, ResolutionOp,
    ResolutionWork, TraceEntry,
    entity::game_entity,
    resolver::{
        allocate_event_slot, push_resolution_op, push_resolution_ops, resolution_is_active,
    },
};

use super::{
    error::SimulationError,
    event_resolver::{prepare_event, take_prepared_event},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SimultaneousEventOrder {
    OrderOfPlay,
    Given,
}

pub(super) fn apply_damage(
    world: &mut World,
    source: Option<GameEntityId>,
    target: GameEntityId,
    proposed: i32,
) -> Result<(), SimulationError> {
    apply_damage_batch(
        world,
        vec![DamageRequest {
            source,
            target,
            proposed,
        }],
        SimultaneousEventOrder::Given,
    )
}

pub(super) fn apply_damage_batch(
    world: &mut World,
    mut requests: Vec<DamageRequest>,
    order: SimultaneousEventOrder,
) -> Result<(), SimulationError> {
    validate_health_change_targets(world, requests.iter().map(|request| request.target))?;
    order_health_change_requests(world, &mut requests, order, |request| request.target);
    if resolution_is_active(world) {
        push_resolution_op(world, ResolutionOp::ProcessDamageBatch(requests));
    } else {
        for (ordinal, mut request) in requests.into_iter().enumerate() {
            request.proposed = request.proposed.max(0);
            if damage_passes_protection(world, request) {
                reduce_damage(
                    world,
                    request,
                    u32::try_from(ordinal).expect("damage batch exceeds u32"),
                );
            } else {
                trace_damage(world, request, 0);
            }
        }
    }
    Ok(())
}

pub(super) fn expand_damage_batch(world: &mut World, requests: Vec<DamageRequest>) {
    if requests.is_empty() {
        return;
    }
    let slots = (0..requests.len())
        .map(|_| allocate_event_slot(world))
        .collect::<Vec<_>>();
    let mut operations = requests
        .into_iter()
        .zip(slots.iter().copied())
        .enumerate()
        .map(
            |(ordinal, (request, actual_event))| ResolutionOp::ProcessDamage {
                request,
                actual_event,
                ordinal: u32::try_from(ordinal).expect("damage batch exceeds u32"),
            },
        )
        .collect::<Vec<_>>();
    operations.extend(slots.into_iter().map(ResolutionOp::ResolveEventSlot));
    push_resolution_ops(world, operations);
}

pub(super) fn process_damage(
    world: &mut World,
    mut request: DamageRequest,
    actual_event: EventSlotId,
    ordinal: u32,
) {
    request.proposed = request.proposed.max(0);
    if !damage_passes_protection(world, request) {
        trace_damage(world, request, 0);
        return;
    }
    let proposed_event = prepare_event(
        world,
        EventContext {
            kind: EventKind::ProposedDamage,
            source: request.source,
            targets: vec![request.target],
            controller: event_controller(world, request.source, request.target),
            proposed_value: Some(request.proposed),
            actual_value: None,
            simultaneous_ordinal: ordinal,
        },
    );
    push_resolution_ops(
        world,
        [
            ResolutionOp::ResolveEvent(proposed_event),
            ResolutionOp::ApplyDamage {
                request,
                proposed_event,
                actual_event,
                ordinal,
            },
        ],
    );
}

pub(super) fn apply_prepared_damage(
    world: &mut World,
    mut request: DamageRequest,
    proposed_event: crate::EventId,
    actual_event: EventSlotId,
    ordinal: u32,
) -> Result<(), SimulationError> {
    let proposed = take_prepared_event(world, proposed_event)?
        .context
        .proposed_value
        .unwrap_or_default()
        .max(0);
    request.proposed = proposed;
    let event = reduce_damage(world, request, ordinal);
    if event.actual_value.is_some_and(|actual| actual > 0) {
        fill_event_slot(world, actual_event, event)?;
    }
    Ok(())
}

fn damage_passes_protection(world: &mut World, request: DamageRequest) -> bool {
    if request.proposed <= 0 {
        return false;
    }
    let entity = game_entity(world, request.target)
        .expect("validated damage target remains indexed during damage prevention");
    if world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::Immune))
    {
        return false;
    }
    if world
        .get::<Keywords>(entity)
        .is_some_and(|keywords| keywords.0.contains(&Keyword::DivineShield))
    {
        world
            .get_mut::<Keywords>(entity)
            .expect("keywords were just read")
            .0
            .remove(&Keyword::DivineShield);
        return false;
    }
    true
}

fn reduce_damage(
    world: &mut World,
    request: DamageRequest,
    simultaneous_ordinal: u32,
) -> EventContext {
    let entity = game_entity(world, request.target)
        .expect("validated damage target remains indexed during damage mutation");
    let proposed = request.proposed.max(0);
    let armor = world.get::<Armor>(entity).map_or(0, |armor| armor.0);
    let absorbed = armor.min(proposed);
    if absorbed > 0 {
        world
            .get_mut::<Armor>(entity)
            .expect("armor was just read")
            .0 -= absorbed;
    }
    let health_damage = proposed - absorbed;
    if health_damage > 0 {
        let mut entity = world.entity_mut(entity);
        let mut damage = entity.entry::<Damage>().or_default().into_mut();
        damage.0 = damage.0.saturating_add(health_damage);
    }
    let actual = absorbed.saturating_add(health_damage);
    trace_damage(world, request, actual);
    EventContext {
        kind: EventKind::Damage,
        source: request.source,
        targets: vec![request.target],
        controller: event_controller(world, request.source, request.target),
        proposed_value: Some(proposed),
        actual_value: Some(actual),
        simultaneous_ordinal,
    }
}

fn trace_damage(world: &mut World, request: DamageRequest, actual: i32) {
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Damage {
            source: request.source,
            target: request.target,
            proposed: request.proposed,
            actual,
        });
}

pub(super) fn apply_healing_batch(
    world: &mut World,
    mut requests: Vec<HealingRequest>,
    order: SimultaneousEventOrder,
) -> Result<(), SimulationError> {
    validate_health_change_targets(world, requests.iter().map(|request| request.target))?;
    order_health_change_requests(world, &mut requests, order, |request| request.target);
    if resolution_is_active(world) {
        push_resolution_op(world, ResolutionOp::ProcessHealingBatch(requests));
    } else {
        for (ordinal, request) in requests.into_iter().enumerate() {
            reduce_healing(
                world,
                request,
                u32::try_from(ordinal).expect("healing batch exceeds u32"),
            );
        }
    }
    Ok(())
}

pub(super) fn expand_healing_batch(world: &mut World, requests: Vec<HealingRequest>) {
    if requests.is_empty() {
        return;
    }
    let slots = (0..requests.len())
        .map(|_| allocate_event_slot(world))
        .collect::<Vec<_>>();
    let mut operations = requests
        .into_iter()
        .zip(slots.iter().copied())
        .enumerate()
        .map(
            |(ordinal, (request, actual_event))| ResolutionOp::ProcessHealing {
                request,
                actual_event,
                ordinal: u32::try_from(ordinal).expect("healing batch exceeds u32"),
            },
        )
        .collect::<Vec<_>>();
    operations.extend(slots.into_iter().map(ResolutionOp::ResolveEventSlot));
    push_resolution_ops(world, operations);
}

pub(super) fn process_healing(
    world: &mut World,
    mut request: HealingRequest,
    actual_event: EventSlotId,
    ordinal: u32,
) {
    request.proposed = request.proposed.max(0);
    if request.proposed == 0 {
        reduce_healing(world, request, ordinal);
        return;
    }
    let proposed_event = prepare_event(
        world,
        EventContext {
            kind: EventKind::ProposedHealing,
            source: request.source,
            targets: vec![request.target],
            controller: event_controller(world, request.source, request.target),
            proposed_value: Some(request.proposed),
            actual_value: None,
            simultaneous_ordinal: ordinal,
        },
    );
    push_resolution_ops(
        world,
        [
            ResolutionOp::ResolveEvent(proposed_event),
            ResolutionOp::ApplyHealing {
                request,
                proposed_event,
                actual_event,
                ordinal,
            },
        ],
    );
}

pub(super) fn apply_prepared_healing(
    world: &mut World,
    mut request: HealingRequest,
    proposed_event: crate::EventId,
    actual_event: EventSlotId,
    ordinal: u32,
) -> Result<(), SimulationError> {
    request.proposed = take_prepared_event(world, proposed_event)?
        .context
        .proposed_value
        .unwrap_or_default()
        .max(0);
    let event = reduce_healing(world, request, ordinal);
    if event.actual_value.is_some_and(|actual| actual > 0) {
        fill_event_slot(world, actual_event, event)?;
    }
    Ok(())
}

fn reduce_healing(
    world: &mut World,
    request: HealingRequest,
    simultaneous_ordinal: u32,
) -> EventContext {
    let entity = game_entity(world, request.target)
        .expect("validated healing target remains indexed during simultaneous mutation");
    let proposed = request.proposed.max(0);
    let actual = if let Some(mut damage) = world.get_mut::<Damage>(entity) {
        let actual = damage.0.min(proposed);
        damage.0 -= actual;
        actual
    } else {
        0
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Healing {
            source: request.source,
            target: request.target,
            proposed,
            actual,
        });
    EventContext {
        kind: EventKind::Healing,
        source: request.source,
        targets: vec![request.target],
        controller: event_controller(world, request.source, request.target),
        proposed_value: Some(proposed),
        actual_value: Some(actual),
        simultaneous_ordinal,
    }
}

fn fill_event_slot(
    world: &mut World,
    slot: EventSlotId,
    context: EventContext,
) -> Result<(), SimulationError> {
    let event = prepare_event(world, context);
    let mut work = world.resource_mut::<ResolutionWork>();
    let prepared_slot = work
        .event_slots
        .get_mut(&slot)
        .ok_or(crate::resolver::ResolutionError::MissingEventSlot(slot))?;
    prepared_slot.event = Some(event);
    Ok(())
}

fn validate_health_change_targets(
    world: &World,
    targets: impl IntoIterator<Item = GameEntityId>,
) -> Result<(), SimulationError> {
    for target in targets {
        if game_entity(world, target).is_none() {
            return Err(SimulationError::EntityNotFound(target));
        }
    }
    Ok(())
}

fn order_health_change_requests<T>(
    world: &World,
    requests: &mut [T],
    order: SimultaneousEventOrder,
    target: impl Fn(&T) -> GameEntityId,
) {
    if order == SimultaneousEventOrder::OrderOfPlay {
        requests.sort_by_key(|request| {
            let target = target(request);
            let play_order = game_entity(world, target)
                .and_then(|entity| world.get::<PlayOrder>(entity))
                .map_or(0, |order| order.0);
            (play_order, target)
        });
    }
}

fn event_controller(world: &World, source: Option<GameEntityId>, target: GameEntityId) -> PlayerId {
    source
        .and_then(|source| game_entity(world, source))
        .or_else(|| game_entity(world, target))
        .and_then(|entity| world.get::<Controller>(entity))
        .map_or(PlayerId::One, |controller| controller.0)
}
