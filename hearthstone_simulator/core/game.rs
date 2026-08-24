use bevy::prelude::Resource;

use crate::PlayerId;

#[derive(Clone, Debug, Eq, PartialEq, Resource)]
pub struct GameState {
    pub active_player: PlayerId,
    pub turn_number: u32,
    pub winner: Option<PlayerId>,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            active_player: PlayerId::One,
            turn_number: 1,
            winner: None,
        }
    }
}
