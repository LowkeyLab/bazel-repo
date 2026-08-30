//! Deterministic, headless Hearthstone rules simulation built on Bevy ECS.

#![forbid(unsafe_code)]

mod card_definition;
mod death;
mod effect;
mod enchantment;
mod entity;
mod event;
mod game;
mod ids;
mod model;
mod native_effect;
mod resolver;
mod rng;
mod ruleset;
mod simulation;
mod trace;
mod trigger;
mod zone;

pub use card_definition::{CardDefinition, CardDefinitions};
pub use death::{DeathEventCache, DeathRecord};
pub use effect::{
    Effect, EffectContext, EventValueOperation, PlayerSelector, Selector, ValueExpression,
};
pub use enchantment::{AttachedEnchantments, AttachedTo, AuraApplication, AuraCache, StatModifier};
pub use entity::{
    Abilities, Armor, AttackState, BaseStats, Controller, CurrentStats, Damage, DefinitionId,
    DisplayName, Enchantments, EntityKind, GameObject, Keyword, Keywords, PendingDestroy,
    PlayOrder, Player,
};
pub use event::{EventContext, EventKind};
pub use game::{DominantPlayer, GameOutcome, GameState, SimulationStatus};
pub use ids::{ChoiceId, EventId, EventSlotId, GameEntityId, PlayerId, ResolutionId};
pub use model::{Card, PlayerConfig, PlayerRef};
pub use native_effect::NativeEffectId;
pub use resolver::{
    ChoiceOption, ChoiceRequest, CurrentResolutionOp, DamageRequest, HealingRequest, PendingChoice,
    PhaseBoundaryPlan, PhaseBoundarySet, PreparedEvent, PreparedEventSlot, ResolutionError,
    ResolutionOp, ResolutionWork, ResolveFrame, ResolvePhaseBoundary, SequenceStep,
    StackedResolutionOp,
};
pub use rng::{DeterministicRng, RNG_ALGORITHM_VERSION, RngSnapshot};
pub use ruleset::{
    DEFAULT_RESOLUTION_BUDGET, MAX_BOARD_SIZE, MAX_HAND_SIZE, MAX_MANA, RULEBOOK_DATE,
    RULEBOOK_REVISION, Ruleset, RulesetId, STARTING_HEALTH,
};
pub use simulation::{
    CHECKPOINT_SCHEMA_VERSION, CardRuntimeCheckpoint, GameAction, GameEntityCheckpoint,
    GameObjectSnapshot, GameSnapshot, HearthstoneSimulationPlugin, PlayerSnapshot, Simulation,
    SimulationCheckpoint, SimulationError,
};
pub use trace::{CanonicalTrace, TraceEntry};
pub use trigger::{
    ConditionTiming, RuntimeTriggers, SourceEligibilityPolicy, TimedCondition, TriggerCandidate,
    TriggerCondition, TriggerDefinition, TriggerOrderKey, TriggerSeed, WoundedTargetPolicy,
};
pub use zone::{Zone, ZonePosition};
