use bevy::prelude::Resource;

use crate::{
    AuraApplication, AuraCategory, CopyStatePolicy, DrawOutcome, EventId, EventKind,
    EventValueOperation, GameEntityId, PlayerId, ResolutionId, RulesetId, SequenceStep,
    TransformKind, TriggerCandidate, Zone,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TraceEntry {
    ActionAccepted {
        player: PlayerId,
        action: String,
    },
    ActionRejected {
        player: PlayerId,
        reason: String,
    },
    OperationPopped {
        id: ResolutionId,
        kind: String,
    },
    SequenceStepSkipped {
        step: SequenceStep,
        subject: GameEntityId,
        expected_zone: Zone,
        actual_zone: Option<Zone>,
    },
    EventCreated {
        id: EventId,
        kind: EventKind,
        source: Option<GameEntityId>,
        targets: Vec<GameEntityId>,
        proposed: Option<i32>,
        actual: Option<i32>,
    },
    TriggerSnapshot {
        event: EventId,
        candidates: Vec<TriggerCandidate>,
    },
    EventValueChanged {
        event: EventId,
        operation: EventValueOperation,
        previous: i32,
        current: i32,
    },
    TriggerAborted {
        id: ResolutionId,
        source: GameEntityId,
    },
    TriggerResolved {
        id: ResolutionId,
        source: GameEntityId,
    },
    ZoneMoved {
        entity: GameEntityId,
        from: Zone,
        to: Zone,
    },
    DrawResolved {
        player: PlayerId,
        source: Option<GameEntityId>,
        outcome: DrawOutcome,
    },
    EntityTransformed {
        entity: GameEntityId,
        previous_definition: String,
        replacement_definition: String,
        kind: TransformKind,
    },
    EntityCopied {
        source: GameEntityId,
        copy: GameEntityId,
        policy: CopyStatePolicy,
    },
    AuraUpdated {
        target: GameEntityId,
        category: AuraCategory,
        applications: Vec<AuraApplication>,
    },
    ResourceSpent {
        player: PlayerId,
        amount: i32,
    },
    Damage {
        source: Option<GameEntityId>,
        target: GameEntityId,
        proposed: i32,
        actual: i32,
    },
    Healing {
        source: Option<GameEntityId>,
        target: GameEntityId,
        proposed: i32,
        actual: i32,
    },
    EntityDied {
        entity: GameEntityId,
    },
    HeroDefeated {
        entity: GameEntityId,
        controller: PlayerId,
    },
    HeroReplaced {
        player: PlayerId,
        previous: GameEntityId,
        replacement: GameEntityId,
    },
    HeroPowerReplaced {
        player: PlayerId,
        previous: Option<GameEntityId>,
        replacement: GameEntityId,
    },
    DeathPhaseQueued {
        deaths: Vec<GameEntityId>,
    },
    ChoiceRequested {
        choice: crate::ChoiceId,
        player: PlayerId,
    },
    ChoiceAnswered {
        choice: crate::ChoiceId,
        option: crate::ChoiceId,
    },
    ExtraTurnsScheduled {
        player: PlayerId,
        count: u32,
        timing: crate::ExtraTurnTiming,
    },
    TemporaryEffectExpired {
        entity: GameEntityId,
    },
    TurnChanged {
        active_player: PlayerId,
        turn: u32,
        kind: crate::ScheduledTurnKind,
    },
    Outcome {
        winner: Option<PlayerId>,
    },
    RngChoice {
        position: u64,
        candidates: Vec<GameEntityId>,
        selected: GameEntityId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct CanonicalTrace {
    pub ruleset: RulesetId,
    pub entries: Vec<TraceEntry>,
}

impl Default for CanonicalTrace {
    fn default() -> Self {
        Self {
            ruleset: RulesetId::AdvancedRulebook2026_06_26,
            entries: Vec::new(),
        }
    }
}
