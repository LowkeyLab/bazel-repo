//! Deterministic, headless Hearthstone simulation engine built on Bevy ECS.

#![forbid(unsafe_code)]

mod aura;
mod death;
mod enchantment;
mod native_effect;
mod resolver;
mod rng;
mod simulation;
mod trigger;
mod zone;

mod entity {
    pub(crate) use hearthstone_simulator_core::{
        GameEntityIndex, NextGameEntityId, PlayOrderCounter, allocate_game_id, allocate_play_order,
        game_entity,
    };
}

pub(crate) use hearthstone_simulator_core::*;
pub use resolver::{CurrentResolutionOp, PhaseBoundarySet, ResolveFrame, ResolvePhaseBoundary};
pub use simulation::{HearthstoneSimulationPlugin, Simulation};
