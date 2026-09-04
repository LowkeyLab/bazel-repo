use thiserror::Error;

use crate::{GameEntityId, NativeEffectId, PlayerId, ResolutionError, Zone, ZoneError};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SimulationError {
    #[error("the game is already over")]
    GameOver,
    #[error("the simulation is not awaiting an action")]
    NotAwaitingAction,
    #[error("it is not {0:?}'s turn")]
    NotPlayersTurn(PlayerId),
    #[error("player {0:?} does not exist")]
    PlayerNotFound(PlayerId),
    #[error("game entity {0:?} does not exist")]
    EntityNotFound(GameEntityId),
    #[error("entity {entity:?} is controlled by another player")]
    NotControlled { entity: GameEntityId },
    #[error("entity {entity:?} is not in {expected:?}")]
    WrongZone {
        entity: GameEntityId,
        expected: Zone,
    },
    #[error("entity {0:?} is not a playable card")]
    NotPlayable(GameEntityId),
    #[error("invalid hero replacement: {0}")]
    InvalidHeroReplacement(String),
    #[error("invalid trigger enchantment: {0}")]
    InvalidTriggerEnchantment(String),
    #[error("player {player:?} needs {required} mana but only has {available}")]
    NotEnoughMana {
        player: PlayerId,
        required: i32,
        available: i32,
    },
    #[error("player {0:?}'s board is full")]
    BoardFull(PlayerId),
    #[error("attacker {0:?} cannot attack")]
    CannotAttack(GameEntityId),
    #[error("defender {0:?} is not a legal combat target")]
    InvalidDefender(GameEntityId),
    #[error("the simulation did not produce an action result")]
    MissingActionResult,
    #[error("resolution failed: {0}")]
    Resolution(#[from] ResolutionError),
    #[error("zone operation failed: {0}")]
    Zone(#[from] ZoneError),
    #[error("native effect {0:?} is already registered")]
    NativeEffectAlreadyRegistered(NativeEffectId),
    #[error("native effect {0:?} is not registered")]
    NativeEffectNotRegistered(NativeEffectId),
    #[error("native effect {id:?} failed: {reason}")]
    NativeEffectFailed { id: NativeEffectId, reason: String },
    #[error("an event-value modifier requires an active proposed damage or healing event")]
    NoModifiableEventValue,
    #[error("simulation checkpoint failed: {0}")]
    Checkpoint(String),
    #[error("simulation invariant failed: {0}")]
    Invariant(String),
}
