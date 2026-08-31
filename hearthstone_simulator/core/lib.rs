//! Core data models for the deterministic Hearthstone simulator.

#![forbid(unsafe_code)]

mod action;
mod aura;
mod card_definition;
mod checkpoint;
mod death;
mod effect;
mod enchantment;
mod entity;
mod error;
mod event;
mod game;
mod ids;
mod model;
mod native_effect;
mod resolver;
mod rng;
mod ruleset;
mod snapshot;
mod trace;
mod trigger;
mod zone;

pub use action::GameAction;
pub use aura::{
    AttackAuraCache, AuraApplication, AuraCategory, AuraDefinition, AuraModifier, AuraRefreshPlan,
    AuraTarget, ContinuousEffectDefinition, ContinuousModifier, HealthAuraCache, OtherAuraCache,
    OtherAuraModifier, PlayerAudience, RuntimeAuras, RuntimeContinuousEffects, SilenceRemovable,
};
pub use card_definition::{CardDefinition, CardDefinitions};
pub use checkpoint::{
    CHECKPOINT_SCHEMA_VERSION, CardRuntimeCheckpoint, GameEntityCheckpoint, SimulationCheckpoint,
};
pub use death::{DeathEventCache, DeathRecord, DefeatedHeroes, PendingDeaths};
pub use effect::{
    Effect, EffectContext, EffectOrigin, EventValueOperation, HeroClassPolicy, HeroHealthPolicy,
    HeroReplacement, PlayerSelector, Selector, ValueExpression,
};
pub use enchantment::{
    AttachedEnchantments, AttachedTo, KeywordModifier, StatModifier, TemporaryDuration,
};
pub use entity::{
    Abilities, Armor, AttackState, BaseKeywords, BaseStats, Controller, CurrentStats, Damage,
    DefinitionId, DisplayName, Enchantments, EntityKind, GameEntityIndex, GameObject, HeroClass,
    HeroMetadata, HeroPowerState, KeepEnchantments, Keyword, Keywords, NextGameEntityId,
    PendingDestroy, PlayOrder, PlayOrderCounter, Player, Silenced,
};
#[doc(hidden)]
pub use entity::{allocate_game_id, allocate_play_order, game_entity};
pub use error::SimulationError;
pub use event::{EventContext, EventKind};
pub use game::{
    DominantPlayer, ExtraTurnTiming, GameOutcome, GameState, ScheduledTurn, ScheduledTurnKind,
    SimulationStatus, TurnSchedule,
};
pub use ids::{ChoiceId, EventId, EventSlotId, GameEntityId, PlayerId, ResolutionId};
pub use model::{Card, PlayerConfig, PlayerRef};
pub use native_effect::NativeEffectId;
pub use resolver::{
    ChoiceOption, ChoiceRequest, DamageRequest, HealingRequest, PendingChoice, PhaseBoundaryPlan,
    PreparedEvent, PreparedEventSlot, ResolutionError, ResolutionOp, ResolutionWork, SequenceStep,
    StackedResolutionOp,
};
pub use rng::{DeterministicRng, RNG_ALGORITHM_VERSION, RngSnapshot};
pub use ruleset::{
    DEFAULT_RESOLUTION_BUDGET, MAX_BOARD_SIZE, MAX_DECK_SIZE, MAX_HAND_SIZE, MAX_MANA,
    MAX_SECRET_ZONE_SIZE, RULEBOOK_DATE, RULEBOOK_REVISION, Ruleset, RulesetId, STARTING_HEALTH,
};
pub use snapshot::{GameObjectSnapshot, GameSnapshot, PlayerSnapshot};
pub use trace::{CanonicalTrace, TraceEntry};
pub use trigger::{
    ConditionTiming, RuntimeTriggers, SourceEligibilityPolicy, TimedCondition, TriggerCandidate,
    TriggerCondition, TriggerDefinition, TriggerOrderKey, TriggerSeed, WoundedTargetPolicy,
};
pub use zone::{
    Zone, ZoneError, ZoneIndex, ZoneMoveOutcome, ZoneMoveRequest, ZoneMovementKind, ZonePosition,
};
