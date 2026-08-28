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
mod queue;
mod relationships;
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
    Abilities, Armor, AttackState, BaseStats, Charge, ComputedStats, Controller, CurrentStats,
    Damage, DefinitionId, DisplayName, DivineShield, Enchantments, EntityKind, GameObject,
    HeroForm, Immune, Keyword, Lifesteal, MinionForm, PendingDestroy, PlayOrder, Player, Poisonous,
    Rush, StatBearing, Stealth, Taunt, Windfury,
};
pub use event::{EventContext, EventKind};
pub use game::{GameOutcome, GameState, SimulationStatus};
pub use ids::{ChoiceId, GameEntityId, PlayerId, ResolutionId};
pub use model::{Card, PlayerConfig, PlayerRef};
pub use native_effect::NativeEffectId;
pub use queue::{
    EventOrderKey, FrozenQueueEntries, QueueCursor, QueueEntryStatus, QueueKind,
    QueueMutationError, QueueState, QueuedEvent, QueuedTrigger, ResolutionQueue, TriggerOrderKey,
};
pub use relationships::{NestedFrames, NestedUnder, QueueEntries, QueuedIn};
pub use resolver::{
    PhaseBoundarySet, ResolutionCursor, ResolutionIdentity, ResolutionKind, ResolutionNode,
    ResolutionProgress, ResolutionState, ResolveFrame, ResolvePhaseBoundary,
};
pub use rng::{DeterministicRng, RNG_ALGORITHM_VERSION, RngSnapshot};
pub use ruleset::{
    DEFAULT_RESOLUTION_BUDGET, MAX_BOARD_SIZE, MAX_HAND_SIZE, MAX_MANA, RULEBOOK_DATE,
    RULEBOOK_REVISION, Ruleset, RulesetId, STARTING_HEALTH,
};
pub use simulation::{
    GameAction, GameObjectSnapshot, GameSnapshot, HearthstoneSimulationPlugin, PlayerSnapshot,
    Simulation, SimulationError,
};
pub use trace::{CanonicalTrace, TraceEntry};
pub use trigger::{
    ConditionTiming, RuntimeTriggers, SourceEligibilityPolicy, TimedCondition, TriggerCondition,
    TriggerDefinition, TriggerExecution, WoundedTargetPolicy,
};
pub use zone::{Zone, ZonePosition};
