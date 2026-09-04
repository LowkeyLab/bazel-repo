use bevy::prelude::Component;

use crate::{Effect, EventId, EventKind, GameEntityId, PlayerId, PlayerSelector, Selector, Zone};

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum ConditionTiming {
    PreCheck,
    QueueTime,
    ResolutionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TriggerCondition {
    Always,
    SourceInPlay,
    SourceInZone(Zone),
    EventValueAtLeast(i32),
    EventSourceIsSelf,
    EventTargetsSelf,
    EventTargetsAttachedEntity,
    EventControllerIs(PlayerSelector),
    ControllerIs(PlayerId),
    MinimumEntityCount { selector: Selector, count: usize },
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum SourceEligibilityPolicy {
    MustExist,
    MustRemainInEligibleZone,
    RememberedSource,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum WoundedTargetPolicy {
    ExcludeMortallyWounded,
    IncludeMortallyWounded,
    IncludePendingDestroy,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TimedCondition {
    pub timing: ConditionTiming,
    pub condition: TriggerCondition,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerDefinition {
    pub event: EventKind,
    pub eligible_zones: Vec<Zone>,
    pub conditions: Vec<TimedCondition>,
    pub source_eligibility: SourceEligibilityPolicy,
    pub priority: i16,
    pub wounded_target_policy: WoundedTargetPolicy,
    pub effect_program: Vec<Effect>,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeTriggers(pub Vec<TriggerDefinition>);

#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct TriggerOrderKey {
    pub player_bucket: u8,
    pub zone_bucket: u8,
    pub priority: i16,
    pub play_order: u64,
    pub source: GameEntityId,
    pub tie_breaker: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerSeed {
    pub source: GameEntityId,
    pub definition_index: u32,
    pub definition: TriggerDefinition,
    pub controller: PlayerId,
    pub zone: Zone,
    pub play_order: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TriggerCandidate {
    pub source: GameEntityId,
    pub event: EventId,
    pub definition_index: u32,
    pub definition: TriggerDefinition,
    pub controller: PlayerId,
    pub order: TriggerOrderKey,
}
