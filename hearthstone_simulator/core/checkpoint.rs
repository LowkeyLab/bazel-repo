use std::collections::BTreeSet;

use crate::{
    Abilities, Armor, AttackAuraCache, AttackState, AuraDefinition, BaseKeywords, BaseStats,
    CanonicalTrace, ContinuousEffectDefinition, CurrentStats, Damage, DeathRecord, Effect,
    Enchantments, EntityKind, GameEntityId, GameState, HealthAuraCache, HeroMetadata,
    HeroPowerState, KeywordModifier, Keywords, OtherAuraCache, Player, PlayerId, ResolutionWork,
    RngSnapshot, Ruleset, SimulationError, StatModifier, TemporaryDuration, TriggerDefinition,
    TurnSchedule, Zone,
};

pub const CHECKPOINT_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CardRuntimeCheckpoint {
    pub cost: i32,
    pub program: Vec<Effect>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GameEntityCheckpoint {
    pub id: GameEntityId,
    pub definition_id: Option<String>,
    pub kind: Option<EntityKind>,
    pub controller: Option<PlayerId>,
    pub display_name: Option<String>,
    pub play_order: Option<u64>,
    pub base_stats: Option<BaseStats>,
    pub base_keywords: Option<BaseKeywords>,
    pub current_stats: Option<CurrentStats>,
    pub damage: Option<Damage>,
    pub armor: Option<Armor>,
    pub pending_destroy: bool,
    pub keywords: Option<Keywords>,
    pub abilities: Option<Abilities>,
    pub enchantments: Option<Enchantments>,
    pub attack_state: Option<AttackState>,
    pub hero_metadata: Option<HeroMetadata>,
    pub hero_power_state: Option<HeroPowerState>,
    pub player: Option<Player>,
    pub card_runtime: Option<CardRuntimeCheckpoint>,
    pub runtime_triggers: Option<Vec<TriggerDefinition>>,
    pub runtime_auras: Option<Vec<AuraDefinition>>,
    pub runtime_continuous_effects: Option<Vec<ContinuousEffectDefinition>>,
    pub silenced: bool,
    pub keep_enchantments: bool,
    pub silence_removable: bool,
    pub stat_modifier: Option<StatModifier>,
    pub keyword_modifier: Option<KeywordModifier>,
    pub temporary_duration: Option<TemporaryDuration>,
    pub health_aura_cache: Option<HealthAuraCache>,
    pub attack_aura_cache: Option<AttackAuraCache>,
    pub other_aura_cache: Option<OtherAuraCache>,
    pub attached_to: Option<GameEntityId>,
    pub death_record: Option<DeathRecord>,
    pub zone: Option<Zone>,
    pub zone_position: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SimulationCheckpoint {
    pub schema_version: u32,
    pub ruleset: Ruleset,
    pub game: GameState,
    pub turn_schedule: TurnSchedule,
    pub dominant_player: PlayerId,
    pub next_game_entity_id: u64,
    pub next_play_order: u64,
    pub rng: RngSnapshot,
    pub trace: CanonicalTrace,
    pub deaths: Vec<DeathRecord>,
    pub pending_deaths: Vec<DeathRecord>,
    pub defeated_heroes: BTreeSet<PlayerId>,
    pub resolution: ResolutionWork,
    pub entities: Vec<GameEntityCheckpoint>,
}

impl SimulationCheckpoint {
    /// Serializes this versioned checkpoint as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError::Checkpoint`] if JSON serialization fails.
    pub fn to_json(&self) -> Result<String, SimulationError> {
        serde_json::to_string(self).map_err(|error| SimulationError::Checkpoint(error.to_string()))
    }

    /// Deserializes a versioned checkpoint from JSON. World-level validation occurs on restore.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError::Checkpoint`] if the input is not a valid checkpoint document.
    pub fn from_json(json: &str) -> Result<Self, SimulationError> {
        serde_json::from_str(json).map_err(|error| SimulationError::Checkpoint(error.to_string()))
    }
}
