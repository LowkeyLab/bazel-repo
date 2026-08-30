use bevy::prelude::Resource;

pub const RULEBOOK_REVISION: u64 = 913_067;
pub const RULEBOOK_DATE: &str = "2026-06-26";
pub const MAX_BOARD_SIZE: usize = 7;
pub const MAX_HAND_SIZE: usize = 10;
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
    pub maximum_mana: i32,
    pub resolution_budget: usize,
}

impl Default for Ruleset {
    fn default() -> Self {
        Self {
            id: RulesetId::AdvancedRulebook2026_06_26,
            rulebook_revision: RULEBOOK_REVISION,
            board_limit: MAX_BOARD_SIZE,
            hand_limit: MAX_HAND_SIZE,
            maximum_mana: MAX_MANA,
            resolution_budget: DEFAULT_RESOLUTION_BUDGET,
        }
    }
}
