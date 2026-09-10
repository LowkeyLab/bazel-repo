use bevy::prelude::*;

use crate::{
    CanonicalTrace, ChoiceRequest, Controller, CurrentResolutionOp, DeathRecord,
    DrawContinuationPolicy, DrawOutcome, EffectContext, EventContext, EventId, EventKind,
    GameEntityId, GameState, PendingChoice, PhaseBoundaryPlan, PreparedEvent, ResolutionOp,
    ResolutionWork, ResolvePhaseBoundary, SimulationStatus, TraceEntry, TransformKind, TriggerSeed,
    death::take_pending_deaths,
    entity::game_entity,
    resolver::{push_resolution_op, push_resolution_ops},
    trigger::{
        ConditionTiming, collect_trigger_candidates, collect_trigger_seeds, trigger_is_eligible,
    },
};

use super::{
    action::run_sequence_step,
    effect_executor::{copy_entity, execute_effect_operation, push_effects, transform_entity},
    error::SimulationError,
    health::{
        apply_prepared_damage, apply_prepared_healing, expand_damage_batch, expand_healing_batch,
        process_damage, process_healing,
    },
    player::{check_outcome, process_draw},
};

#[derive(Default, Resource)]
pub(super) struct OperationFailure(pub Option<SimulationError>);

pub(super) fn execute_current_resolution_op(world: &mut World) {
    let Some(current) = world.resource_mut::<CurrentResolutionOp>().0.take() else {
        return;
    };
    if let Err(error) = execute_resolution_op(world, current.id, current.operation) {
        world.resource_mut::<OperationFailure>().0 = Some(error);
    }
}

fn execute_resolution_op(
    world: &mut World,
    operation_id: crate::ResolutionId,
    operation: ResolutionOp,
) -> Result<(), SimulationError> {
    match operation {
        ResolutionOp::RunSequenceStep(step) => run_sequence_step(world, &step),
        ResolutionOp::RunPhaseBoundary(plan) => {
            run_phase_boundary(world, plan);
            Ok(())
        }
        ResolutionOp::RefreshAuras(plan) => {
            match plan {
                crate::AuraRefreshPlan::PlayedProvider(provider) => {
                    crate::aura::refresh_played_provider(world, provider);
                }
                crate::AuraRefreshPlan::Summon => crate::aura::refresh_all_auras(world),
            }
            Ok(())
        }
        ResolutionOp::CheckOutcome => {
            check_outcome(world);
            Ok(())
        }
        ResolutionOp::PrepareEvent(context) => {
            let event = prepare_event(world, context);
            push_resolution_op(world, ResolutionOp::ResolveEvent(event));
            Ok(())
        }
        ResolutionOp::ResolveEvent(event) => expand_event(world, event),
        ResolutionOp::FinishEvent(event) => {
            take_prepared_event(world, event)?;
            Ok(())
        }
        ResolutionOp::ResolveEventSlot(slot) => {
            let prepared = world
                .resource_mut::<ResolutionWork>()
                .event_slots
                .remove(&slot)
                .ok_or(crate::resolver::ResolutionError::MissingEventSlot(slot))?;
            if let Some(event) = prepared.event {
                push_resolution_op(world, ResolutionOp::ResolveEvent(event));
            }
            Ok(())
        }
        ResolutionOp::AttemptTrigger(candidate) => attempt_trigger(world, operation_id, &candidate),
        ResolutionOp::FinishTrigger { attempt, source } => {
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::TriggerResolved {
                    id: attempt,
                    source,
                });
            Ok(())
        }
        ResolutionOp::RunEffect {
            context,
            effect,
            event,
        } => execute_effect_operation(world, &context, &effect, event),
        ResolutionOp::ProcessDamageBatch(requests) => {
            expand_damage_batch(world, requests);
            Ok(())
        }
        ResolutionOp::ProcessDamage {
            request,
            actual_event,
            ordinal,
        } => {
            process_damage(world, request, actual_event, ordinal);
            Ok(())
        }
        ResolutionOp::ApplyDamage {
            request,
            proposed_event,
            actual_event,
            ordinal,
        } => apply_prepared_damage(world, request, proposed_event, actual_event, ordinal),
        ResolutionOp::ProcessHealingBatch(requests) => {
            expand_healing_batch(world, requests);
            Ok(())
        }
        ResolutionOp::ProcessHealing {
            request,
            actual_event,
            ordinal,
        } => {
            process_healing(world, request, actual_event, ordinal);
            Ok(())
        }
        ResolutionOp::ApplyHealing {
            request,
            proposed_event,
            actual_event,
            ordinal,
        } => apply_prepared_healing(world, request, proposed_event, actual_event, ordinal),
        ResolutionOp::ProcessDraw(request) => process_draw(world, request),
        ResolutionOp::FinishDraw(result) => {
            crate::resolver::take_draw_result(world, result)?;
            Ok(())
        }
        ResolutionOp::ContinueDraw {
            result,
            mut context,
            effects,
            policy,
        } => {
            let outcome = crate::resolver::take_draw_result(world, result)?;
            match outcome {
                DrawOutcome::Drawn(card) => {
                    context.drawn_card = Some(card);
                    push_effects(world, &context, &effects, None);
                }
                DrawOutcome::Burned(_) | DrawOutcome::Fatigue { .. } => {
                    if policy == DrawContinuationPolicy::RunWithoutCard {
                        context.drawn_card = None;
                        push_effects(world, &context, &effects, None);
                    }
                }
            }
            Ok(())
        }
        ResolutionOp::TransformEntity {
            target,
            source,
            card,
            kind,
        } => {
            if kind == TransformKind::PlayedSelf
                && (source != Some(target) || !played_self_finish_barrier_remains(world, target))
            {
                return Err(SimulationError::InvalidTransformation(format!(
                    "played-self target {target:?} requires itself as source and a finish barrier"
                )));
            }
            transform_entity(world, target, card, kind)?;
            match kind {
                TransformKind::Spell => {}
                TransformKind::NonSpell => {
                    push_resolution_op(
                        world,
                        ResolutionOp::RefreshAuras(crate::AuraRefreshPlan::Summon),
                    );
                }
                TransformKind::PlayedSelf => {
                    world
                        .resource_mut::<ResolutionWork>()
                        .pending_played_self_transforms
                        .insert(target);
                }
            };
            Ok(())
        }
        ResolutionOp::FinishPlayedSelfTransform {
            subject,
            original_after_play,
        } => finish_played_self_transform(world, subject, original_after_play),
        ResolutionOp::CopyEntity(request) => copy_entity(world, request),
        ResolutionOp::RequestChoice(request) => {
            request_choice(world, request);
            Ok(())
        }
    }
}

pub(super) fn prepare_event(world: &mut World, context: EventContext) -> EventId {
    record_event(world, context, None)
}

pub(super) fn prepare_event_with_seeds(
    world: &mut World,
    context: EventContext,
    seeds: Vec<TriggerSeed>,
) -> EventId {
    record_event(world, context, Some(seeds))
}

fn prepare_prechecked_event(world: &mut World, context: EventContext) -> EventId {
    let prechecked_triggers = collect_trigger_seeds(world, &context);
    record_event(world, context, Some(prechecked_triggers))
}

fn record_event(
    world: &mut World,
    context: EventContext,
    prechecked_triggers: Option<Vec<crate::TriggerSeed>>,
) -> EventId {
    let event = crate::resolver::allocate_event_id(world);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::EventCreated {
            id: event,
            kind: context.kind,
            source: context.source,
            targets: context.targets.clone(),
            proposed: context.proposed_value,
            actual: context.actual_value,
        });
    let previous = world.resource_mut::<ResolutionWork>().events.insert(
        event,
        PreparedEvent {
            context,
            prechecked_triggers,
            candidates: None,
        },
    );
    debug_assert!(previous.is_none());
    event
}

fn played_self_finish_barrier_remains(world: &World, subject: GameEntityId) -> bool {
    world
        .resource::<ResolutionWork>()
        .stack
        .iter()
        .any(|stacked| {
            matches!(
                &stacked.operation,
                ResolutionOp::FinishPlayedSelfTransform {
                    subject: barrier_subject,
                    ..
                } if *barrier_subject == subject
            )
        })
}

fn finish_played_self_transform(
    world: &mut World,
    subject: GameEntityId,
    original_after_play: Vec<TriggerSeed>,
) -> Result<(), SimulationError> {
    if !world
        .resource_mut::<ResolutionWork>()
        .pending_played_self_transforms
        .remove(&subject)
    {
        return Ok(());
    }
    let entity = game_entity(world, subject).ok_or(SimulationError::EntityNotFound(subject))?;
    let controller = world
        .get::<Controller>(entity)
        .ok_or(SimulationError::EntityNotFound(subject))?
        .0;
    let context = |kind| EventContext {
        kind,
        source: Some(subject),
        targets: vec![subject],
        controller,
        proposed_value: None,
        actual_value: None,
        simultaneous_ordinal: 0,
    };
    let inserted_event = prepare_event(world, context(EventKind::AfterPlayAndSummon));
    let original_after_play_event =
        prepare_event_with_seeds(world, context(EventKind::AfterPlay), original_after_play);
    push_resolution_ops(
        world,
        [
            ResolutionOp::ResolveEvent(inserted_event),
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::ResolveEvent(original_after_play_event),
        ],
    );
    Ok(())
}

pub(super) fn take_prepared_event(
    world: &mut World,
    event: EventId,
) -> Result<PreparedEvent, SimulationError> {
    world
        .resource_mut::<ResolutionWork>()
        .events
        .remove(&event)
        .ok_or_else(|| crate::resolver::ResolutionError::MissingEvent(event).into())
}

fn expand_event(world: &mut World, event: EventId) -> Result<(), SimulationError> {
    let prepared = world
        .resource::<ResolutionWork>()
        .events
        .get(&event)
        .cloned()
        .ok_or(crate::resolver::ResolutionError::MissingEvent(event))?;
    let candidates = if let Some(candidates) = prepared.candidates {
        candidates
    } else {
        let seeds = prepared
            .prechecked_triggers
            .unwrap_or_else(|| collect_trigger_seeds(world, &prepared.context));
        let candidates = collect_trigger_candidates(world, event, &prepared.context, &seeds);
        world
            .resource_mut::<ResolutionWork>()
            .events
            .get_mut(&event)
            .expect("prepared event still exists")
            .candidates = Some(candidates.clone());
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::TriggerSnapshot {
                event,
                candidates: candidates.clone(),
            });
        candidates
    };
    let proposed = matches!(
        prepared.context.kind,
        EventKind::ProposedDamage | EventKind::ProposedHealing
    );
    let mut operations = candidates
        .into_iter()
        .map(ResolutionOp::AttemptTrigger)
        .collect::<Vec<_>>();
    if !proposed {
        operations.push(ResolutionOp::FinishEvent(event));
    }
    push_resolution_ops(world, operations);
    Ok(())
}

fn attempt_trigger(
    world: &mut World,
    attempt: crate::ResolutionId,
    candidate: &crate::TriggerCandidate,
) -> Result<(), SimulationError> {
    let event = world
        .resource::<ResolutionWork>()
        .events
        .get(&candidate.event)
        .map(|event| event.context.clone())
        .ok_or(crate::resolver::ResolutionError::MissingEvent(
            candidate.event,
        ))?;
    if !trigger_is_eligible(world, candidate, &event, ConditionTiming::ResolutionTime) {
        trace_trigger_aborted(world, attempt, candidate.source);
        return Ok(());
    }
    let context = EffectContext {
        source: game_entity(world, candidate.source).map(|_| candidate.source),
        controller: candidate.controller,
        declared_target: event.targets.first().copied(),
        drawn_card: None,
        origin: crate::EffectOrigin::Other,
    };
    push_resolution_op(
        world,
        ResolutionOp::FinishTrigger {
            attempt,
            source: candidate.source,
        },
    );
    push_effects(
        world,
        &context,
        &candidate.definition.effect_program,
        Some(candidate.event),
    );
    Ok(())
}

fn trace_trigger_aborted(
    world: &mut World,
    attempt: crate::ResolutionId,
    source: crate::GameEntityId,
) {
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TriggerAborted {
            id: attempt,
            source,
        });
}

fn run_phase_boundary(world: &mut World, _plan: PhaseBoundaryPlan) {
    world.run_schedule(ResolvePhaseBoundary);
    let deaths = take_pending_deaths(world);
    if deaths.is_empty() {
        return;
    }
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::DeathPhaseQueued {
            deaths: deaths.iter().map(|record| record.entity).collect(),
        });

    // Death Event records and pre-check-eligible trigger sources are frozen together. Each
    // event's queue-time conditions are evaluated only when that event begins resolution.
    let events = deaths
        .into_iter()
        .map(|record| prepare_death_event(world, &record))
        .collect::<Vec<_>>();
    let mut operations = events
        .into_iter()
        .map(ResolutionOp::ResolveEvent)
        .collect::<Vec<_>>();
    operations.push(ResolutionOp::RunPhaseBoundary(
        PhaseBoundaryPlan::ForcedDeath,
    ));
    push_resolution_ops(world, operations);
}

fn prepare_death_event(world: &mut World, record: &DeathRecord) -> EventId {
    prepare_prechecked_event(
        world,
        EventContext {
            kind: EventKind::Death,
            source: Some(record.entity),
            targets: vec![record.entity],
            controller: record.controller,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: record.simultaneous_ordinal,
        },
    )
}

fn request_choice(world: &mut World, request: ChoiceRequest) {
    let choice = request.id;
    let player = request.player;
    world.resource_mut::<ResolutionWork>().pending_choice = Some(PendingChoice { request });
    world.resource_mut::<GameState>().status = SimulationStatus::AwaitingChoice;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ChoiceRequested { choice, player });
}
