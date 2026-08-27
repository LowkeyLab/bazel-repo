use bevy::prelude::*;

use crate::{
    Armor, CanonicalTrace, Controller, Damage, EventContext, EventKind, GameEntityId, Keyword,
    Keywords, PlayOrder, PlayerId, ResolutionCursor, ResolutionKind, TraceEntry,
    entity::game_entity,
    resolver::{activate_resolution_child, complete_active, consume_budget, push_resolution},
};

use super::{
    error::SimulationError,
    event_resolver::{
        add_prepared_event, freeze_prepared_event_queue, prepare_collecting_event_queue,
        prepare_event_child, resolve_prepared_event, resolve_prepared_events,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DamageRequest {
    pub(super) source: Option<GameEntityId>,
    pub(super) target: GameEntityId,
    pub(super) proposed: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HealingRequest {
    pub(super) source: Option<GameEntityId>,
    pub(super) target: GameEntityId,
    pub(super) proposed: i32,
}

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
    if requests.is_empty() {
        return Ok(());
    }
    if world.resource::<ResolutionCursor>().active.is_none() {
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
        return Ok(());
    }

    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;
    let actual_queue = prepare_collecting_event_queue(world, batch)?;
    for (ordinal, mut request) in requests.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).expect("damage batch exceeds u32");
        request.proposed = request.proposed.max(0);
        if !damage_passes_protection(world, request) {
            trace_damage(world, request, 0);
            continue;
        }

        let proposed_event = EventContext {
            kind: EventKind::ProposedDamage,
            source: request.source,
            targets: vec![request.target],
            controller: event_controller(world, request.source, request.target),
            proposed_value: Some(request.proposed),
            actual_value: None,
            simultaneous_ordinal: ordinal,
        };
        request.proposed = resolve_proposed_health_event(world, batch, proposed_event)?;
        let actual = reduce_damage(world, request, ordinal);
        if actual.actual_value.is_some_and(|value| value > 0) {
            add_prepared_event(world, actual_queue, actual, None)?;
        }
    }

    freeze_prepared_event_queue(world, actual_queue)?;
    activate_resolution_child(world, actual_queue)?;
    resolve_prepared_events(world, actual_queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
    Ok(())
}

fn damage_passes_protection(world: &mut World, request: DamageRequest) -> bool {
    if request.proposed == 0 {
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

fn resolve_proposed_health_event(
    world: &mut World,
    parent: Entity,
    event: EventContext,
) -> Result<i32, SimulationError> {
    let event_entity = prepare_event_child(world, parent, event, None)?;
    activate_resolution_child(world, event_entity)?;
    resolve_prepared_event(world, event_entity)?;
    let proposed = world
        .get::<EventContext>(event_entity)
        .and_then(|event| event.proposed_value)
        .unwrap_or_default()
        .max(0);
    complete_active(world)?;
    Ok(proposed)
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
    if requests.is_empty() {
        return Ok(());
    }
    if world.resource::<ResolutionCursor>().active.is_none() {
        for (ordinal, request) in requests.into_iter().enumerate() {
            reduce_healing(
                world,
                request,
                u32::try_from(ordinal).expect("healing batch exceeds u32"),
            );
        }
        return Ok(());
    }

    let batch = push_resolution(world, ResolutionKind::EventBatch)?;
    consume_budget(world)?;
    let actual_queue = prepare_collecting_event_queue(world, batch)?;
    for (ordinal, mut request) in requests.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).expect("healing batch exceeds u32");
        request.proposed = request.proposed.max(0);
        if request.proposed > 0 {
            let proposed_event = EventContext {
                kind: EventKind::ProposedHealing,
                source: request.source,
                targets: vec![request.target],
                controller: event_controller(world, request.source, request.target),
                proposed_value: Some(request.proposed),
                actual_value: None,
                simultaneous_ordinal: ordinal,
            };
            request.proposed = resolve_proposed_health_event(world, batch, proposed_event)?;
        }
        let actual = reduce_healing(world, request, ordinal);
        if actual.actual_value.is_some_and(|value| value > 0) {
            add_prepared_event(world, actual_queue, actual, None)?;
        }
    }

    freeze_prepared_event_queue(world, actual_queue)?;
    activate_resolution_child(world, actual_queue)?;
    resolve_prepared_events(world, actual_queue)?;
    complete_active(world)?;
    debug_assert_eq!(world.resource::<ResolutionCursor>().active, Some(batch));
    complete_active(world)?;
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
