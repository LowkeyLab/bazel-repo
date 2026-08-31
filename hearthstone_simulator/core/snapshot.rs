use crate::{
    DeathRecord, EntityKind, GameEntityId, GameState, HeroClass, PlayerId, ResolutionWork,
    RngSnapshot, RulesetId, TurnSchedule, Zone,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub entity: GameEntityId,
    pub id: PlayerId,
    pub name: String,
    pub hero: GameEntityId,
    pub hero_power: Option<GameEntityId>,
    pub hero_class: HeroClass,
    pub health: i32,
    pub armor: i32,
    pub available_resources: i32,
    pub maximum_resources: i32,
    pub used_resources: i32,
    pub temporary_resources: i32,
    pub pending_overload: i32,
    pub locked_overload: i32,
    pub resources_spent: i32,
    pub fatigue: u32,
    pub hand: Vec<GameEntityId>,
    pub deck: Vec<GameEntityId>,
    pub board: Vec<GameEntityId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameObjectSnapshot {
    pub id: GameEntityId,
    pub definition_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub controller: PlayerId,
    pub zone: Zone,
    pub zone_position: usize,
    pub play_order: u64,
    pub attack: Option<i32>,
    pub maximum_health: Option<i32>,
    pub damage: i32,
    pub exhausted: Option<bool>,
    pub hero_class: Option<HeroClass>,
    pub hero_power_exhausted: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSnapshot {
    pub ruleset: RulesetId,
    pub game: GameState,
    pub turn_schedule: TurnSchedule,
    pub dominant_player: PlayerId,
    pub players: Vec<PlayerSnapshot>,
    pub objects: Vec<GameObjectSnapshot>,
    pub deaths: Vec<DeathRecord>,
    pub rng: RngSnapshot,
    pub resolution: ResolutionWork,
}
