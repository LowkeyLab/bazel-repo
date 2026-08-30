use bevy::prelude::Resource;

use crate::PlayerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct DominantPlayer(pub PlayerId);

impl Default for DominantPlayer {
    fn default() -> Self {
        Self(PlayerId::One)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum SimulationStatus {
    SettingUp,
    AwaitingAction,
    Resolving,
    AwaitingChoice,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum GameOutcome {
    Winner(PlayerId),
    Draw,
}

#[derive(Clone, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct GameState {
    pub active_player: PlayerId,
    pub turn_number: u32,
    pub outcome: Option<GameOutcome>,
    pub status: SimulationStatus,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            active_player: PlayerId::One,
            turn_number: 1,
            outcome: None,
            status: SimulationStatus::SettingUp,
        }
    }
}
