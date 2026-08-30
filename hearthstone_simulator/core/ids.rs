use bevy::prelude::Component;

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum PlayerId {
    One,
    Two,
}

impl PlayerId {
    pub const ALL: [Self; 2] = [Self::One, Self::Two];

    pub const fn opponent(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::One,
        }
    }

    pub const fn bucket(self) -> u8 {
        match self {
            Self::One => 0,
            Self::Two => 1,
        }
    }
}

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
#[component(
    immutable,
    on_add = crate::entity::index_game_entity_hook,
    on_remove = crate::entity::unindex_game_entity_hook,
)]
#[require(crate::GameObject)]
pub struct GameEntityId(pub u64);

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct ResolutionId(pub u64);

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct EventId(pub u64);

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct EventSlotId(pub u64);

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct ChoiceId(pub u64);
