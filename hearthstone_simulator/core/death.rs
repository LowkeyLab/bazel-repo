use std::collections::BTreeSet;

use bevy::prelude::{Component, Resource};

use crate::{EntityKind, GameEntityId, PlayerId};

#[derive(Component, Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[component(immutable)]
pub struct DeathRecord {
    pub entity: GameEntityId,
    pub controller: PlayerId,
    pub kind: EntityKind,
    pub play_order: u64,
    pub remembered_zone_position: usize,
    pub simultaneous_ordinal: u32,
    pub turn_of_death: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct DeathEventCache {
    pub records: Vec<DeathRecord>,
}

#[derive(Default, Resource)]
#[doc(hidden)]
pub struct PendingDeaths(pub Vec<DeathRecord>);

#[derive(Default, Resource)]
#[doc(hidden)]
pub struct DefeatedHeroes(pub BTreeSet<PlayerId>);
