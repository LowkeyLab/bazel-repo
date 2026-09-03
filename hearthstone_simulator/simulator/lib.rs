//! Deterministic, headless Hearthstone simulation engine built on Bevy ECS.

#![forbid(unsafe_code)]

mod aura;
mod death;
mod enchantment;
mod native_effect;
mod resolver;
mod rng;
mod simulation;
mod trigger;
mod zone;

mod entity {
    pub(crate) use hearthstone_simulator_core::{
        GameEntityIndex, NextGameEntityId, PlayOrderCounter, allocate_game_id, allocate_play_order,
        game_entity,
    };
}

pub(crate) use hearthstone_simulator_core::{
    Abilities, Armor, AttachedEnchantments, AttachedTo, AttackAuraCache, AttackState,
    AuraApplication, AuraCategory, AuraModifier, AuraRefreshPlan, AuraTarget, BaseKeywords,
    BaseStats, CHECKPOINT_SCHEMA_VERSION, CanonicalTrace, Card, CardRuntimeCheckpoint, ChoiceId,
    ChoiceRequest, ContinuousEffectDefinition, ContinuousModifier, Controller, CostModifier,
    CurrentStats, Damage, DamageRequest, DeathEventCache, DeathRecord, DefinitionId,
    DeterministicRng, DisplayName, DominantPlayer, Effect, EffectContext, EffectOrigin,
    EnchantmentDuration, Enchantments, EntityKind, EventContext, EventId, EventKind, EventSlotId,
    EventValueOperation, GameAction, GameEntityCheckpoint, GameEntityId, GameObject,
    GameObjectSnapshot, GameOutcome, GameSnapshot, GameState, HealingRequest, HealthAuraCache,
    HeroClass, HeroClassPolicy, HeroHealthPolicy, HeroMetadata, HeroPowerState, HeroReplacement,
    KeepEnchantments, Keyword, KeywordModifier, Keywords, NativeEffectId, OtherAuraCache,
    OtherAuraModifier, PendingChoice, PendingDestroy, PhaseBoundaryPlan, PlayOrder, Player,
    PlayerAudience, PlayerConfig, PlayerId, PlayerSelector, PlayerSnapshot, PreparedEvent,
    PreparedEventSlot, RULEBOOK_REVISION, ResolutionId, ResolutionOp, ResolutionWork, Ruleset,
    RuntimeAuras, RuntimeContinuousEffects, RuntimeTriggers, STARTING_HEALTH, ScheduledTurnKind,
    Selector, SequenceStep, SilenceRemovable, Silenced, SimulationCheckpoint, SimulationError,
    SimulationStatus, SourceEligibilityPolicy, StackedResolutionOp, StatModifier, TraceEntry,
    TriggerCandidate, TriggerCondition, TriggerOrderKey, TriggerSeed, TurnSchedule,
    ValueExpression, Zone, ZoneMoveOutcome, ZoneMoveRequest, ZoneMovementKind, ZonePosition,
};

#[cfg(test)]
pub(crate) use hearthstone_simulator_core::{
    AuraDefinition, ChoiceOption, ConditionTiming, CostOperation, ExtraTurnTiming,
    RNG_ALGORITHM_VERSION, ResolutionError, RngSnapshot, TimedCondition, TriggerDefinition,
    WoundedTargetPolicy,
};

pub use resolver::{CurrentResolutionOp, PhaseBoundarySet, ResolveFrame, ResolvePhaseBoundary};
pub use simulation::{HearthstoneSimulationPlugin, Simulation};
