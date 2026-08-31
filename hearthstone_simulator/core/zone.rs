use std::collections::BTreeMap;

use bevy::prelude::{Component, Resource};
use thiserror::Error;

use crate::{GameEntityId, PlayerId};

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
)]
pub enum Zone {
    Deck,
    Hand,
    Play,
    Secret,
    Graveyard,
    SetAside,
    RemovedFromGame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ZoneMovementKind {
    Normal,
    Draw,
    ForcePlay,
    Death,
    Discard,
    DetachEnchantment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ZoneMoveRequest {
    pub entity: GameEntityId,
    pub destination_controller: PlayerId,
    pub destination: Zone,
    pub position: Option<usize>,
    pub kind: ZoneMovementKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneMoveOutcome {
    Moved {
        from: Zone,
        from_controller: PlayerId,
        from_position: usize,
    },
    PreventedByFullZone,
    FullZoneRemoval {
        from: Zone,
        from_controller: PlayerId,
        from_position: usize,
    },
}

#[derive(
    Component, Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize,
)]
pub struct ZonePosition(pub usize);

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource)]
pub struct ZoneIndex(pub BTreeMap<(PlayerId, Zone), Vec<GameEntityId>>);

impl ZoneIndex {
    pub fn entities(&self, player: PlayerId, zone: Zone) -> &[GameEntityId] {
        self.0
            .get(&(player, zone))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ZoneError {
    #[error("game entity {0:?} does not exist")]
    EntityNotFound(GameEntityId),
    #[error("game entity {entity:?} is absent from authoritative {zone:?} index")]
    MissingIndexEntry { entity: GameEntityId, zone: Zone },
    #[error("{zone:?} is full for player {player:?}")]
    Full { player: PlayerId, zone: Zone },
    #[error("position {position} is outside {zone:?} of length {length}")]
    InvalidPosition {
        zone: Zone,
        position: usize,
        length: usize,
    },
}
