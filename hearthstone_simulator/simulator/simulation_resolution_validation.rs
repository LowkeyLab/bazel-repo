use std::collections::BTreeSet;

use bevy::prelude::World;

use crate::{EventKind, ResolutionOp, ResolutionWork, SimulationError};

use super::effect_executor::{
    validate_copy_request, validate_effect_program, validate_play_effect_program,
    validate_transform_request,
};

pub(super) fn validate_resolution_operation(
    world: &World,
    operation: &crate::ResolutionOp,
) -> Result<(), SimulationError> {
    match operation {
        crate::ResolutionOp::RunEffect { effect, event, .. } => {
            let event = event.and_then(|event| {
                world
                    .resource::<ResolutionWork>()
                    .events
                    .get(&event)
                    .map(|prepared| prepared.context.kind)
            });
            validate_effect_program(world, std::slice::from_ref(effect), event)
        }
        crate::ResolutionOp::RunPlayEffect {
            scope,
            context,
            effect,
        } => {
            let subject = context.source.ok_or_else(|| {
                SimulationError::InvalidTransformation("play effect requires its source".into())
            })?;
            crate::resolver::validate_play_scope(world, *scope, subject)?;
            validate_play_effect_program(world, std::slice::from_ref(effect))
        }
        crate::ResolutionOp::ContinueDraw { effects, .. } => {
            validate_effect_program(world, effects, None)
        }
        crate::ResolutionOp::AttemptTrigger(candidate) => validate_effect_program(
            world,
            &candidate.definition.effect_program,
            Some(candidate.definition.event),
        ),
        crate::ResolutionOp::FinishPlayedSelfTransform {
            original_after_play,
            ..
        } => {
            for seed in original_after_play {
                validate_effect_program(
                    world,
                    &seed.definition.effect_program,
                    Some(EventKind::AfterPlay),
                )?;
            }
            Ok(())
        }
        crate::ResolutionOp::TransformEntity {
            target,
            source,
            card,
            kind,
            play_scope,
        } => {
            if *kind == crate::TransformKind::PlayedSelf {
                let scope = play_scope
                    .filter(|_| *source == Some(*target))
                    .ok_or_else(|| {
                        SimulationError::InvalidTransformation(
                            "played-self transformation requires its own explicit play scope"
                                .into(),
                        )
                    })?;
                crate::resolver::validate_play_scope(world, scope, *target)?;
            } else if play_scope.is_some() {
                return Err(SimulationError::InvalidTransformation(
                    "ordinary transformation cannot carry a play scope".into(),
                ));
            }
            validate_transform_request(world, *target, card)
        }
        crate::ResolutionOp::CopyEntity(request) => validate_copy_request(world, request),
        crate::ResolutionOp::RequestChoice(request) => validate_choice_request(world, request),
        _ => Ok(()),
    }
}

pub(super) fn validate_choice_request(
    world: &World,
    request: &crate::ChoiceRequest,
) -> Result<(), SimulationError> {
    let mut options = BTreeSet::new();
    if request.options.is_empty()
        || request
            .options
            .iter()
            .any(|option| !options.insert(option.id))
    {
        return Err(SimulationError::Invariant(
            "choice requires nonempty, unique options".into(),
        ));
    }
    for option in &request.options {
        for operation in &option.operations {
            match operation {
                ResolutionOp::RunEffect { event: Some(_), .. } => {
                    return Err(SimulationError::Invariant(
                        "choice branches cannot borrow an event context".into(),
                    ));
                }
                ResolutionOp::RunPlayEffect { .. }
                | ResolutionOp::FinishPlayedSelfTransform { .. }
                | ResolutionOp::TransformEntity {
                    play_scope: Some(_),
                    ..
                } => {
                    return Err(SimulationError::InvalidTransformation(
                        "choice branches cannot borrow a play scope".into(),
                    ));
                }
                _ => {}
            }
            validate_resolution_operation(world, operation)?;
        }
    }
    Ok(())
}
