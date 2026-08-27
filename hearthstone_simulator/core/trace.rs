use bevy::prelude::Resource;

use crate::{
    EventKind, EventValueOperation, GameEntityId, PlayerId, ResolutionId, RulesetId, Zone,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TraceEntry {
    ActionAccepted {
        player: PlayerId,
        action: String,
    },
    ActionRejected {
        player: PlayerId,
        reason: String,
    },
    FrameBegin {
        id: ResolutionId,
        kind: String,
    },
    FrameEnd {
        id: ResolutionId,
        kind: String,
    },
    EventCreated {
        id: ResolutionId,
        kind: EventKind,
        source: Option<GameEntityId>,
        targets: Vec<GameEntityId>,
        proposed: Option<i32>,
        actual: Option<i32>,
    },
    QueueFrozen {
        queue: ResolutionId,
        entries: Vec<ResolutionId>,
    },
    EventValueChanged {
        event: ResolutionId,
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
    DeathPhaseQueued {
        deaths: Vec<GameEntityId>,
    },
    TurnChanged {
        active_player: PlayerId,
        turn: u32,
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

#[derive(Clone, Debug, Eq, PartialEq, Resource)]
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
