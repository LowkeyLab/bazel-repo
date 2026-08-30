use bevy::prelude::Component;

use crate::{GameEntityId, PlayerId};

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum EventKind {
    SequenceStarted,
    CardPlayed,
    Summoned,
    ProposedDamage,
    Damage,
    ProposedHealing,
    Healing,
    Attack,
    AfterAttack,
    Death,
    TurnStarted,
    TurnEnded,
}

#[derive(Component, Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EventContext {
    pub kind: EventKind,
    pub source: Option<GameEntityId>,
    pub targets: Vec<GameEntityId>,
    pub controller: PlayerId,
    pub proposed_value: Option<i32>,
    pub actual_value: Option<i32>,
    pub simultaneous_ordinal: u32,
}
