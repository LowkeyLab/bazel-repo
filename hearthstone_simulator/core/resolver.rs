use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::Resource;
use thiserror::Error;

use crate::{
    AuraRefreshPlan, Card, ChoiceId, CopyStatePolicy, DrawContinuationPolicy, DrawResultSlotId,
    Effect, EffectContext, EventContext, EventId, EventSlotId, GameEntityId, PlayerId,
    ResolutionId, ScheduledTurnKind, TransformKind, TriggerCandidate, TriggerSeed, Zone,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DrawRequest {
    pub player: PlayerId,
    pub source: Option<GameEntityId>,
    pub result: DrawResultSlotId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum DrawOutcome {
    Drawn(GameEntityId),
    Burned(GameEntityId),
    Fatigue { amount: i32 },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct DrawResultSlot {
    pub outcome: Option<DrawOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CopyRequest {
    pub source: GameEntityId,
    pub originating_source: Option<GameEntityId>,
    pub controller: PlayerId,
    pub destination: Zone,
    pub board_index: Option<usize>,
    pub policy: CopyStatePolicy,
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
    ProcessDraw(DrawRequest),
    FinishDraw(DrawResultSlotId),
    ContinueDraw {
        result: DrawResultSlotId,
        context: EffectContext,
        effects: Vec<Effect>,
        policy: DrawContinuationPolicy,
    },
    TransformEntity {
        target: GameEntityId,
        source: Option<GameEntityId>,
        card: Card,
        kind: TransformKind,
    },
    FinishPlayedSelfTransform {
        subject: GameEntityId,
        original_after_play: Vec<TriggerSeed>,
    },
    CopyEntity(CopyRequest),
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
            Self::ProcessDraw(_) => "ProcessDraw",
            Self::FinishDraw(_) => "FinishDraw",
            Self::ContinueDraw { .. } => "ContinueDraw",
            Self::TransformEntity { .. } => "TransformEntity",
            Self::FinishPlayedSelfTransform { .. } => "FinishPlayedSelfTransform",
            Self::CopyEntity(_) => "CopyEntity",
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
    pub next_draw_result_slot_id: u64,
    pub events: BTreeMap<EventId, PreparedEvent>,
    pub event_slots: BTreeMap<EventSlotId, PreparedEventSlot>,
    pub draw_result_slots: BTreeMap<DrawResultSlotId, DrawResultSlot>,
    pub pending_played_self_transforms: BTreeSet<GameEntityId>,
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
    #[error("draw result slot {0:?} does not exist")]
    MissingDrawResultSlot(DrawResultSlotId),
    #[error("draw result slot {0:?} is already filled")]
    DrawResultSlotAlreadyFilled(DrawResultSlotId),
    #[error("draw result slot {0:?} is empty")]
    EmptyDrawResultSlot(DrawResultSlotId),
    #[error("no player choice is pending")]
    NoPendingChoice,
    #[error("choice option {0:?} is invalid")]
    InvalidChoice(ChoiceId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Card, CopyRequest, CopyStatePolicy, DrawContinuationPolicy, DrawOutcome, DrawRequest,
        DrawResultSlot, DrawResultSlotId, EffectOrigin, TransformKind, Zone,
    };

    #[test]
    fn milestone_seven_operations_have_stable_kind_names() {
        let request = DrawRequest {
            player: PlayerId::One,
            source: Some(GameEntityId(7)),
            result: DrawResultSlotId(3),
        };
        assert_eq!(ResolutionOp::ProcessDraw(request).kind(), "ProcessDraw");
        assert_eq!(
            ResolutionOp::FinishDraw(DrawResultSlotId(3)).kind(),
            "FinishDraw"
        );
        assert_eq!(
            ResolutionOp::ContinueDraw {
                result: DrawResultSlotId(3),
                context: EffectContext {
                    source: Some(GameEntityId(7)),
                    controller: PlayerId::One,
                    declared_target: None,
                    drawn_card: None,
                    origin: EffectOrigin::Spell,
                },
                effects: Vec::new(),
                policy: DrawContinuationPolicy::RequireCard,
            }
            .kind(),
            "ContinueDraw"
        );
        assert_eq!(
            ResolutionOp::TransformEntity {
                target: GameEntityId(11),
                source: Some(GameEntityId(7)),
                card: Card::minion("Sheep", 1, 1, 1),
                kind: TransformKind::Spell,
            }
            .kind(),
            "TransformEntity"
        );
        assert_eq!(
            ResolutionOp::FinishPlayedSelfTransform {
                subject: GameEntityId(11),
                original_after_play: Vec::new(),
            }
            .kind(),
            "FinishPlayedSelfTransform"
        );
        assert_eq!(
            ResolutionOp::CopyEntity(CopyRequest {
                source: GameEntityId(11),
                originating_source: Some(GameEntityId(7)),
                controller: PlayerId::One,
                destination: Zone::Hand,
                board_index: None,
                policy: CopyStatePolicy::CurrentForm,
            })
            .kind(),
            "CopyEntity"
        );
    }

    #[test]
    fn resolution_work_round_trips_empty_and_filled_draw_slots() {
        let mut work = ResolutionWork::default();
        work.next_draw_result_slot_id = 2;
        work.draw_result_slots
            .insert(DrawResultSlotId(0), DrawResultSlot::default());
        work.draw_result_slots.insert(
            DrawResultSlotId(1),
            DrawResultSlot {
                outcome: Some(DrawOutcome::Drawn(GameEntityId(17))),
            },
        );

        let json = serde_json::to_string(&work).unwrap();
        let restored = serde_json::from_str::<ResolutionWork>(&json).unwrap();

        assert_eq!(restored, work);
    }

    #[test]
    fn copy_request_round_trips_its_originating_source() {
        let request = CopyRequest {
            source: GameEntityId(11),
            originating_source: Some(GameEntityId(7)),
            controller: PlayerId::One,
            destination: Zone::Play,
            board_index: Some(0),
            policy: CopyStatePolicy::InPlayState,
        };

        let json = serde_json::to_string(&request).unwrap();

        assert_eq!(serde_json::from_str::<CopyRequest>(&json).unwrap(), request);
    }
}
