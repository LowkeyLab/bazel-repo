use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::{
    CanonicalTrace, DeathEventCache, DeterministicRng, Effect, EffectContext, GameState,
    NativeEffectId, PlayerConfig, ResolutionCursor, Ruleset, SimulationStatus, TraceEntry,
    death::{DefeatedHeroes, PendingDeaths},
    entity::{
        GameEntityIndex, NextGameEntityId, PlayOrderCounter, assert_runtime_shape_invariants,
    },
    native_effect::{NativeEffectFactory, NativeEffectRegistry},
    resolver::{NextResolutionId, assert_resolution_invariants, configure_resolution},
    trigger::TriggerGuards,
    zone::{ZoneIndex, assert_zone_invariants},
};

#[cfg(test)]
use crate::{
    Armor, Card, Damage, EntityKind, EventContext, EventKind, EventOrderKey, EventValueOperation,
    GameEntityId, GameObject, GameOutcome, Keyword, PlayerId, PlayerSelector, QueueKind,
    QueueState, QueuedEvent, QueuedTrigger, ResolutionKind, ResolutionQueue, RuntimeTriggers,
    STARTING_HEALTH, Selector, ValueExpression, Zone,
    enchantment::StatModifier,
    entity::{game_entity, has_keyword, insert_keyword, materialize_entity_form, remove_keyword},
    queue::{add_event_entry, freeze_queue},
    resolver::{
        activate_resolution_child, begin_resolution, cleanup_resolution, complete_active,
        spawn_resolution_child,
    },
    trigger::TriggersSuppressed,
    zone::ZoneError,
};

#[path = "simulation_action.rs"]
mod action;
#[path = "simulation_card_runtime.rs"]
mod card_runtime;
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
pub use error::SimulationError;
pub use snapshot::{GameObjectSnapshot, GameSnapshot, PlayerSnapshot};

use action::{configure_actions, legal_actions, submit_action};
use card_runtime::setup_game;
use snapshot::{assert_game_entity_index, build_snapshot};

#[cfg(test)]
use card_runtime::spawn_card;
#[cfg(test)]
use effect_executor::{
    attach_stat_modifier, copy_card_data, evaluate_value, execute_effect, execute_effects,
    modify_active_event_value, resolve_player, select_entities, silence_entity, transform_entity,
};
#[cfg(test)]
use event_resolver::resolve_prepared_events;
#[cfg(test)]
use health::{
    HealingRequest, SimultaneousEventOrder, apply_damage, apply_damage_batch, apply_healing_batch,
};
#[cfg(test)]
use player::{check_outcome, draw_card, hero_id, player};

pub struct HearthstoneSimulationPlugin;

impl Plugin for HearthstoneSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
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
            .init_resource::<ResolutionCursor>()
            .init_resource::<NextResolutionId>()
            .init_resource::<TriggerGuards>()
            .init_resource::<NativeEffectRegistry>();
        configure_resolution(app);
        configure_actions(app);
    }
}

pub struct Simulation {
    app: App,
    initial_players: [PlayerConfig; 2],
    seed: u64,
    action_history: Vec<GameAction>,
    native_effect_factories: BTreeMap<NativeEffectId, NativeEffectFactory>,
}

impl Simulation {
    pub fn new(players: [PlayerConfig; 2]) -> Self {
        Self::with_seed(players, 0)
    }

    pub fn with_seed(players: [PlayerConfig; 2], seed: u64) -> Self {
        let initial_players = players.clone();
        let mut app = App::new();
        app.add_plugins(HearthstoneSimulationPlugin);
        app.world_mut().insert_resource(DeterministicRng::new(seed));
        setup_game(app.world_mut(), players).expect("valid player fixture should initialize");
        assert_runtime_shape_invariants(app.world())
            .expect("game setup should materialize valid runtime shapes");
        app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
        Self {
            app,
            initial_players,
            seed,
            action_history: Vec::new(),
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

    /// Applies one player action and records it when resolution succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] when the action is illegal or resolution fails.
    pub fn apply(&mut self, action: GameAction) -> Result<(), SimulationError> {
        let result = submit_action(&mut self.app, action.clone());
        if result.is_ok() {
            self.action_history.push(action);
        }
        result
    }

    pub fn legal_actions(&mut self) -> Vec<GameAction> {
        legal_actions(self.app.world_mut())
    }

    pub fn snapshot(&mut self) -> GameSnapshot {
        build_snapshot(self.app.world_mut())
    }

    pub fn trace(&self) -> &[TraceEntry] {
        &self.app.world().resource::<CanonicalTrace>().entries
    }

    pub fn fork(&self) -> Result<Self, SimulationError> {
        let mut fork = Self::with_seed(self.initial_players.clone(), self.seed);
        for (id, factory) in &self.native_effect_factories {
            let system = factory(fork.app.world_mut());
            fork.app
                .world_mut()
                .resource_mut::<NativeEffectRegistry>()
                .0
                .insert(id.clone(), system);
            fork.native_effect_factories
                .insert(id.clone(), factory.clone());
        }
        for action in &self.action_history {
            fork.apply(action.clone())?;
        }
        fork.assert_invariants()?;
        Ok(fork)
    }

    /// Verifies zone, resolution, logical-index, and runtime-shape invariants.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError::Invariant`] when durable or execution state has drifted.
    pub fn assert_invariants(&self) -> Result<(), SimulationError> {
        assert_zone_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_resolution_invariants(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_game_entity_index(self.app.world()).map_err(SimulationError::Invariant)?;
        assert_runtime_shape_invariants(self.app.world()).map_err(SimulationError::Invariant)
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
