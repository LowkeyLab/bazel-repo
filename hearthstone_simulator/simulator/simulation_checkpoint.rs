use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::{
    Abilities, Armor, AttackAuraCache, AttackState, BaseKeywords, BaseStats,
    CHECKPOINT_SCHEMA_VERSION, CanonicalTrace, CardRuntimeCheckpoint, Controller, CostModifier,
    CurrentStats, Damage, DeathEventCache, DeathRecord, DefinitionId, DeterministicRng,
    DisplayName, DominantPlayer, EnchantmentDuration, Enchantments, EntityKind,
    GameEntityCheckpoint, GameEntityId, GameObject, GameState, HealthAuraCache, HeroMetadata,
    HeroPowerState, KeepEnchantments, KeywordModifier, Keywords, OtherAuraCache, PlayOrder, Player,
    PlayerId, ResolutionWork, Ruleset, RuntimeAuras, RuntimeContinuousEffects, RuntimeTriggers,
    SilenceRemovable, Silenced, SimulationCheckpoint, StatModifier, TurnSchedule, Zone,
    death::{DefeatedHeroes, PendingDeaths},
    enchantment::{AttachedTo, assert_enchantment_invariants},
    entity::{NextGameEntityId, PlayOrderCounter, game_entity},
    zone::{assert_zone_invariants, insert_into_zone},
};

use super::{
    card_runtime::CardRuntime, effect_executor::validate_effect_program, error::SimulationError,
    player::assert_player_role_invariants, snapshot::assert_game_entity_index,
};

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
                base_keywords: entity.get::<BaseKeywords>().cloned(),
                current_stats: entity.get::<CurrentStats>().copied(),
                damage: entity.get::<Damage>().copied(),
                armor: entity.get::<Armor>().copied(),
                pending_destroy: entity.contains::<crate::PendingDestroy>(),
                keywords: entity.get::<Keywords>().cloned(),
                abilities: entity.get::<Abilities>().cloned(),
                enchantments: entity.get::<Enchantments>().cloned(),
                attack_state: entity.get::<AttackState>().copied(),
                hero_metadata: entity.get::<HeroMetadata>().copied(),
                hero_power_state: entity.get::<HeroPowerState>().copied(),
                player: entity.get::<Player>().cloned(),
                card_runtime: entity
                    .get::<CardRuntime>()
                    .map(|runtime| CardRuntimeCheckpoint {
                        base_cost: runtime.base_cost,
                        cost: runtime.cost,
                        program: runtime.program.clone(),
                    }),
                runtime_triggers: entity.get::<RuntimeTriggers>().map(|value| value.0.clone()),
                runtime_auras: entity.get::<RuntimeAuras>().map(|value| value.0.clone()),
                runtime_continuous_effects: entity
                    .get::<RuntimeContinuousEffects>()
                    .map(|value| value.0.clone()),
                silenced: entity.contains::<Silenced>(),
                keep_enchantments: entity.contains::<KeepEnchantments>(),
                silence_removable: entity.contains::<SilenceRemovable>(),
                stat_modifier: entity.get::<StatModifier>().copied(),
                keyword_modifier: entity.get::<KeywordModifier>().copied(),
                cost_modifier: entity.get::<CostModifier>().copied(),
                enchantment_duration: entity.get::<EnchantmentDuration>().copied(),
                health_aura_cache: entity.get::<HealthAuraCache>().cloned(),
                attack_aura_cache: entity.get::<AttackAuraCache>().cloned(),
                other_aura_cache: entity.get::<OtherAuraCache>().cloned(),
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
        turn_schedule: world.resource::<TurnSchedule>().clone(),
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
    world.insert_resource(checkpoint.turn_schedule);
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
    assert_enchantment_invariants(world).map_err(SimulationError::Invariant)?;
    assert_player_role_invariants(world).map_err(SimulationError::Invariant)?;
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
    if let Some(value) = &object.base_keywords {
        entity.insert(value.clone());
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
    if let Some(value) = object.hero_metadata {
        entity.insert(value);
    }
    if let Some(value) = object.hero_power_state {
        entity.insert(value);
    }
    if let Some(value) = &object.player {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.card_runtime {
        entity.insert(CardRuntime {
            base_cost: value.base_cost,
            cost: value.cost,
            program: value.program.clone(),
        });
    }
    if let Some(value) = &object.runtime_triggers {
        entity.insert(RuntimeTriggers(value.clone()));
    }
    if let Some(value) = &object.runtime_auras {
        entity.insert(RuntimeAuras(value.clone()));
    }
    if let Some(value) = &object.runtime_continuous_effects {
        entity.insert(RuntimeContinuousEffects(value.clone()));
    }
    if object.silenced {
        entity.insert(Silenced);
    }
    if object.keep_enchantments {
        entity.insert(KeepEnchantments);
    }
    if object.silence_removable {
        entity.insert(SilenceRemovable);
    }
    if let Some(value) = object.stat_modifier {
        entity.insert(value);
    }
    if let Some(value) = object.keyword_modifier {
        entity.insert(value);
    }
    if let Some(value) = object.cost_modifier {
        entity.insert(value);
    }
    if let Some(value) = object.enchantment_duration {
        entity.insert(value);
    }
    if let Some(value) = &object.health_aura_cache {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.attack_aura_cache {
        entity.insert(value.clone());
    }
    if let Some(value) = &object.other_aura_cache {
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
    if let Some(pending) = &resolution.pending_choice {
        validate_choice_request(world, &pending.request)?;
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
        crate::ResolutionOp::RequestChoice(request) => validate_choice_request(world, request),
        _ => Ok(()),
    }
}

fn validate_choice_request(
    world: &World,
    request: &crate::ChoiceRequest,
) -> Result<(), SimulationError> {
    for option in &request.options {
        for operation in &option.operations {
            validate_resolution_operation(world, operation)?;
        }
    }
    Ok(())
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
    validate_next_counter(
        "play order",
        checkpoint.next_play_order,
        checkpoint
            .entities
            .iter()
            .filter_map(|entity| entity.play_order)
            .max(),
    )?;
    for entity in &checkpoint.entities {
        validate_checkpoint_entity(entity, &ids)?;
    }
    validate_checkpoint_costs(checkpoint)?;
    validate_checkpoint_player_roles(checkpoint)?;
    Ok(())
}

fn validate_checkpoint_costs(checkpoint: &SimulationCheckpoint) -> Result<(), SimulationError> {
    for target in checkpoint
        .entities
        .iter()
        .filter(|entity| entity.card_runtime.is_some())
    {
        let runtime = target
            .card_runtime
            .as_ref()
            .expect("filtered card runtime exists");
        let mut modifiers = checkpoint
            .entities
            .iter()
            .filter(|entity| entity.attached_to == Some(target.id))
            .filter_map(|entity| {
                entity
                    .cost_modifier
                    .map(|modifier| (entity.play_order, entity.id, modifier))
            })
            .collect::<Vec<_>>();
        if modifiers.iter().any(|(order, _, _)| order.is_none()) {
            return Err(SimulationError::Checkpoint(format!(
                "cost modifier attached to {:?} lacks play order",
                target.id
            )));
        }
        modifiers.sort_by_key(|(order, id, _)| (order.unwrap_or_default(), *id));
        let cost = modifiers
            .into_iter()
            .filter(|(_, _, modifier)| !(target.silenced && modifier.silence_removable))
            .fold(runtime.base_cost, |cost, (_, _, modifier)| {
                modifier.apply(cost)
            });
        if cost != runtime.cost {
            return Err(SimulationError::Checkpoint(format!(
                "entity {:?} has inconsistent effective cost",
                target.id
            )));
        }
    }
    Ok(())
}

fn validate_checkpoint_player_roles(
    checkpoint: &SimulationCheckpoint,
) -> Result<(), SimulationError> {
    for player_id in PlayerId::ALL {
        let players = checkpoint
            .entities
            .iter()
            .filter(|entity| {
                entity
                    .player
                    .as_ref()
                    .is_some_and(|player| player.id == player_id)
            })
            .collect::<Vec<_>>();
        if players.len() != 1
            || players[0].kind != Some(EntityKind::Player)
            || players[0].controller != Some(player_id)
        {
            return Err(SimulationError::Checkpoint(format!(
                "player {player_id:?} must have exactly one valid Player entity"
            )));
        }

        let heroes = checkpoint
            .entities
            .iter()
            .filter(|entity| {
                entity.kind == Some(EntityKind::Hero)
                    && entity.controller == Some(player_id)
                    && entity.zone == Some(Zone::Play)
            })
            .collect::<Vec<_>>();
        if heroes.len() != 1
            || heroes[0].current_stats.is_none()
            || heroes[0].damage.is_none()
            || heroes[0].armor.is_none()
            || heroes[0].attack_state.is_none()
            || heroes[0].hero_metadata.is_none()
        {
            return Err(SimulationError::Checkpoint(format!(
                "player {player_id:?} must have exactly one valid active Hero"
            )));
        }

        let powers = checkpoint
            .entities
            .iter()
            .filter(|entity| {
                entity.kind == Some(EntityKind::HeroPower)
                    && entity.controller == Some(player_id)
                    && entity.zone == Some(Zone::Play)
            })
            .collect::<Vec<_>>();
        if powers.len() != 1
            || powers[0].hero_power_state.is_none()
            || powers[0].card_runtime.is_none()
        {
            return Err(SimulationError::Checkpoint(format!(
                "player {player_id:?} must have exactly one valid active Hero Power"
            )));
        }
    }
    Ok(())
}

fn validate_checkpoint_entity(
    entity: &GameEntityCheckpoint,
    ids: &BTreeSet<GameEntityId>,
) -> Result<(), SimulationError> {
    if entity.kind == Some(EntityKind::Enchantment) && entity.enchantment_duration.is_none() {
        return Err(SimulationError::Checkpoint(format!(
            "enchantment {:?} lacks enchantment duration",
            entity.id
        )));
    }
    if entity.kind == Some(EntityKind::Enchantment)
        && entity.attached_to.is_some()
        && entity.zone != Some(Zone::Play)
    {
        return Err(SimulationError::Checkpoint(format!(
            "attached enchantment {:?} is not in Play",
            entity.id
        )));
    }
    if entity.zone.is_some() && (entity.controller.is_none() || entity.zone_position.is_none()) {
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
    let dangling_aura_provider = entity
        .health_aura_cache
        .as_ref()
        .into_iter()
        .flat_map(|cache| &cache.0)
        .chain(
            entity
                .attack_aura_cache
                .as_ref()
                .into_iter()
                .flat_map(|cache| &cache.0),
        )
        .chain(
            entity
                .other_aura_cache
                .as_ref()
                .into_iter()
                .flat_map(|cache| &cache.0),
        )
        .find(|application| !ids.contains(&application.provider));
    if let Some(application) = dangling_aura_provider {
        return Err(SimulationError::Checkpoint(format!(
            "entity {:?} references missing aura provider {:?}",
            entity.id, application.provider
        )));
    }
    Ok(())
}

fn validate_next_counter(
    name: &str,
    next: u64,
    highest_retained: Option<u64>,
) -> Result<(), SimulationError> {
    if let Some(highest) = highest_retained.filter(|highest| *highest >= next) {
        return Err(SimulationError::Checkpoint(format!(
            "next {name} ID {next} does not exceed highest retained ID {highest}"
        )));
    }
    Ok(())
}
