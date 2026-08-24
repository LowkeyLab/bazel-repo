use bevy::prelude::Component;

pub const STARTING_HEALTH: i32 = 30;
pub const MAX_MANA: u8 = 10;
pub const MAX_BOARD_SIZE: usize = 7;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlayerId {
    One,
    Two,
}

impl PlayerId {
    pub const fn opponent(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::One,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Card {
    Minion(MinionCard),
}

impl Card {
    pub fn minion(name: impl Into<String>, mana_cost: u8, attack: i32, health: i32) -> Self {
        Self::Minion(MinionCard {
            name: name.into(),
            mana_cost,
            attack,
            health,
        })
    }

    pub const fn mana_cost(&self) -> u8 {
        match self {
            Self::Minion(card) => card.mana_cost,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinionCard {
    pub name: String,
    pub mana_cost: u8,
    pub attack: i32,
    pub health: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerConfig {
    pub name: String,
    pub hand: Vec<Card>,
}

impl PlayerConfig {
    pub fn new(name: impl Into<String>, hand: Vec<Card>) -> Self {
        Self {
            name: name.into(),
            hand,
        }
    }
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub health: i32,
    pub mana: u8,
    pub max_mana: u8,
    pub hand: Vec<Card>,
}

impl Player {
    pub(crate) fn from_config(id: PlayerId, config: PlayerConfig, starts: bool) -> Self {
        let max_mana = u8::from(starts);
        Self {
            id,
            name: config.name,
            health: STARTING_HEALTH,
            mana: max_mana,
            max_mana,
            hand: config.hand,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MinionId(pub u32);

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct Minion {
    pub id: MinionId,
    pub owner: PlayerId,
    pub name: String,
    pub attack: i32,
    pub health: i32,
    pub can_attack: bool,
}
