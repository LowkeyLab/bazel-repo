use bevy::prelude::Resource;

use crate::{GameEntityId, PlayerId, ResolutionId, RulesetId, Zone};

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
    QueueFrozen {
        queue: ResolutionId,
        entries: Vec<ResolutionId>,
    },
    TriggerAborted {
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
    EntityDied {
        entity: GameEntityId,
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
