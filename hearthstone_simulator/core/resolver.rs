use std::collections::BTreeMap;

use bevy::prelude::Resource;
use thiserror::Error;

use crate::{
    AuraRefreshPlan, ChoiceId, Effect, EffectContext, EventContext, EventId, EventSlotId,
    GameEntityId, PlayerId, ResolutionId, ScheduledTurnKind, TriggerCandidate,
};

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
    AdvanceTurn {
        ending_player: PlayerId,
    },
    StartTurn {
        player: PlayerId,
        kind: ScheduledTurnKind,
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
    RefreshAuras(AuraRefreshPlan),
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
            Self::RefreshAuras(_) => "RefreshAuras",
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
