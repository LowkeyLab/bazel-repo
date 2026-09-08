use bevy::{
    ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
};

pub(crate) use hearthstone_simulator_core::ResolutionError;

use crate::{
    DrawOutcome, DrawResultSlot, DrawResultSlotId, EventId, EventSlotId, PreparedEventSlot,
    ResolutionId, ResolutionOp, ResolutionWork, Ruleset, StackedResolutionOp,
    aura::{refresh_health_attack_auras, refresh_post_death_auras},
    death::create_deaths,
};

#[cfg(test)]
use crate::PhaseBoundaryPlan;

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

#[derive(Clone, Debug, Default, Resource)]
pub struct CurrentResolutionOp(pub Option<StackedResolutionOp>);

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
            (
                refresh_health_attack_auras.in_set(PhaseBoundarySet::HealthAttackAuras),
                refresh_health_attack_auras.in_set(PhaseBoundarySet::RefreshHealthAttackAuras),
                create_deaths.in_set(PhaseBoundarySet::CreateDeaths),
                refresh_post_death_auras.in_set(PhaseBoundarySet::OtherAuras),
            ),
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
        || !work.draw_result_slots.is_empty()
        || !work.pending_played_self_transforms.is_empty()
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
    work.draw_result_slots.clear();
    work.pending_played_self_transforms.clear();
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

pub(crate) fn allocate_draw_result_slot(world: &mut World) -> DrawResultSlotId {
    let mut work = world.resource_mut::<ResolutionWork>();
    let id = DrawResultSlotId(work.next_draw_result_slot_id);
    work.next_draw_result_slot_id = work
        .next_draw_result_slot_id
        .checked_add(1)
        .expect("draw result slot ID overflow");
    let previous = work.draw_result_slots.insert(id, DrawResultSlot::default());
    debug_assert!(previous.is_none());
    id
}

pub(crate) fn fill_draw_result_slot(
    world: &mut World,
    slot: DrawResultSlotId,
    outcome: DrawOutcome,
) -> Result<(), ResolutionError> {
    let mut work = world.resource_mut::<ResolutionWork>();
    let result = work
        .draw_result_slots
        .get_mut(&slot)
        .ok_or(ResolutionError::MissingDrawResultSlot(slot))?;
    if result.outcome.is_some() {
        return Err(ResolutionError::DrawResultSlotAlreadyFilled(slot));
    }
    result.outcome = Some(outcome);
    Ok(())
}

pub(crate) fn take_draw_result(
    world: &mut World,
    slot: DrawResultSlotId,
) -> Result<DrawOutcome, ResolutionError> {
    world
        .resource_mut::<ResolutionWork>()
        .draw_result_slots
        .remove(&slot)
        .ok_or(ResolutionError::MissingDrawResultSlot(slot))?
        .outcome
        .ok_or(ResolutionError::EmptyDrawResultSlot(slot))
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
            || !work.draw_result_slots.is_empty()
            || !work.pending_played_self_transforms.is_empty()
            || work.pending_choice.is_some()
        {
            return Err("idle resolution work retains operations, events, event slots, draw result slots, transform timing markers, or a choice".into());
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
    use crate::GameEntityId;

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
    fn draw_result_slot_allocation_increments_counter_and_inserts_empty_slot() {
        let mut world = world();

        let first = allocate_draw_result_slot(&mut world);
        let second = allocate_draw_result_slot(&mut world);

        assert_that!(first, eq(DrawResultSlotId(0)));
        assert_that!(second, eq(DrawResultSlotId(1)));
        let work = world.resource::<ResolutionWork>();
        assert_that!(work.next_draw_result_slot_id, eq(2));
        assert_that!(work.draw_result_slots.len(), eq(2));
        assert_that!(work.draw_result_slots[&first].outcome, none());
        assert_that!(work.draw_result_slots[&second].outcome, none());
    }

    #[googletest::test]
    fn draw_result_slots_fill_and_consume_exactly_once() {
        let mut world = world();
        let slot = allocate_draw_result_slot(&mut world);
        fill_draw_result_slot(&mut world, slot, DrawOutcome::Drawn(GameEntityId(9))).unwrap();

        assert_that!(
            fill_draw_result_slot(&mut world, slot, DrawOutcome::Fatigue { amount: 1 }),
            err(eq(&ResolutionError::DrawResultSlotAlreadyFilled(slot)))
        );
        assert_that!(
            take_draw_result(&mut world, slot),
            ok(eq(&DrawOutcome::Drawn(GameEntityId(9))))
        );
        assert_that!(
            take_draw_result(&mut world, slot),
            err(eq(&ResolutionError::MissingDrawResultSlot(slot)))
        );
    }

    #[googletest::test]
    fn draw_result_slots_reject_missing_fill_and_empty_take() {
        let mut world = world();
        let missing = DrawResultSlotId(42);
        assert_that!(
            fill_draw_result_slot(&mut world, missing, DrawOutcome::Fatigue { amount: 1 }),
            err(eq(&ResolutionError::MissingDrawResultSlot(missing)))
        );

        let empty = allocate_draw_result_slot(&mut world);
        assert_that!(
            take_draw_result(&mut world, empty),
            err(eq(&ResolutionError::EmptyDrawResultSlot(empty)))
        );
        assert_that!(
            take_draw_result(&mut world, empty),
            err(eq(&ResolutionError::MissingDrawResultSlot(empty)))
        );
    }

    #[googletest::test]
    fn sequence_start_rejects_draw_slots_and_transform_timing_markers() {
        let mut world = world();
        let slot = allocate_draw_result_slot(&mut world);
        assert_that!(
            begin_sequence(&mut world),
            err(eq(&ResolutionError::AlreadyResolving))
        );

        world
            .resource_mut::<ResolutionWork>()
            .draw_result_slots
            .remove(&slot);
        world
            .resource_mut::<ResolutionWork>()
            .pending_played_self_transforms
            .insert(GameEntityId(7));
        assert_that!(
            begin_sequence(&mut world),
            err(eq(&ResolutionError::AlreadyResolving))
        );
    }

    #[googletest::test]
    fn abandon_sequence_clears_draw_slots_and_transform_timing_markers() {
        let mut world = world();
        let slot = allocate_draw_result_slot(&mut world);
        fill_draw_result_slot(&mut world, slot, DrawOutcome::Fatigue { amount: 1 }).unwrap();
        world
            .resource_mut::<ResolutionWork>()
            .pending_played_self_transforms
            .insert(GameEntityId(7));

        abandon_sequence(&mut world);

        let work = world.resource::<ResolutionWork>();
        assert_that!(work.draw_result_slots, is_empty());
        assert_that!(work.pending_played_self_transforms, is_empty());
    }

    #[googletest::test]
    fn idle_invariants_reject_draw_slots_and_transform_timing_markers() {
        let mut world = world();
        let slot = allocate_draw_result_slot(&mut world);
        let idle_error = "idle resolution work retains operations, events, event slots, draw result slots, transform timing markers, or a choice".to_string();
        assert_that!(assert_resolution_invariants(&world), err(eq(&idle_error)));

        world
            .resource_mut::<ResolutionWork>()
            .draw_result_slots
            .remove(&slot);
        world
            .resource_mut::<ResolutionWork>()
            .pending_played_self_transforms
            .insert(GameEntityId(7));
        assert_that!(assert_resolution_invariants(&world), err(eq(&idle_error)));
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
                &"idle resolution work retains operations, events, event slots, draw result slots, transform timing markers, or a choice".to_string()
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
