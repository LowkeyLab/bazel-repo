use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::{
    Abilities, Armor, AttackState, AuraCache, BaseStats, CanonicalTrace, Controller, CurrentStats,
    Damage, DeathEventCache, DeathRecord, DefinitionId, DeterministicRng, DisplayName,
    DominantPlayer, Enchantments, EntityKind, GameEntityId, GameObject, GameState, Keywords,
    PlayOrder, Player, PlayerId, ResolutionWork, RngSnapshot, Ruleset, RuntimeTriggers,
    StatModifier, Zone,
    death::{DefeatedHeroes, PendingDeaths},
    enchantment::AttachedTo,
    entity::{NextGameEntityId, PlayOrderCounter, game_entity},
    trigger::TriggersSuppressed,
    zone::{assert_zone_invariants, insert_into_zone},
};

use super::{
    card_runtime::CardRuntime, effect_executor::validate_effect_program, error::SimulationError,
    snapshot::assert_game_entity_index,
};

pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CardRuntimeCheckpoint {
    pub cost: i32,
    pub program: Vec<crate::Effect>,
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
    pub current_stats: Option<CurrentStats>,
    pub damage: Option<Damage>,
    pub armor: Option<Armor>,
    pub pending_destroy: bool,
    pub keywords: Option<Keywords>,
    pub abilities: Option<Abilities>,
    pub enchantments: Option<Enchantments>,
    pub attack_state: Option<AttackState>,
    pub player: Option<Player>,
    pub card_runtime: Option<CardRuntimeCheckpoint>,
    pub runtime_triggers: Option<Vec<crate::TriggerDefinition>>,
    pub triggers_suppressed: bool,
    pub stat_modifier: Option<StatModifier>,
    pub aura_cache: Option<AuraCache>,
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

pub(super) fn build_checkpoint(world: &World) -> Result<SimulationCheckpoint, SimulationError> {
    if world.resource::<crate::CurrentResolutionOp>().0.is_some() {
        return Err(SimulationError::Checkpoint(
            "cannot checkpoint while an operation is executing".to_string(),
        ));
    }
    let mut entities = world
        .iter_entities()
        .filter(bevy::prelude::EntityRef::contains::<GameObject>)
        .map(|entity| {
            let id = *entity.get::<GameEntityId>().ok_or_else(|| {
                SimulationError::Checkpoint("GameObject has no logical ID".into())
            })?;
            let attached_to = entity
                .get::<AttachedTo>()
                .map(|attached| {
                    world
                        .get::<GameEntityId>(attached.0)
                        .copied()
                        .ok_or_else(|| {
                            SimulationError::Checkpoint(format!(
                                "attachment {id:?} targets a non-game entity"
                            ))
                        })
                })
                .transpose()?;
            Ok(GameEntityCheckpoint {
                id,
                definition_id: entity.get::<DefinitionId>().map(|value| value.0.clone()),
                kind: entity.get::<EntityKind>().copied(),
                controller: entity.get::<Controller>().map(|value| value.0),
                display_name: entity.get::<DisplayName>().map(|value| value.0.clone()),
                play_order: entity.get::<PlayOrder>().map(|value| value.0),
                base_stats: entity.get::<BaseStats>().copied(),
                current_stats: entity.get::<CurrentStats>().copied(),
                damage: entity.get::<Damage>().copied(),
                armor: entity.get::<Armor>().copied(),
                pending_destroy: entity.contains::<crate::PendingDestroy>(),
                keywords: entity.get::<Keywords>().cloned(),
                abilities: entity.get::<Abilities>().cloned(),
                enchantments: entity.get::<Enchantments>().cloned(),
                attack_state: entity.get::<AttackState>().copied(),
                player: entity.get::<Player>().cloned(),
                card_runtime: entity
                    .get::<CardRuntime>()
                    .map(|runtime| CardRuntimeCheckpoint {
                        cost: runtime.cost,
                        program: runtime.program.clone(),
                    }),
                runtime_triggers: entity.get::<RuntimeTriggers>().map(|value| value.0.clone()),
                triggers_suppressed: entity.contains::<TriggersSuppressed>(),
                stat_modifier: entity.get::<StatModifier>().copied(),
                aura_cache: entity.get::<AuraCache>().cloned(),
                attached_to,
                death_record: entity.get::<DeathRecord>().cloned(),
                zone: entity.get::<Zone>().copied(),
                zone_position: entity.get::<crate::ZonePosition>().map(|value| value.0),
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    entities.sort_by_key(|entity| entity.id);

    Ok(SimulationCheckpoint {
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        ruleset: world.resource::<Ruleset>().clone(),
        game: world.resource::<GameState>().clone(),
        dominant_player: world.resource::<DominantPlayer>().0,
        next_game_entity_id: world.resource::<NextGameEntityId>().0,
        next_play_order: world.resource::<PlayOrderCounter>().0,
        rng: world.resource::<DeterministicRng>().state(),
        trace: world.resource::<CanonicalTrace>().clone(),
        deaths: world.resource::<DeathEventCache>().records.clone(),
        pending_deaths: world.resource::<PendingDeaths>().0.clone(),
        defeated_heroes: world.resource::<DefeatedHeroes>().0.clone(),
        resolution: world.resource::<ResolutionWork>().clone(),
        entities,
    })
}

pub(super) fn restore_checkpoint(
    world: &mut World,
    checkpoint: SimulationCheckpoint,
) -> Result<(), SimulationError> {
    validate_checkpoint(&checkpoint)?;
    let rng = DeterministicRng::from_snapshot(checkpoint.rng).ok_or_else(|| {
        SimulationError::Checkpoint(format!(
            "unsupported RNG algorithm version {}",
            checkpoint.rng.algorithm_version
        ))
    })?;

    world.insert_resource(checkpoint.ruleset);
    world.insert_resource(checkpoint.game);
    world.insert_resource(DominantPlayer(checkpoint.dominant_player));
    world.insert_resource(NextGameEntityId(checkpoint.next_game_entity_id));
    world.insert_resource(PlayOrderCounter(checkpoint.next_play_order));
    world.insert_resource(rng);
    world.insert_resource(checkpoint.trace);
    world.insert_resource(DeathEventCache {
        records: checkpoint.deaths,
    });
    world.insert_resource(PendingDeaths(checkpoint.pending_deaths));
    world.insert_resource(DefeatedHeroes(checkpoint.defeated_heroes));
    world.insert_resource(checkpoint.resolution);

    for object in &checkpoint.entities {
        restore_entity_components(world, object);
    }

    let mut zoned = checkpoint
        .entities
        .iter()
        .filter_map(|object| {
            object.zone.map(|zone| {
                (
                    object
                        .controller
                        .expect("validated zoned entity controller"),
                    zone,
                    object.zone_position.expect("validated zone position"),
                    object.id,
                )
            })
        })
        .collect::<Vec<_>>();
    zoned.sort_by_key(|(player, zone, position, id)| (*player, *zone, *position, *id));
    for (player, zone, _, id) in zoned {
        insert_into_zone(world, id, player, zone, None)?;
    }

    validate_restored_programs(world)?;

    for object in &checkpoint.entities {
        if let Some(target) = object.attached_to {
            let entity = game_entity(world, object.id).ok_or_else(|| {
                SimulationError::Checkpoint(format!("missing entity {:?}", object.id))
            })?;
            let target = game_entity(world, target).ok_or_else(|| {
                SimulationError::Checkpoint(format!("missing attachment target {target:?}"))
            })?;
            world.entity_mut(entity).insert(AttachedTo(target));
        }
    }

    assert_zone_invariants(world).map_err(SimulationError::Invariant)?;
    crate::resolver::assert_resolution_invariants(world).map_err(SimulationError::Invariant)?;
    assert_game_entity_index(world).map_err(SimulationError::Invariant)
}

fn restore_entity_components(world: &mut World, object: &GameEntityCheckpoint) {
    let entity = world.spawn((GameObject, object.id)).id();
    let mut entity = world.entity_mut(entity);
    if let Some(value) = &object.definition_id {
        entity.insert(DefinitionId(value.clone()));
    }
    if let Some(value) = object.kind {
        entity.insert(value);
    }
    if let Some(value) = object.controller {
        entity.insert(Controller(value));
    }
    if let Some(value) = &object.display_name {
        entity.insert(DisplayName(value.clone()));
    }
    if let Some(value) = object.play_order {
        entity.insert(PlayOrder(value));
    }
    if let Some(value) = object.base_stats {
        entity.insert(value);
    }
    if let Some(value) = object.current_stats {
        entity.insert(value);
    }
    if let Some(value) = object.damage {
        entity.insert(value);
    }
    if let Some(value) = object.armor {
        entity.insert(value);
    }
    if object.pending_destroy {
        entity.insert(crate::PendingDestroy);
    }
    if let Some(value) = &object.keywords {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.abilities {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.enchantments {
        entity.insert(value.clone());
    }
    if let Some(value) = object.attack_state {
        entity.insert(value);
    }
    if let Some(value) = &object.player {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.card_runtime {
        entity.insert(CardRuntime {
            cost: value.cost,
            program: value.program.clone(),
        });
    }
    if let Some(value) = &object.runtime_triggers {
        entity.insert(RuntimeTriggers(value.clone()));
    }
    if object.triggers_suppressed {
        entity.insert(TriggersSuppressed);
    }
    if let Some(value) = object.stat_modifier {
        entity.insert(value);
    }
    if let Some(value) = &object.aura_cache {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.death_record {
        entity.insert(value.clone());
    }
}

fn validate_restored_programs(world: &World) -> Result<(), SimulationError> {
    // Dormant card programs remain legal without registrations: normal action validation rejects
    // them before mutation. Only already-retained resolution work must be executable immediately.
    let resolution = world.resource::<ResolutionWork>();
    for stacked in &resolution.stack {
        validate_resolution_operation(world, &stacked.operation)?;
    }
    for event in resolution.events.values() {
        if let Some(seeds) = &event.prechecked_triggers {
            for seed in seeds {
                validate_effect_program(
                    world,
                    &seed.definition.effect_program,
                    Some(seed.definition.event),
                )?;
            }
        }
        if let Some(candidates) = &event.candidates {
            for candidate in candidates {
                validate_effect_program(
                    world,
                    &candidate.definition.effect_program,
                    Some(candidate.definition.event),
                )?;
            }
        }
    }
    Ok(())
}

fn validate_resolution_operation(
    world: &World,
    operation: &crate::ResolutionOp,
) -> Result<(), SimulationError> {
    match operation {
        crate::ResolutionOp::RunEffect { effect, event, .. } => {
            let event = event.and_then(|event| {
                world
                    .resource::<ResolutionWork>()
                    .events
                    .get(&event)
                    .map(|prepared| prepared.context.kind)
            });
            validate_effect_program(world, std::slice::from_ref(effect), event)
        }
        crate::ResolutionOp::AttemptTrigger(candidate) => validate_effect_program(
            world,
            &candidate.definition.effect_program,
            Some(candidate.definition.event),
        ),
        crate::ResolutionOp::RequestChoice(request) => {
            for option in &request.options {
                for operation in &option.operations {
                    validate_resolution_operation(world, operation)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_checkpoint(checkpoint: &SimulationCheckpoint) -> Result<(), SimulationError> {
    if checkpoint.schema_version != CHECKPOINT_SCHEMA_VERSION {
        return Err(SimulationError::Checkpoint(format!(
            "unsupported checkpoint schema version {}",
            checkpoint.schema_version
        )));
    }
    if checkpoint.ruleset.rulebook_revision != crate::RULEBOOK_REVISION {
        return Err(SimulationError::Checkpoint(format!(
            "unsupported rulebook revision {}",
            checkpoint.ruleset.rulebook_revision
        )));
    }
    if checkpoint.trace.ruleset != checkpoint.ruleset.id {
        return Err(SimulationError::Checkpoint(
            "trace and checkpoint rulesets disagree".to_string(),
        ));
    }
    validate_next_counter(
        "resolution",
        checkpoint.resolution.next_resolution_id,
        checkpoint
            .resolution
            .stack
            .iter()
            .map(|stacked| stacked.id.0)
            .max(),
    )?;
    validate_next_counter(
        "event",
        checkpoint.resolution.next_event_id,
        checkpoint
            .resolution
            .events
            .keys()
            .map(|event| event.0)
            .max(),
    )?;
    validate_next_counter(
        "event slot",
        checkpoint.resolution.next_event_slot_id,
        checkpoint
            .resolution
            .event_slots
            .keys()
            .map(|slot| slot.0)
            .max(),
    )?;
    let ids = checkpoint
        .entities
        .iter()
        .map(|entity| entity.id)
        .collect::<BTreeSet<_>>();
    if ids.len() != checkpoint.entities.len() {
        return Err(SimulationError::Checkpoint(
            "checkpoint contains duplicate logical entity IDs".to_string(),
        ));
    }
    if ids
        .iter()
        .next_back()
        .is_some_and(|id| id.0 >= checkpoint.next_game_entity_id)
    {
        return Err(SimulationError::Checkpoint(
            "next logical entity ID does not exceed existing IDs".to_string(),
        ));
    }
    for entity in &checkpoint.entities {
        if entity.zone.is_some() && (entity.controller.is_none() || entity.zone_position.is_none())
        {
            return Err(SimulationError::Checkpoint(format!(
                "zoned entity {:?} lacks a controller or position",
                entity.id
            )));
        }
        if entity
            .attached_to
            .is_some_and(|target| !ids.contains(&target))
        {
            return Err(SimulationError::Checkpoint(format!(
                "entity {:?} has a missing attachment target",
                entity.id
            )));
        }
        if entity
            .enchantments
            .as_ref()
            .is_some_and(|enchantments| enchantments.0.iter().any(|id| !ids.contains(id)))
        {
            return Err(SimulationError::Checkpoint(format!(
                "entity {:?} references a missing enchantment",
                entity.id
            )));
        }
    }
    Ok(())
}

fn validate_next_counter(
    name: &str,
    next: u64,
    highest_retained: Option<u64>,
) -> Result<(), SimulationError> {
    if highest_retained.is_some_and(|highest| highest >= next) {
        return Err(SimulationError::Checkpoint(format!(
            "next {name} ID does not exceed retained IDs"
        )));
    }
    Ok(())
}
