//! Deterministic, headless Hearthstone-style simulation primitives built on Bevy ECS.

#![forbid(unsafe_code)]

mod game;
mod model;
mod simulation;

pub use game::GameState;
pub use model::{
    Card, MAX_BOARD_SIZE, MAX_MANA, Minion, MinionCard, MinionId, Player, PlayerConfig, PlayerId,
    STARTING_HEALTH,
};
pub use simulation::{
    GameAction, GameSnapshot, HearthstoneSimulationPlugin, MinionSnapshot, PlayerSnapshot,
    Simulation, SimulationError,
};
