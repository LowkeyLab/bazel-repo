use bevy::prelude::Resource;

use crate::{Zone, ZoneMovementKind};

pub const RULEBOOK_REVISION: u64 = 913_067;
pub const RULEBOOK_DATE: &str = "2026-06-26";
pub const MAX_BOARD_SIZE: usize = 7;
pub const MAX_HAND_SIZE: usize = 10;
pub const MAX_DECK_SIZE: usize = 99;
pub const MAX_SECRET_ZONE_SIZE: usize = 5;
pub const MAX_MANA: i32 = 10;
pub const STARTING_HEALTH: i32 = 30;
pub const DEFAULT_RESOLUTION_BUDGET: usize = 100_000;

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum RulesetId {
    AdvancedRulebook2026_06_26,
}

#[derive(Clone, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct Ruleset {
    pub id: RulesetId,
    pub rulebook_revision: u64,
    pub board_limit: usize,
    pub hand_limit: usize,
    pub deck_limit: usize,
    pub secret_limit: usize,
    pub hero_limit: usize,
    pub weapon_limit: usize,
    pub hero_power_limit: usize,
    pub quest_limit: usize,
    pub maximum_mana: i32,
    pub resolution_budget: usize,
}

impl Ruleset {
    #[must_use]
    pub const fn resets_runtime_state(
        &self,
        kind: ZoneMovementKind,
        source: Zone,
        destination: Zone,
    ) -> bool {
        if matches!(kind, ZoneMovementKind::Death) {
            return true;
        }
        match (zone_rank(source), zone_rank(destination)) {
            (Some(source), Some(destination)) => destination < source,
            _ => false,
        }
    }
}

const fn zone_rank(zone: Zone) -> Option<u8> {
    match zone {
        Zone::Deck => Some(0),
        Zone::Hand => Some(1),
        Zone::Play => Some(2),
        Zone::Graveyard => Some(3),
        Zone::Secret | Zone::SetAside | Zone::RemovedFromGame => None,
    }
}

impl Default for Ruleset {
    fn default() -> Self {
        Self {
            id: RulesetId::AdvancedRulebook2026_06_26,
            rulebook_revision: RULEBOOK_REVISION,
            board_limit: MAX_BOARD_SIZE,
            hand_limit: MAX_HAND_SIZE,
            deck_limit: MAX_DECK_SIZE,
            secret_limit: MAX_SECRET_ZONE_SIZE,
            hero_limit: 1,
            weapon_limit: 1,
            hero_power_limit: 1,
            quest_limit: 1,
            maximum_mana: MAX_MANA,
            resolution_budget: DEFAULT_RESOLUTION_BUDGET,
        }
    }
}
