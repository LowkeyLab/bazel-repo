use bevy::{
    ecs::{
        entity::MapEntities,
        schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel},
    },
    prelude::*,
};
use thiserror::Error;

use crate::{CanonicalTrace, NestedUnder, ResolutionId, Ruleset, TraceEntry, death::create_deaths};

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
    QueueDeathPhase,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResolutionNode;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResolutionKind {
    Sequence,
    Phase,
    EventBatch,
    Event,
    EventQueue,
    TriggerQueue,
    Trigger,
    Effect,
    PhaseBoundary,
    DeathPhase,
    Choice,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[component(immutable)]
pub struct ResolutionIdentity {
    pub id: ResolutionId,
    pub kind: ResolutionKind,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionProgress {
    Ready,
    Running,
    Suspended,
    Complete,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolutionState {
    pub progress: ResolutionProgress,
}

#[derive(Resource, Clone, Debug, Default, Eq, PartialEq, MapEntities)]
pub struct ResolutionCursor {
    #[entities]
    pub root: Option<Entity>,
    #[entities]
    pub active: Option<Entity>,
    pub remaining_budget: usize,
}

#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct NextResolutionId(pub u64);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ResolutionError {
    #[error("resolution budget exhausted at {active:?}")]
    BudgetExhausted { active: Option<ResolutionId> },
    #[error("active resolution cursor does not point to a live node")]
    InvalidCursor,
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
                PhaseBoundarySet::QueueDeathPhase,
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

pub(crate) fn begin_resolution(world: &mut World, kind: ResolutionKind) -> Entity {
    let id = allocate_resolution_id(world);
    let entity = world
        .spawn((
            ResolutionNode,
            ResolutionIdentity { id, kind },
            ResolutionState {
                progress: ResolutionProgress::Ready,
            },
        ))
        .id();
    let budget = world.resource::<Ruleset>().resolution_budget;
    *world.resource_mut::<ResolutionCursor>() = ResolutionCursor {
        root: Some(entity),
        active: Some(entity),
        remaining_budget: budget,
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::FrameBegin {
            id,
            kind: format!("{kind:?}"),
        });
    entity
}

pub(crate) fn push_resolution(
    world: &mut World,
    kind: ResolutionKind,
) -> Result<Entity, ResolutionError> {
    let parent = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let child = spawn_resolution_child(world, parent, kind);
    activate_resolution_child(world, child)?;
    Ok(child)
}

pub(crate) fn spawn_resolution_child(
    world: &mut World,
    parent: Entity,
    kind: ResolutionKind,
) -> Entity {
    let id = allocate_resolution_id(world);
    world
        .spawn((
            ResolutionNode,
            ResolutionIdentity { id, kind },
            ResolutionState {
                progress: ResolutionProgress::Ready,
            },
            NestedUnder(parent),
        ))
        .id()
}

pub(crate) fn activate_resolution_child(
    world: &mut World,
    child: Entity,
) -> Result<(), ResolutionError> {
    let parent = world
        .get::<NestedUnder>(child)
        .map(|parent| parent.0)
        .ok_or(ResolutionError::InvalidCursor)?;
    if world.resource::<ResolutionCursor>().active != Some(parent) {
        return Err(ResolutionError::InvalidCursor);
    }
    let identity = *world
        .get::<ResolutionIdentity>(child)
        .ok_or(ResolutionError::InvalidCursor)?;
    world.resource_mut::<ResolutionCursor>().active = Some(child);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::FrameBegin {
            id: identity.id,
            kind: format!("{:?}", identity.kind),
        });
    Ok(())
}

#[allow(dead_code, reason = "used by the upcoming suspended-choice driver")]
pub(crate) fn suspend_active(world: &mut World) -> Result<(), ResolutionError> {
    let active = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    world.entity_mut(active).insert(ResolutionState {
        progress: ResolutionProgress::Suspended,
    });
    Ok(())
}

#[allow(dead_code, reason = "used by the upcoming suspended-choice driver")]
pub(crate) fn resume_active(world: &mut World) -> Result<(), ResolutionError> {
    let active = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    if world
        .get::<ResolutionState>(active)
        .map(|state| state.progress)
        != Some(ResolutionProgress::Suspended)
    {
        return Err(ResolutionError::InvalidCursor);
    }
    world.entity_mut(active).insert(ResolutionState {
        progress: ResolutionProgress::Running,
    });
    Ok(())
}

pub(crate) fn complete_active(world: &mut World) -> Result<(), ResolutionError> {
    let active = world
        .resource::<ResolutionCursor>()
        .active
        .ok_or(ResolutionError::InvalidCursor)?;
    let identity = *world
        .get::<ResolutionIdentity>(active)
        .ok_or(ResolutionError::InvalidCursor)?;
    let parent = world.get::<NestedUnder>(active).map(|parent| parent.0);
    world.entity_mut(active).insert(ResolutionState {
        progress: ResolutionProgress::Complete,
    });
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::FrameEnd {
            id: identity.id,
            kind: format!("{:?}", identity.kind),
        });
    world.resource_mut::<ResolutionCursor>().active = parent;
    Ok(())
}

pub(crate) fn consume_budget(world: &mut World) -> Result<(), ResolutionError> {
    let active_entity = {
        let mut cursor = world.resource_mut::<ResolutionCursor>();
        if cursor.remaining_budget > 0 {
            cursor.remaining_budget -= 1;
            return Ok(());
        }
        cursor.active
    };
    let active = active_entity.and_then(|entity| {
        world
            .get::<ResolutionIdentity>(entity)
            .map(|identity| identity.id)
    });
    Err(ResolutionError::BudgetExhausted { active })
}

pub(crate) fn cleanup_resolution(world: &mut World) {
    let root = world.resource::<ResolutionCursor>().root;
    if let Some(root) = root {
        let _ = world.despawn(root);
    }
    *world.resource_mut::<ResolutionCursor>() = ResolutionCursor::default();
}

pub(crate) fn assert_resolution_invariants(world: &World) -> Result<(), String> {
    let cursor = world.resource::<ResolutionCursor>();
    let (Some(root), Some(active)) = (cursor.root, cursor.active) else {
        if cursor.root.is_none() && cursor.active.is_none() {
            return Ok(());
        }
        return Err("resolution root and active cursor disagree".to_string());
    };
    if !world.entity(root).contains::<ResolutionNode>()
        || !world.entity(active).contains::<ResolutionNode>()
    {
        return Err("resolution cursor references a non-resolution entity".to_string());
    }
    let mut current = active;
    while let Some(parent) = world.get::<NestedUnder>(current).map(|parent| parent.0) {
        current = parent;
    }
    if current != root {
        return Err("active resolution node is outside the root ancestry".to_string());
    }
    Ok(())
}

pub(crate) fn allocate_resolution_id(world: &mut World) -> ResolutionId {
    let mut next = world.resource_mut::<NextResolutionId>();
    let id = ResolutionId(next.0);
    next.0 = next.0.checked_add(1).expect("resolution ID overflow");
    id
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn app_with_resolution() -> App {
        let mut app = App::new();
        app.init_resource::<Ruleset>()
            .init_resource::<CanonicalTrace>()
            .init_resource::<ResolutionCursor>()
            .init_resource::<NextResolutionId>();
        configure_resolution(&mut app);
        app
    }

    #[googletest::test]
    fn active_cursor_follows_depth_first_relationship_path() {
        let mut app = app_with_resolution();
        let world = app.world_mut();
        let root = begin_resolution(world, ResolutionKind::Sequence);
        let phase = push_resolution(world, ResolutionKind::Phase).expect("phase should push");
        let effect = push_resolution(world, ResolutionKind::Effect).expect("effect should push");

        assert_that!(
            world.resource::<ResolutionCursor>().active,
            eq(Some(effect))
        );
        assert_that!(world.get::<NestedUnder>(effect).unwrap().0, eq(phase));
        assert_that!(world.get::<NestedUnder>(phase).unwrap().0, eq(root));
        assert_resolution_invariants(world).expect("active leaf should belong to root");
        suspend_active(world).expect("active frame should suspend");
        assert_that!(
            world.get::<ResolutionState>(effect).unwrap().progress,
            eq(ResolutionProgress::Suspended)
        );
        resume_active(world).expect("active frame should resume");

        complete_active(world).expect("effect should complete");
        assert_that!(world.resource::<ResolutionCursor>().active, eq(Some(phase)));
        complete_active(world).expect("phase should complete");
        complete_active(world).expect("root should complete");
        cleanup_resolution(world);

        assert_that!(
            *world.resource::<ResolutionCursor>(),
            eq(&ResolutionCursor::default())
        );
        assert_that!(
            world
                .iter_entities()
                .filter(|entity| entity.contains::<ResolutionNode>())
                .count(),
            eq(0)
        );
    }

    #[googletest::test]
    fn resolution_budget_reports_the_active_logical_id() {
        let mut app = app_with_resolution();
        app.world_mut().resource_mut::<Ruleset>().resolution_budget = 1;
        let world = app.world_mut();
        begin_resolution(world, ResolutionKind::Sequence);

        consume_budget(world).expect("first step fits budget");
        assert_that!(
            consume_budget(world),
            err(eq(&ResolutionError::BudgetExhausted {
                active: Some(ResolutionId(0)),
            }))
        );
    }

    #[googletest::test]
    fn invalid_cursor_operations_and_invariants_are_reported() {
        let mut app = app_with_resolution();
        let world = app.world_mut();

        assert_that!(
            push_resolution(world, ResolutionKind::Effect),
            err(eq(&ResolutionError::InvalidCursor))
        );
        assert_that!(
            suspend_active(world),
            err(eq(&ResolutionError::InvalidCursor))
        );
        assert_that!(
            resume_active(world),
            err(eq(&ResolutionError::InvalidCursor))
        );
        assert_that!(
            complete_active(world),
            err(eq(&ResolutionError::InvalidCursor))
        );

        let root = begin_resolution(world, ResolutionKind::Sequence);
        assert_that!(
            resume_active(world),
            err(eq(&ResolutionError::InvalidCursor))
        );
        let active_child = spawn_resolution_child(world, root, ResolutionKind::Effect);
        let inactive_sibling = spawn_resolution_child(world, root, ResolutionKind::Effect);
        activate_resolution_child(world, active_child).expect("child should activate");
        assert_that!(
            activate_resolution_child(world, inactive_sibling),
            err(eq(&ResolutionError::InvalidCursor))
        );
        complete_active(world).expect("active child should complete");
        let malformed = world.spawn_empty().id();
        world.resource_mut::<ResolutionCursor>().active = Some(malformed);
        assert_that!(
            complete_active(world),
            err(eq(&ResolutionError::InvalidCursor))
        );
        world.resource_mut::<ResolutionCursor>().remaining_budget = 0;
        assert_that!(
            consume_budget(world),
            err(eq(&ResolutionError::BudgetExhausted { active: None }))
        );

        world.resource_mut::<ResolutionCursor>().active = None;
        assert_that!(
            assert_resolution_invariants(world),
            err(eq(&"resolution root and active cursor disagree".to_string()))
        );
        world.resource_mut::<ResolutionCursor>().active = Some(malformed);
        assert_that!(
            assert_resolution_invariants(world),
            err(eq(
                &"resolution cursor references a non-resolution entity".to_string()
            ))
        );
        world.entity_mut(malformed).insert(ResolutionNode);
        assert_that!(
            assert_resolution_invariants(world),
            err(eq(
                &"active resolution node is outside the root ancestry".to_string()
            ))
        );
        world.entity_mut(malformed).insert(NestedUnder(root));
        assert_resolution_invariants(world).unwrap();
    }
}
