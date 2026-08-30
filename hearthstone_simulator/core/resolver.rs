use std::collections::BTreeMap;

use bevy::{
    ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
};
use thiserror::Error;

use crate::{
    ChoiceId, Effect, EffectContext, EventContext, EventId, EventSlotId, GameEntityId, PlayerId,
    ResolutionId, Ruleset, TriggerCandidate, death::create_deaths,
};

#[derive(ScheduleLabel, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolveFrame;

#[derive(ScheduleLabel, Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolvePhaseBoundary;

#[derive(SystemSet, Clone, Debug, Eq, Hash, PartialEq)]
pub enum PhaseBoundarySet {
    HealthAttackAuras,
    QuestRewards,
    SummonResolution,
    RefreshHealthAttackAuras,
    CreateDeaths,
    OtherAuras,
    CompileDeathPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PhaseBoundaryPlan {
    Ordinary,
    ForcedDeath,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum SequenceStep {
    PlayCard {
        player: PlayerId,
        card: GameEntityId,
        target: Option<GameEntityId>,
        board_index: Option<usize>,
    },
    Attack {
        player: PlayerId,
        attacker: GameEntityId,
        defender: GameEntityId,
    },
    FinishAttack {
        player: PlayerId,
        attacker: GameEntityId,
        defender: GameEntityId,
    },
    EndTurn {
        player: PlayerId,
    },
    StartTurn {
        player: PlayerId,
    },
    Concede {
        player: PlayerId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DamageRequest {
    pub source: Option<GameEntityId>,
    pub target: GameEntityId,
    pub proposed: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct HealingRequest {
    pub source: Option<GameEntityId>,
    pub target: GameEntityId,
    pub proposed: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ChoiceOption {
    pub id: ChoiceId,
    pub operations: Vec<ResolutionOp>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ChoiceRequest {
    pub id: ChoiceId,
    pub player: PlayerId,
    pub options: Vec<ChoiceOption>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PendingChoice {
    pub request: ChoiceRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ResolutionOp {
    RunSequenceStep(SequenceStep),
    RunPhaseBoundary(PhaseBoundaryPlan),
    CheckOutcome,
    PrepareEvent(EventContext),
    ResolveEvent(EventId),
    FinishEvent(EventId),
    ResolveEventSlot(EventSlotId),
    AttemptTrigger(TriggerCandidate),
    FinishTrigger {
        attempt: ResolutionId,
        source: GameEntityId,
    },
    RunEffect {
        context: EffectContext,
        effect: Effect,
        event: Option<EventId>,
    },
    ProcessDamageBatch(Vec<DamageRequest>),
    ProcessDamage {
        request: DamageRequest,
        actual_event: EventSlotId,
        ordinal: u32,
    },
    ApplyDamage {
        request: DamageRequest,
        proposed_event: EventId,
        actual_event: EventSlotId,
        ordinal: u32,
    },
    ProcessHealingBatch(Vec<HealingRequest>),
    ProcessHealing {
        request: HealingRequest,
        actual_event: EventSlotId,
        ordinal: u32,
    },
    ApplyHealing {
        request: HealingRequest,
        proposed_event: EventId,
        actual_event: EventSlotId,
        ordinal: u32,
    },
    RequestChoice(ChoiceRequest),
}

impl ResolutionOp {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunSequenceStep(_) => "RunSequenceStep",
            Self::RunPhaseBoundary(_) => "RunPhaseBoundary",
            Self::CheckOutcome => "CheckOutcome",
            Self::PrepareEvent(_) => "PrepareEvent",
            Self::ResolveEvent(_) => "ResolveEvent",
            Self::FinishEvent(_) => "FinishEvent",
            Self::ResolveEventSlot(_) => "ResolveEventSlot",
            Self::AttemptTrigger(_) => "AttemptTrigger",
            Self::FinishTrigger { .. } => "FinishTrigger",
            Self::RunEffect { .. } => "RunEffect",
            Self::ProcessDamageBatch(_) => "ProcessDamageBatch",
            Self::ProcessDamage { .. } => "ProcessDamage",
            Self::ApplyDamage { .. } => "ApplyDamage",
            Self::ProcessHealingBatch(_) => "ProcessHealingBatch",
            Self::ProcessHealing { .. } => "ProcessHealing",
            Self::ApplyHealing { .. } => "ApplyHealing",
            Self::RequestChoice(_) => "RequestChoice",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StackedResolutionOp {
    pub id: ResolutionId,
    pub operation: ResolutionOp,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PreparedEventSlot {
    pub event: Option<EventId>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PreparedEvent {
    pub context: EventContext,
    pub prechecked_triggers: Option<Vec<crate::TriggerSeed>>,
    pub candidates: Option<Vec<TriggerCandidate>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct ResolutionWork {
    pub stack: Vec<StackedResolutionOp>,
    pub remaining_budget: usize,
    pub next_resolution_id: u64,
    pub next_event_id: u64,
    pub next_event_slot_id: u64,
    pub events: BTreeMap<EventId, PreparedEvent>,
    pub event_slots: BTreeMap<EventSlotId, PreparedEventSlot>,
    pub pending_choice: Option<PendingChoice>,
    pub sequence_active: bool,
}

#[derive(Clone, Debug, Default, Resource)]
pub struct CurrentResolutionOp(pub Option<StackedResolutionOp>);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ResolutionError {
    #[error("resolution budget exhausted while executing {operation:?}")]
    BudgetExhausted { operation: Option<ResolutionId> },
    #[error("resolution work already exists")]
    AlreadyResolving,
    #[error("prepared event {0:?} does not exist")]
    MissingEvent(EventId),
    #[error("prepared event slot {0:?} does not exist")]
    MissingEventSlot(EventSlotId),
    #[error("no player choice is pending")]
    NoPendingChoice,
    #[error("choice option {0:?} is invalid")]
    InvalidChoice(ChoiceId),
}

pub(crate) fn configure_resolution(app: &mut App) {
    app.init_schedule(ResolveFrame)
        .init_schedule(ResolvePhaseBoundary)
        .configure_sets(
            ResolvePhaseBoundary,
            (
                PhaseBoundarySet::HealthAttackAuras,
                PhaseBoundarySet::QuestRewards,
                PhaseBoundarySet::SummonResolution,
                PhaseBoundarySet::RefreshHealthAttackAuras,
                PhaseBoundarySet::CreateDeaths,
                PhaseBoundarySet::OtherAuras,
                PhaseBoundarySet::CompileDeathPhase,
            )
                .chain(),
        )
        .add_systems(
            ResolvePhaseBoundary,
            create_deaths.in_set(PhaseBoundarySet::CreateDeaths),
        );
    let settings = ScheduleBuildSettings {
        ambiguity_detection: LogLevel::Error,
        hierarchy_detection: LogLevel::Error,
        auto_insert_apply_deferred: false,
        ..default()
    };
    app.edit_schedule(ResolveFrame, |schedule| {
        schedule
            .set_build_settings(settings.clone())
            .set_apply_final_deferred(false);
    });
    app.edit_schedule(ResolvePhaseBoundary, |schedule| {
        schedule
            .set_build_settings(settings.clone())
            .set_apply_final_deferred(false);
    });
}

pub(crate) fn begin_sequence(world: &mut World) -> Result<(), ResolutionError> {
    let budget = world.resource::<Ruleset>().resolution_budget;
    let mut work = world.resource_mut::<ResolutionWork>();
    if work.sequence_active
        || !work.stack.is_empty()
        || !work.events.is_empty()
        || !work.event_slots.is_empty()
        || work.pending_choice.is_some()
    {
        return Err(ResolutionError::AlreadyResolving);
    }
    work.remaining_budget = budget;
    work.sequence_active = true;
    Ok(())
}

pub(crate) fn finish_sequence(world: &mut World) {
    world.resource_mut::<ResolutionWork>().sequence_active = false;
}

pub(crate) fn abandon_sequence(world: &mut World) {
    let mut work = world.resource_mut::<ResolutionWork>();
    work.stack.clear();
    work.events.clear();
    work.event_slots.clear();
    work.pending_choice = None;
    work.sequence_active = false;
    work.remaining_budget = 0;
}

pub(crate) fn push_resolution_op(world: &mut World, operation: ResolutionOp) -> ResolutionId {
    let mut work = world.resource_mut::<ResolutionWork>();
    let id = ResolutionId(work.next_resolution_id);
    work.next_resolution_id = work
        .next_resolution_id
        .checked_add(1)
        .expect("resolution ID overflow");
    work.stack.push(StackedResolutionOp { id, operation });
    id
}

pub(crate) fn push_resolution_ops(
    world: &mut World,
    operations_in_execution_order: impl IntoIterator<Item = ResolutionOp>,
) {
    let operations = operations_in_execution_order
        .into_iter()
        .collect::<Vec<_>>();
    for operation in operations.into_iter().rev() {
        push_resolution_op(world, operation);
    }
}

pub(crate) fn pop_resolution_op(world: &mut World) -> Option<StackedResolutionOp> {
    world.resource_mut::<ResolutionWork>().stack.pop()
}

pub(crate) fn consume_budget(
    world: &mut World,
    operation: ResolutionId,
) -> Result<(), ResolutionError> {
    let mut work = world.resource_mut::<ResolutionWork>();
    if work.remaining_budget == 0 {
        return Err(ResolutionError::BudgetExhausted {
            operation: Some(operation),
        });
    }
    work.remaining_budget -= 1;
    Ok(())
}

pub(crate) fn allocate_event_id(world: &mut World) -> EventId {
    let mut work = world.resource_mut::<ResolutionWork>();
    let id = EventId(work.next_event_id);
    work.next_event_id = work
        .next_event_id
        .checked_add(1)
        .expect("event ID overflow");
    id
}

pub(crate) fn allocate_event_slot(world: &mut World) -> EventSlotId {
    let mut work = world.resource_mut::<ResolutionWork>();
    let id = EventSlotId(work.next_event_slot_id);
    work.next_event_slot_id = work
        .next_event_slot_id
        .checked_add(1)
        .expect("event slot ID overflow");
    let previous = work.event_slots.insert(id, PreparedEventSlot::default());
    debug_assert!(previous.is_none());
    id
}

pub(crate) fn resolution_is_active(world: &World) -> bool {
    world.resource::<ResolutionWork>().sequence_active
}

pub(crate) fn assert_resolution_invariants(world: &World) -> Result<(), String> {
    let work = world.resource::<ResolutionWork>();
    if !work.sequence_active {
        if !work.stack.is_empty()
            || !work.events.is_empty()
            || !work.event_slots.is_empty()
            || work.pending_choice.is_some()
        {
            return Err(
                "idle resolution work retains operations, events, slots, or a choice".into(),
            );
        }
    } else if world.resource::<crate::GameState>().status == crate::SimulationStatus::AwaitingChoice
        && work.pending_choice.is_none()
    {
        return Err("AwaitingChoice has no pending choice".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Ruleset>();
        world.init_resource::<ResolutionWork>();
        world.init_resource::<crate::GameState>();
        world
    }

    #[googletest::test]
    fn reverse_push_produces_one_shot_lifo_execution_order() {
        let mut world = world();
        begin_sequence(&mut world).unwrap();
        push_resolution_ops(
            &mut world,
            [
                ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
                ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::ForcedDeath),
            ],
        );

        assert_that!(
            pop_resolution_op(&mut world).unwrap().operation,
            eq(&ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary))
        );
        assert_that!(
            pop_resolution_op(&mut world).unwrap().operation,
            eq(&ResolutionOp::RunPhaseBoundary(
                PhaseBoundaryPlan::ForcedDeath
            ))
        );
        assert_that!(pop_resolution_op(&mut world), none());
    }

    #[googletest::test]
    fn budget_reports_the_exact_popped_operation() {
        let mut world = world();
        world.resource_mut::<Ruleset>().resolution_budget = 1;
        begin_sequence(&mut world).unwrap();
        let first = push_resolution_op(
            &mut world,
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
        );
        consume_budget(&mut world, first).unwrap();
        let second = push_resolution_op(
            &mut world,
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
        );

        assert_that!(
            consume_budget(&mut world, second),
            err(eq(&ResolutionError::BudgetExhausted {
                operation: Some(second)
            }))
        );
    }

    #[googletest::test]
    fn sequence_start_and_resolution_invariants_reject_inconsistent_work() {
        let mut world = world();
        begin_sequence(&mut world).unwrap();
        assert_that!(
            begin_sequence(&mut world),
            err(eq(&ResolutionError::AlreadyResolving))
        );

        abandon_sequence(&mut world);
        push_resolution_op(&mut world, ResolutionOp::CheckOutcome);
        assert_that!(
            assert_resolution_invariants(&world),
            err(eq(
                &"idle resolution work retains operations, events, slots, or a choice".to_string()
            ))
        );

        world.resource_mut::<ResolutionWork>().stack.clear();
        world.resource_mut::<ResolutionWork>().sequence_active = true;
        world.resource_mut::<crate::GameState>().status = crate::SimulationStatus::AwaitingChoice;
        assert_that!(
            assert_resolution_invariants(&world),
            err(eq(&"AwaitingChoice has no pending choice".to_string()))
        );
    }
}
