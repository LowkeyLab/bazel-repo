use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{
    CanonicalTrace, CurrentResolutionOp, DeathEventCache, DeterministicRng, DominantPlayer, Effect,
    EffectContext, GameState, NativeEffectId, PlayerConfig, ResolutionWork, Ruleset,
    SimulationStatus, TraceEntry,
    death::{DefeatedHeroes, PendingDeaths},
    entity::{GameEntityIndex, NextGameEntityId, PlayOrderCounter},
    native_effect::{NativeEffectFactory, NativeEffectRegistry},
    resolver::{assert_resolution_invariants, configure_resolution},
    zone::{ZoneIndex, assert_zone_invariants},
};

#[cfg(test)]
use crate::{
    Armor, Card, ChoiceId, ChoiceOption, ChoiceRequest, Damage, EntityKind, EventContext,
    EventKind, EventValueOperation, GameEntityId, GameObject, GameOutcome, HealingRequest, Keyword,
    Keywords, PlayerId, PlayerSelector, ResolutionError, ResolutionOp, RuntimeTriggers,
    STARTING_HEALTH, Selector, ValueExpression, Zone,
    enchantment::StatModifier,
    entity::game_entity,
    resolver::{begin_sequence, finish_sequence, push_resolution_ops},
    trigger::TriggersSuppressed,
    zone::ZoneError,
};

#[path = "simulation_action.rs"]
mod action;
#[path = "simulation_card_runtime.rs"]
mod card_runtime;
#[path = "simulation_checkpoint.rs"]
mod checkpoint;
#[path = "simulation_effect_executor.rs"]
mod effect_executor;
#[path = "simulation_error.rs"]
mod error;
#[path = "simulation_event_resolver.rs"]
mod event_resolver;
#[path = "simulation_health.rs"]
mod health;
#[path = "simulation_player.rs"]
mod player;
#[path = "simulation_snapshot.rs"]
mod snapshot;

pub use action::GameAction;
pub use checkpoint::{
    CHECKPOINT_SCHEMA_VERSION, CardRuntimeCheckpoint, GameEntityCheckpoint, SimulationCheckpoint,
};
pub use error::SimulationError;
pub use snapshot::{GameObjectSnapshot, GameSnapshot, PlayerSnapshot};

#[cfg(test)]
use action::drive_resolution;
use action::{configure_actions, legal_actions, submit_action, submit_choice};
use card_runtime::setup_game;
use checkpoint::{build_checkpoint, restore_checkpoint};
use snapshot::{assert_game_entity_index, build_snapshot};

#[cfg(test)]
use card_runtime::spawn_card;
#[cfg(test)]
use effect_executor::{
    attach_stat_modifier, copy_card_data, evaluate_value, execute_effect, execute_effect_operation,
    execute_effects, modify_active_event_value, modify_event_value, resolve_player,
    select_entities, silence_entity, transform_entity,
};
#[cfg(test)]
use event_resolver::{prepare_event, take_prepared_event};
#[cfg(test)]
use health::{SimultaneousEventOrder, apply_damage, apply_damage_batch, apply_healing_batch};
#[cfg(test)]
use player::{check_outcome, draw_card, hero_id, player};

pub struct HearthstoneSimulationPlugin;

impl Plugin for HearthstoneSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
            .init_resource::<DominantPlayer>()
            .init_resource::<Ruleset>()
            .init_resource::<GameEntityIndex>()
            .init_resource::<NextGameEntityId>()
            .init_resource::<PlayOrderCounter>()
            .init_resource::<ZoneIndex>()
            .init_resource::<CanonicalTrace>()
            .init_resource::<DeathEventCache>()
            .init_resource::<PendingDeaths>()
            .init_resource::<DefeatedHeroes>()
            .init_resource::<DeterministicRng>()
            .init_resource::<ResolutionWork>()
            .init_resource::<CurrentResolutionOp>()
            .init_resource::<NativeEffectRegistry>()
            .init_resource::<event_resolver::OperationFailure>();
        configure_resolution(app);
        app.add_systems(
            crate::ResolveFrame,
            event_resolver::execute_current_resolution_op,
        );
        configure_actions(app);
    }
}

pub struct Simulation {
    app: App,
    native_effect_factories: BTreeMap<NativeEffectId, NativeEffectFactory>,
}

impl Simulation {
    pub fn new(players: [PlayerConfig; 2]) -> Self {
        Self::with_seed(players, 0)
    }

    #[must_use]
    pub fn with_seed(players: [PlayerConfig; 2], seed: u64) -> Self {
        Self::with_seed_and_dominant_player(players, seed, crate::PlayerId::One)
    }

    /// Creates a deterministic simulation with explicit dominant-player identity.
    ///
    /// # Panics
    ///
    /// Panics if the supplied fixture cannot be inserted into the configured starting zones.
    #[must_use]
    pub fn with_seed_and_dominant_player(
        players: [PlayerConfig; 2],
        seed: u64,
        dominant_player: crate::PlayerId,
    ) -> Self {
        let mut app = App::new();
        app.add_plugins(HearthstoneSimulationPlugin);
        app.world_mut()
            .insert_resource(DominantPlayer(dominant_player));
        app.world_mut().insert_resource(DeterministicRng::new(seed));
        setup_game(app.world_mut(), players).expect("valid player fixture should initialize");
        app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
        Self {
            app,
            native_effect_factories: BTreeMap::new(),
        }
    }

    pub fn register_native_effect<M>(
        &mut self,
        id: impl Into<NativeEffectId>,
        handler: impl IntoSystem<In<EffectContext>, Vec<Effect>, M> + Clone + Send + Sync + 'static,
    ) -> Result<(), SimulationError>
    where
        M: 'static,
    {
        let id = id.into();
        let world = self.app.world_mut();
        if world.resource::<NativeEffectRegistry>().0.contains_key(&id) {
            return Err(SimulationError::NativeEffectAlreadyRegistered(id));
        }
        let factory: NativeEffectFactory =
            std::sync::Arc::new(move |world| world.register_system(handler.clone()));
        let system = factory(world);
        world
            .resource_mut::<NativeEffectRegistry>()
            .0
            .insert(id.clone(), system);
        self.native_effect_factories.insert(id, factory);
        Ok(())
    }

    /// Applies one player action.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] when the action is illegal or resolution fails.
    pub fn apply(&mut self, action: GameAction) -> Result<(), SimulationError> {
        submit_action(&mut self.app, action)
    }

    pub fn legal_actions(&mut self) -> Vec<GameAction> {
        legal_actions(self.app.world_mut())
    }

    /// Returns the choice that suspended resolution, if any.
    pub fn pending_choice(&self) -> Option<&crate::PendingChoice> {
        self.app
            .world()
            .resource::<ResolutionWork>()
            .pending_choice
            .as_ref()
    }

    /// Selects one option for the pending choice and resumes the retained LIFO work.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] when no choice is pending, the option is invalid, or resumed
    /// resolution fails.
    pub fn choose(&mut self, option: crate::ChoiceId) -> Result<(), SimulationError> {
        submit_choice(&mut self.app, option)
    }

    /// Exposes the serializable logical resolution state for inspection and persistence.
    pub fn resolution_work(&self) -> &ResolutionWork {
        self.app.world().resource::<ResolutionWork>()
    }

    pub fn snapshot(&mut self) -> GameSnapshot {
        build_snapshot(self.app.world_mut())
    }

    pub fn trace(&self) -> &[TraceEntry] {
        &self.app.world().resource::<CanonicalTrace>().entries
    }

    /// Captures all durable simulation state using logical entity identifiers.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] if an operation is currently executing or a durable
    /// relationship cannot be translated to a logical entity ID.
    pub fn checkpoint(&self) -> Result<SimulationCheckpoint, SimulationError> {
        build_checkpoint(self.app.world())
    }

    /// Restores a new simulation from a checkpoint without native-effect registrations.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] if the checkpoint version, references, retained work, or
    /// restored invariants are invalid.
    pub fn from_checkpoint(checkpoint: SimulationCheckpoint) -> Result<Self, SimulationError> {
        Self::from_checkpoint_with_factories(checkpoint, BTreeMap::new())
    }

    /// Replaces this simulation with checkpoint state while retaining native-effect factories.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] if the checkpoint cannot be validated or restored. The
    /// original simulation remains unchanged on failure.
    pub fn restore(&mut self, checkpoint: SimulationCheckpoint) -> Result<(), SimulationError> {
        let restored =
            Self::from_checkpoint_with_factories(checkpoint, self.native_effect_factories.clone())?;
        *self = restored;
        Ok(())
    }

    /// Creates an exact checkpoint-based clone of this simulation.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] if the current state cannot be checkpointed or restored.
    pub fn fork(&self) -> Result<Self, SimulationError> {
        Self::from_checkpoint_with_factories(
            self.checkpoint()?,
            self.native_effect_factories.clone(),
        )
    }

    fn from_checkpoint_with_factories(
        checkpoint: SimulationCheckpoint,
        factories: BTreeMap<NativeEffectId, NativeEffectFactory>,
    ) -> Result<Self, SimulationError> {
        let mut app = App::new();
        app.add_plugins(HearthstoneSimulationPlugin);
        for (id, factory) in &factories {
            let system = factory(app.world_mut());
            app.world_mut()
                .resource_mut::<NativeEffectRegistry>()
                .0
                .insert(id.clone(), system);
        }
        restore_checkpoint(app.world_mut(), checkpoint)?;
        Ok(Self {
            app,
            native_effect_factories: factories,
        })
    }

    /// Validates zone, resolution, and logical entity-index invariants.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError::Invariant`] describing the first violated invariant.
    pub fn assert_invariants(&self) -> Result<(), SimulationError> {
        assert_zone_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_resolution_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_game_entity_index(self.app.world()).map_err(SimulationError::Invariant)
    }
}

#[cfg(test)]
#[path = "simulation_tests_actions.rs"]
mod action_tests;
#[cfg(test)]
#[path = "simulation_tests_api.rs"]
mod api_tests;
#[cfg(test)]
#[path = "simulation_tests_effects.rs"]
mod effect_tests;
#[cfg(test)]
#[path = "simulation_tests_events.rs"]
mod event_tests;
#[cfg(test)]
#[path = "simulation_tests_health.rs"]
mod health_tests;
#[cfg(test)]
#[path = "simulation_test_support.rs"]
mod test_support;
