use bevy::prelude::*;

use crate::{
    Armor, AttachedTo, AttackState, BaseKeywords, BaseStats, CanonicalTrace, Card, Controller,
    CostModifier, CurrentStats, Damage, DamageRequest, DefinitionId, DisplayName, Effect,
    EffectContext, EntityKind, EventId, EventKind, EventValueOperation, GameEntityId,
    HealingRequest, HeroClassPolicy, HeroHealthPolicy, HeroMetadata, HeroPowerState,
    HeroReplacement, KeywordModifier, Keywords, PendingDestroy, PlayerId, PlayerSelector,
    ResolutionOp, ResolutionWork, Ruleset, RuntimeAuras, RuntimeContinuousEffects, RuntimeTriggers,
    Selector, SilenceRemovable, Silenced, StatModifier, TraceEntry, ValueExpression, Zone,
    ZoneMoveOutcome, ZoneMoveRequest, ZoneMovementKind,
    enchantment::{recalculate_cost, recalculate_keywords, recalculate_stats},
    entity::{allocate_game_id, allocate_play_order, game_entity},
    native_effect::NativeEffectRegistry,
    resolver::push_resolution_ops,
    rng::choose_game_entity,
    zone::{
        ZoneIndex, board_is_full, insert_into_zone, move_entity, move_entity_with_request,
        validate_board_position,
    },
};

use super::{
    card_runtime::{CardRuntime, spawn_card},
    error::SimulationError,
    event_resolver::prepare_event,
    health::{SimultaneousEventOrder, apply_damage_batch, apply_healing_batch},
    player::{draw_card, hero_id, player_mut},
};

#[cfg(test)]
pub(super) fn execute_effects(
    world: &mut World,
    context: &EffectContext,
    effects: &[Effect],
) -> Result<(), SimulationError> {
    push_effects(world, context, effects, None);
    Ok(())
}

pub(super) fn push_effects(
    world: &mut World,
    context: &EffectContext,
    effects: &[Effect],
    event: Option<EventId>,
) {
    push_resolution_ops(
        world,
        effects
            .iter()
            .cloned()
            .map(|effect| ResolutionOp::RunEffect {
                context: context.clone(),
                effect,
                event,
            }),
    );
}

#[cfg(test)]
pub(super) fn execute_effect(
    world: &mut World,
    context: &EffectContext,
    effect: &Effect,
) -> Result<(), SimulationError> {
    execute_effect_operation(world, context, effect, None)
}

#[allow(
    clippy::too_many_lines,
    reason = "the exhaustive effect dispatcher keeps one-shot operation semantics in one place"
)]
pub(super) fn execute_effect_operation(
    world: &mut World,
    context: &EffectContext,
    effect: &Effect,
    event: Option<EventId>,
) -> Result<(), SimulationError> {
    match effect {
        Effect::DealDamage { targets, amount } => {
            let targets = select_entities(world, context, targets);
            let mut value = evaluate_value(world, context, *amount, targets.len());
            value = match context.origin {
                crate::EffectOrigin::Spell => value
                    .saturating_add(crate::aura::current_spell_damage(world, context.controller)),
                crate::EffectOrigin::HeroPower => value.saturating_add(
                    crate::aura::hero_power_damage_bonus(world, context.controller),
                ),
                crate::EffectOrigin::Other => value,
            };
            let requests = targets
                .into_iter()
                .map(|target| DamageRequest {
                    source: context.source,
                    target,
                    proposed: value,
                })
                .collect();
            apply_damage_batch(world, requests, SimultaneousEventOrder::OrderOfPlay)
        }
        Effect::Heal { targets, amount } => {
            let targets = select_entities(world, context, targets);
            let value = evaluate_value(world, context, *amount, targets.len());
            let requests = targets
                .into_iter()
                .map(|target| HealingRequest {
                    source: context.source,
                    target,
                    proposed: value,
                })
                .collect();
            apply_healing_batch(world, requests, SimultaneousEventOrder::OrderOfPlay)
        }
        Effect::ModifyEventValue { operation, value } => {
            modify_event_value(world, event, context, *operation, *value)
        }
        Effect::Destroy { targets } => {
            for target in select_entities(world, context, targets) {
                if let Some(entity) = game_entity(world, target) {
                    world.entity_mut(entity).insert(PendingDestroy);
                }
            }
            Ok(())
        }
        Effect::Draw { player, count } if *count > 1 => {
            push_effects(
                world,
                context,
                &(0..*count)
                    .map(|_| Effect::Draw {
                        player: *player,
                        count: 1,
                    })
                    .collect::<Vec<_>>(),
                event,
            );
            Ok(())
        }
        Effect::Draw { player, count } => {
            if *count == 1 {
                draw_card(world, resolve_player(context.controller, *player))?;
            }
            Ok(())
        }
        Effect::Move {
            targets,
            player,
            zone,
            kind,
        } => {
            let destination_controller = resolve_player(context.controller, *player);
            let mut targets = select_entities(world, context, targets);
            targets.sort_by_key(|id| {
                let order = game_entity(world, *id)
                    .and_then(|entity| world.get::<crate::PlayOrder>(entity))
                    .map_or(0, |order| order.0);
                (order, *id)
            });
            for target in targets {
                let outcome = move_entity_with_request(
                    world,
                    ZoneMoveRequest {
                        entity: target,
                        destination_controller,
                        destination: *zone,
                        position: None,
                        kind: *kind,
                    },
                )?;
                let (from, to) = match outcome {
                    ZoneMoveOutcome::Moved { from, .. } => (from, *zone),
                    ZoneMoveOutcome::FullZoneRemoval { from, .. } => (from, Zone::Graveyard),
                    ZoneMoveOutcome::PreventedByFullZone => continue,
                };
                world
                    .resource_mut::<CanonicalTrace>()
                    .entries
                    .push(TraceEntry::ZoneMoved {
                        entity: target,
                        from,
                        to,
                    });
            }
            Ok(())
        }
        Effect::GainResource {
            player,
            amount,
            temporary,
        } => {
            let player_id = resolve_player(context.controller, *player);
            let maximum = world.resource::<Ruleset>().maximum_mana;
            let (_, mut player, _, _) = player_mut(world, player_id)?;
            if *temporary {
                player.temporary_resources += *amount;
            } else {
                player.maximum_resources = (player.maximum_resources + *amount).min(maximum);
            }
            Ok(())
        }
        Effect::ScheduleExtraTurns {
            player,
            count,
            timing,
        } => {
            let player = resolve_player(context.controller, *player);
            let active_player = world.resource::<crate::GameState>().active_player;
            world.resource_mut::<crate::TurnSchedule>().schedule(
                active_player,
                player,
                *count,
                *timing,
            );
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::ExtraTurnsScheduled {
                    player,
                    count: *count,
                    timing: *timing,
                });
            Ok(())
        }
        Effect::ReplaceHero {
            player,
            replacement,
        } => replace_hero(
            world,
            resolve_player(context.controller, *player),
            replacement,
        ),
        Effect::Summon {
            player,
            card,
            board_index,
        } => {
            let player = resolve_player(context.controller, *player);
            if card.kind == EntityKind::Minion && board_is_full(world, player) {
                return Ok(());
            }
            validate_board_position(world, player, *board_index)?;
            let summoned = spawn_card(world, player, card.clone(), Zone::Play)?;
            if let Some(index) = board_index {
                move_entity(world, summoned, Zone::Play, Some(*index))?;
            }
            let order = allocate_play_order(world);
            let entity = game_entity(world, summoned).expect("summoned entity was indexed");
            world.entity_mut(entity).insert(order);
            if crate::resolver::resolution_is_active(world) {
                let event = prepare_event(
                    world,
                    crate::EventContext {
                        kind: EventKind::Summoned,
                        source: context.source,
                        targets: vec![summoned],
                        controller: player,
                        proposed_value: None,
                        actual_value: None,
                        simultaneous_ordinal: 0,
                    },
                );
                push_resolution_ops(
                    world,
                    [
                        ResolutionOp::RefreshAuras(crate::AuraRefreshPlan::Summon),
                        ResolutionOp::ResolveEvent(event),
                    ],
                );
            }
            Ok(())
        }
        Effect::AttachStatModifier { targets, modifier } => {
            for target in select_entities(world, context, targets) {
                attach_stat_modifier(world, context.controller, target, *modifier)?;
            }
            Ok(())
        }
        Effect::AttachTemporaryStatModifier {
            targets,
            modifier,
            duration,
        } => {
            for target in select_entities(world, context, targets) {
                let enchantment =
                    attach_stat_modifier(world, context.controller, target, *modifier)?;
                let entity = game_entity(world, enchantment)
                    .expect("new temporary enchantment remains indexed");
                world.entity_mut(entity).insert(*duration);
            }
            Ok(())
        }
        Effect::AttachKeywordModifier {
            targets,
            modifier,
            duration,
        } => {
            for target in select_entities(world, context, targets) {
                attach_keyword_modifier(world, context.controller, target, *modifier, *duration)?;
            }
            Ok(())
        }
        Effect::AttachCostModifier {
            targets,
            modifier,
            duration,
        } => {
            for target in select_entities(world, context, targets) {
                attach_cost_modifier(world, context.controller, target, *modifier, *duration)?;
            }
            Ok(())
        }
        Effect::AttachContinuousEffect {
            targets,
            effect,
            silence_removable,
        } => {
            for target in select_entities(world, context, targets) {
                attach_continuous_effect(
                    world,
                    context.controller,
                    target,
                    *effect,
                    *silence_removable,
                )?;
            }
            Ok(())
        }
        Effect::Silence { targets } => {
            for target in select_entities(world, context, targets) {
                silence_entity(world, target)?;
            }
            Ok(())
        }
        Effect::Transform { targets, card } => {
            for target in select_entities(world, context, targets) {
                transform_entity(world, target, card.clone())?;
            }
            Ok(())
        }
        Effect::Copy {
            targets,
            player,
            zone,
        } => {
            let controller = resolve_player(context.controller, *player);
            for target in select_entities(world, context, targets) {
                copy_entity(world, target, controller, *zone);
            }
            Ok(())
        }
        Effect::Native(id) => {
            let system = world
                .resource::<NativeEffectRegistry>()
                .0
                .get(id)
                .copied()
                .ok_or_else(|| SimulationError::NativeEffectNotRegistered(id.clone()))?;
            // Bevy flushes Commands queued by a registered system before returning. This is the
            // native-handler mutation boundary documented by the design; durable rules changes
            // should still be returned as an effect plan and resolved below.
            let plan = world
                .run_system_with(system, context.clone())
                .map_err(|error| SimulationError::NativeEffectFailed {
                    id: id.clone(),
                    reason: error.to_string(),
                })?;
            let event_kind = event.and_then(|event| {
                world
                    .resource::<ResolutionWork>()
                    .events
                    .get(&event)
                    .map(|event| event.context.kind)
            });
            validate_effect_program(world, &plan, event_kind)?;
            push_effects(world, context, &plan, event);
            Ok(())
        }
        Effect::Sequence(nested) => {
            push_effects(world, context, nested, event);
            Ok(())
        }
    }
}

pub(super) fn validate_effect_program(
    world: &World,
    effects: &[Effect],
    event: Option<EventKind>,
) -> Result<(), SimulationError> {
    for effect in effects {
        match effect {
            Effect::Native(id) if !world.resource::<NativeEffectRegistry>().0.contains_key(id) => {
                return Err(SimulationError::NativeEffectNotRegistered(id.clone()));
            }
            Effect::ModifyEventValue { .. }
                if !matches!(
                    event,
                    Some(EventKind::ProposedDamage | EventKind::ProposedHealing)
                ) =>
            {
                return Err(SimulationError::NoModifiableEventValue);
            }
            Effect::Sequence(nested) => validate_effect_program(world, nested, event)?,
            Effect::Summon { card, .. } | Effect::Transform { card, .. } => {
                validate_card_program(world, card)?;
            }
            Effect::ReplaceHero { replacement, .. } => {
                validate_hero_replacement(world, replacement)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_card_program(world: &World, card: &Card) -> Result<(), SimulationError> {
    validate_effect_program(world, &card.effects, None)?;
    for trigger in &card.triggers {
        validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
    }
    Ok(())
}

fn validate_hero_replacement(
    world: &World,
    replacement: &HeroReplacement,
) -> Result<(), SimulationError> {
    if replacement.hero.kind != EntityKind::Hero {
        return Err(SimulationError::InvalidHeroReplacement(
            "replacement hero must have Hero kind".to_string(),
        ));
    }
    if replacement.hero_power.kind != EntityKind::HeroPower {
        return Err(SimulationError::InvalidHeroReplacement(
            "replacement Hero Power must have HeroPower kind".to_string(),
        ));
    }
    if replacement
        .weapon
        .as_ref()
        .is_some_and(|weapon| weapon.kind != EntityKind::Weapon)
    {
        return Err(SimulationError::InvalidHeroReplacement(
            "replacement weapon must have Weapon kind".to_string(),
        ));
    }
    if let HeroHealthPolicy::Set {
        maximum_health,
        current_health,
    } = replacement.health
        && (maximum_health <= 0 || current_health < 0 || current_health > maximum_health)
    {
        return Err(SimulationError::InvalidHeroReplacement(
            "replacement Health must satisfy 0 <= current <= maximum and maximum > 0".to_string(),
        ));
    }
    validate_card_program(world, &replacement.hero)?;
    validate_card_program(world, &replacement.hero_power)?;
    if let Some(weapon) = &replacement.weapon {
        validate_card_program(world, weapon)?;
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "hero replacement deliberately centralizes its atomic preservation and reset policy"
)]
pub(super) fn replace_hero(
    world: &mut World,
    player: PlayerId,
    replacement: &HeroReplacement,
) -> Result<(), SimulationError> {
    validate_hero_replacement(world, replacement)?;
    let previous = hero_id(world, player).ok_or(SimulationError::PlayerNotFound(player))?;
    let previous_entity =
        game_entity(world, previous).ok_or(SimulationError::EntityNotFound(previous))?;
    let previous_stats = world
        .get::<CurrentStats>(previous_entity)
        .copied()
        .ok_or(SimulationError::EntityNotFound(previous))?;
    let previous_damage = world
        .get::<Damage>(previous_entity)
        .copied()
        .unwrap_or_default();
    let previous_armor = world
        .get::<Armor>(previous_entity)
        .copied()
        .unwrap_or_default();
    let previous_attack = world
        .get::<AttackState>(previous_entity)
        .copied()
        .unwrap_or_default();
    let previous_class = world
        .get::<HeroMetadata>(previous_entity)
        .map(|metadata| metadata.class)
        .unwrap_or_default();
    detach_all_enchantments(world, previous);
    move_out_of_play(world, previous, player, Zone::SetAside)?;

    let (maximum_health, current_health) = match replacement.health {
        HeroHealthPolicy::Preserve => (
            previous_stats.maximum_health,
            previous_stats
                .maximum_health
                .saturating_sub(previous_damage.0),
        ),
        HeroHealthPolicy::Set {
            maximum_health,
            current_health,
        } => (maximum_health, current_health),
    };
    let mut hero_card = replacement.hero.clone();
    hero_card.health = maximum_health;
    let hero = spawn_card(world, player, hero_card, Zone::Play)?;
    let hero_entity = game_entity(world, hero).expect("replacement hero remains indexed");
    let class = match replacement.class {
        HeroClassPolicy::Keep => previous_class,
        HeroClassPolicy::Replace(class) => class,
    };
    let hero_order = allocate_play_order(world);
    world.entity_mut(hero_entity).insert((
        CurrentStats {
            attack: replacement.hero.attack,
            maximum_health,
        },
        Damage(maximum_health.saturating_sub(current_health)),
        Armor(
            previous_armor
                .0
                .saturating_add(replacement.armor_gain)
                .max(0),
        ),
        previous_attack,
        HeroMetadata { class },
        hero_order,
    ));

    let previous_power = controlled_kind(world, player, Zone::Play, EntityKind::HeroPower)
        .first()
        .copied();
    for power in controlled_kind(world, player, Zone::Play, EntityKind::HeroPower) {
        detach_all_enchantments(world, power);
        move_out_of_play(world, power, player, Zone::RemovedFromGame)?;
    }
    let hero_power = spawn_card(world, player, replacement.hero_power.clone(), Zone::Play)?;
    let hero_power_entity =
        game_entity(world, hero_power).expect("replacement power remains indexed");
    let power_order = allocate_play_order(world);
    world
        .entity_mut(hero_power_entity)
        .insert((HeroPowerState::default(), power_order));

    if let Some(weapon) = &replacement.weapon {
        for old_weapon in controlled_kind(world, player, Zone::Play, EntityKind::Weapon) {
            detach_all_enchantments(world, old_weapon);
            move_out_of_play(world, old_weapon, player, Zone::Graveyard)?;
        }
        let weapon = spawn_card(world, player, weapon.clone(), Zone::Play)?;
        let weapon_entity = game_entity(world, weapon).expect("replacement weapon remains indexed");
        let order = allocate_play_order(world);
        world.entity_mut(weapon_entity).insert(order);
    }

    world.resource_mut::<CanonicalTrace>().entries.extend([
        TraceEntry::HeroReplaced {
            player,
            previous,
            replacement: hero,
        },
        TraceEntry::HeroPowerReplaced {
            player,
            previous: previous_power,
            replacement: hero_power,
        },
    ]);
    // Hero replacement is not a minion summon. Its providers enter the ordinary aura lifecycle
    // and become visible at the next ruleset-defined phase boundary; Hero-card sequencing may add
    // a narrower played-provider refresh when that player-action sequence is implemented.
    Ok(())
}

fn controlled_kind(
    world: &World,
    player: PlayerId,
    zone: Zone,
    kind: EntityKind,
) -> Vec<GameEntityId> {
    world
        .resource::<ZoneIndex>()
        .entities(player, zone)
        .iter()
        .copied()
        .filter(|id| {
            game_entity(world, *id).is_some_and(|entity| {
                world.get::<EntityKind>(entity) == Some(&kind)
                    && world
                        .get::<Controller>(entity)
                        .is_some_and(|controller| controller.0 == player)
            })
        })
        .collect()
}

fn move_out_of_play(
    world: &mut World,
    id: GameEntityId,
    player: PlayerId,
    destination: Zone,
) -> Result<(), SimulationError> {
    let outcome = move_entity_with_request(
        world,
        ZoneMoveRequest {
            entity: id,
            destination_controller: player,
            destination,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )?;
    let ZoneMoveOutcome::Moved { from, .. } = outcome else {
        return Err(SimulationError::Invariant(format!(
            "hero replacement move produced {outcome:?}"
        )));
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: id,
            from,
            to: destination,
        });
    Ok(())
}

fn detach_all_enchantments(world: &mut World, target: GameEntityId) {
    let Some(target_entity) = game_entity(world, target) else {
        return;
    };
    let attachments = world
        .iter_entities()
        .filter_map(|entity| {
            if entity.get::<AttachedTo>().map(|attached| attached.0) != Some(target_entity) {
                return None;
            }
            Some((
                *entity.get::<GameEntityId>()?,
                entity.get::<Controller>()?.0,
                entity.id(),
            ))
        })
        .collect::<Vec<_>>();
    for (id, controller, entity) in attachments {
        world.entity_mut(entity).remove::<AttachedTo>();
        let _ = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: id,
                destination_controller: controller,
                destination: Zone::RemovedFromGame,
                position: None,
                kind: ZoneMovementKind::DetachEnchantment,
            },
        );
    }
    recalculate_stats(world, target);
    recalculate_keywords(world, target);
    recalculate_cost(world, target);
}

pub(super) fn attach_stat_modifier(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    modifier: StatModifier,
) -> Result<GameEntityId, SimulationError> {
    let target_entity =
        game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let id = allocate_game_id(world);
    let order = allocate_play_order(world);
    world.spawn((
        id,
        DefinitionId("synthetic:stat_modifier".to_string()),
        EntityKind::Enchantment,
        Controller(controller),
        DisplayName("Stat modifier".to_string()),
        order,
        modifier,
        AttachedTo(target_entity),
    ));
    insert_into_zone(world, id, controller, Zone::SetAside, None)
        .expect("a newly indexed enchantment must fit in the unbounded SetAside zone");
    recalculate_stats(world, target);
    Ok(id)
}

pub(super) fn attach_keyword_modifier(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    modifier: KeywordModifier,
    duration: Option<crate::TemporaryDuration>,
) -> Result<GameEntityId, SimulationError> {
    let target_entity =
        game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let id = allocate_game_id(world);
    let order = allocate_play_order(world);
    let entity = world
        .spawn((
            id,
            DefinitionId("synthetic:keyword_modifier".to_string()),
            EntityKind::Enchantment,
            Controller(controller),
            DisplayName("Keyword modifier".to_string()),
            order,
            modifier,
            AttachedTo(target_entity),
        ))
        .id();
    if modifier.silence_removable {
        world.entity_mut(entity).insert(SilenceRemovable);
    }
    if let Some(duration) = duration {
        world.entity_mut(entity).insert(duration);
    }
    insert_into_zone(world, id, controller, Zone::SetAside, None)
        .expect("a newly indexed enchantment must fit in the unbounded SetAside zone");
    recalculate_keywords(world, target);
    Ok(id)
}

pub(super) fn attach_cost_modifier(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    modifier: CostModifier,
    duration: Option<crate::TemporaryDuration>,
) -> Result<GameEntityId, SimulationError> {
    let target_entity =
        game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let id = allocate_game_id(world);
    let order = allocate_play_order(world);
    let entity = world
        .spawn((
            id,
            DefinitionId("synthetic:cost_modifier".to_string()),
            EntityKind::Enchantment,
            Controller(controller),
            DisplayName("Cost modifier".to_string()),
            order,
            modifier,
            AttachedTo(target_entity),
        ))
        .id();
    if modifier.silence_removable {
        world.entity_mut(entity).insert(SilenceRemovable);
    }
    if let Some(duration) = duration {
        world.entity_mut(entity).insert(duration);
    }
    insert_into_zone(world, id, controller, Zone::SetAside, None)
        .expect("a newly indexed enchantment must fit in the unbounded SetAside zone");
    recalculate_cost(world, target);
    Ok(id)
}

pub(super) fn attach_continuous_effect(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    effect: crate::ContinuousEffectDefinition,
    silence_removable: bool,
) -> Result<(), SimulationError> {
    let target_entity =
        game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    let id = allocate_game_id(world);
    let order = allocate_play_order(world);
    let entity = world
        .spawn((
            id,
            DefinitionId("synthetic:continuous_modifier".to_string()),
            EntityKind::Enchantment,
            Controller(controller),
            DisplayName("Continuous modifier".to_string()),
            order,
            RuntimeContinuousEffects(vec![effect]),
            AttachedTo(target_entity),
        ))
        .id();
    if silence_removable {
        world.entity_mut(entity).insert(SilenceRemovable);
    }
    insert_into_zone(world, id, controller, Zone::SetAside, None)
        .expect("a newly indexed enchantment must fit in the unbounded SetAside zone");
    Ok(())
}

pub(super) fn silence_entity(
    world: &mut World,
    target: GameEntityId,
) -> Result<(), SimulationError> {
    let entity = game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    if let Some(mut keywords) = world.get_mut::<Keywords>(entity) {
        keywords.0.clear();
    }
    world.entity_mut(entity).remove::<PendingDestroy>();
    world.entity_mut(entity).insert(Silenced);
    let enchantments = world
        .iter_entities()
        .filter_map(|candidate| {
            if candidate.get::<AttachedTo>().map(|attached| attached.0) == Some(entity)
                && (candidate
                    .get::<StatModifier>()
                    .is_some_and(|modifier| modifier.silence_removable)
                    || candidate.contains::<SilenceRemovable>())
            {
                Some((
                    *candidate.get::<GameEntityId>()?,
                    candidate.get::<Controller>()?.0,
                    candidate.id(),
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (id, controller, enchantment) in enchantments {
        world.entity_mut(enchantment).remove::<AttachedTo>();
        let _ = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: id,
                destination_controller: controller,
                destination: Zone::RemovedFromGame,
                position: None,
                kind: ZoneMovementKind::DetachEnchantment,
            },
        );
    }
    recalculate_stats(world, target);
    recalculate_keywords(world, target);
    recalculate_cost(world, target);
    Ok(())
}

pub(super) fn transform_entity(
    world: &mut World,
    target: GameEntityId,
    card: Card,
) -> Result<(), SimulationError> {
    detach_all_enchantments(world, target);
    let entity = game_entity(world, target).ok_or(SimulationError::EntityNotFound(target))?;
    world.entity_mut(entity).insert((
        DefinitionId(card.definition_id),
        DisplayName(card.name),
        card.kind,
        BaseStats {
            attack: card.attack,
            health: card.health,
        },
        CurrentStats {
            attack: card.attack,
            maximum_health: card.health,
        },
        Damage::default(),
        BaseKeywords(card.keywords.clone()),
        Keywords(card.keywords.clone()),
        CardRuntime {
            base_cost: card.mana_cost,
            cost: card.mana_cost,
            program: card.effects,
        },
        RuntimeTriggers(card.triggers),
        RuntimeAuras(card.auras),
        RuntimeContinuousEffects(card.continuous_effects),
    ));
    world.entity_mut(entity).remove::<PendingDestroy>();
    world.entity_mut(entity).remove::<Silenced>();
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CopySnapshot {
    card: Card,
    source_zone: Zone,
    silenced: bool,
}

fn copy_entity(world: &mut World, source: GameEntityId, controller: PlayerId, destination: Zone) {
    let Some(snapshot) = capture_copy_snapshot(world, source) else {
        return;
    };
    let Ok(copy) = spawn_card(world, controller, snapshot.card, destination) else {
        return;
    };
    let copy_entity = game_entity(world, copy).expect("new copy remains indexed");
    if snapshot.source_zone == Zone::Play && destination == Zone::Play {
        if snapshot.silenced {
            world.entity_mut(copy_entity).insert(Silenced);
        }
        let play_order = allocate_play_order(world);
        world.entity_mut(copy_entity).insert(play_order);
    }
}

fn capture_copy_snapshot(world: &World, source: GameEntityId) -> Option<CopySnapshot> {
    let entity = game_entity(world, source)?;
    Some(CopySnapshot {
        card: copy_card_data(world, source)?,
        source_zone: *world.get::<Zone>(entity)?,
        silenced: world.get::<Silenced>(entity).is_some(),
    })
}

pub(super) fn copy_card_data(world: &World, source: GameEntityId) -> Option<Card> {
    let entity = game_entity(world, source)?;
    let base = world.get::<BaseStats>(entity)?;
    let runtime = world.get::<CardRuntime>(entity)?;
    Some(Card {
        definition_id: world.get::<DefinitionId>(entity)?.0.clone(),
        name: world.get::<DisplayName>(entity)?.0.clone(),
        kind: *world.get::<EntityKind>(entity)?,
        mana_cost: runtime.base_cost,
        attack: base.attack,
        health: base.health,
        keywords: world
            .get::<BaseKeywords>(entity)
            .map_or_else(std::collections::BTreeSet::new, |keywords| {
                keywords.0.clone()
            }),
        effects: runtime.program.clone(),
        triggers: world
            .get::<RuntimeTriggers>(entity)
            .map_or_else(Vec::new, |triggers| triggers.0.clone()),
        auras: world
            .get::<RuntimeAuras>(entity)
            .map_or_else(Vec::new, |auras| auras.0.clone()),
        continuous_effects: world
            .get::<RuntimeContinuousEffects>(entity)
            .map_or_else(Vec::new, |effects| effects.0.clone()),
    })
}

pub(super) fn select_entities(
    world: &mut World,
    context: &EffectContext,
    selector: &Selector,
) -> Vec<GameEntityId> {
    let mut selected = match selector {
        Selector::Source => context.source.into_iter().collect(),
        Selector::DeclaredTarget => context.declared_target.into_iter().collect(),
        Selector::Entity(entity) => vec![*entity],
        Selector::InZone { player, zone } => world
            .resource::<ZoneIndex>()
            .entities(resolve_player(context.controller, *player), *zone)
            .to_vec(),
        Selector::FriendlyMinions
        | Selector::EnemyMinions
        | Selector::AllMinions
        | Selector::FriendlyCharacters
        | Selector::EnemyCharacters
        | Selector::AllCharacters => world
            .resource::<ZoneIndex>()
            .0
            .iter()
            .filter(|((player, zone), _)| {
                *zone == Zone::Play
                    && match selector {
                        Selector::FriendlyMinions | Selector::FriendlyCharacters => {
                            *player == context.controller
                        }
                        Selector::EnemyMinions | Selector::EnemyCharacters => {
                            *player == context.controller.opponent()
                        }
                        _ => true,
                    }
            })
            .flat_map(|(_, entities)| entities.iter().copied())
            .filter(|id| {
                let kind =
                    game_entity(world, *id).and_then(|entity| world.get::<EntityKind>(entity));
                match selector {
                    Selector::FriendlyMinions | Selector::EnemyMinions | Selector::AllMinions => {
                        kind == Some(&EntityKind::Minion)
                    }
                    _ => matches!(kind, Some(EntityKind::Hero | EntityKind::Minion)),
                }
            })
            .collect(),
        Selector::Random(inner) => {
            let candidates = select_entities(world, context, inner);
            choose_game_entity(world, candidates).into_iter().collect()
        }
    };
    selected.sort_unstable();
    selected.dedup();
    selected
}

pub(super) fn evaluate_value(
    world: &World,
    context: &EffectContext,
    expression: ValueExpression,
    target_count: usize,
) -> i32 {
    match expression {
        ValueExpression::Constant(value) => value,
        ValueExpression::SourceAttack => context
            .source
            .and_then(|source| game_entity(world, source))
            .and_then(|entity| world.get::<CurrentStats>(entity))
            .map_or(0, |stats| stats.attack),
        ValueExpression::TargetCount => target_count as i32,
    }
}

#[cfg(test)]
pub(super) fn modify_active_event_value(
    world: &mut World,
    context: &EffectContext,
    operation: EventValueOperation,
    expression: ValueExpression,
) -> Result<(), SimulationError> {
    modify_event_value(world, None, context, operation, expression)
}

pub(super) fn modify_event_value(
    world: &mut World,
    event: Option<EventId>,
    context: &EffectContext,
    operation: EventValueOperation,
    expression: ValueExpression,
) -> Result<(), SimulationError> {
    let event = event.ok_or(SimulationError::NoModifiableEventValue)?;
    let prepared = world
        .resource::<ResolutionWork>()
        .events
        .get(&event)
        .filter(|prepared| {
            matches!(
                prepared.context.kind,
                EventKind::ProposedDamage | EventKind::ProposedHealing
            ) && prepared.context.proposed_value.is_some()
        })
        .ok_or(SimulationError::NoModifiableEventValue)?;
    let previous = prepared
        .context
        .proposed_value
        .expect("modifiable event has a proposed value");
    let operand = evaluate_value(world, context, expression, prepared.context.targets.len());
    let current = match operation {
        EventValueOperation::Replace => operand,
        EventValueOperation::Add => previous.saturating_add(operand),
        EventValueOperation::Multiply => previous.saturating_mul(operand),
    }
    .max(0);
    world
        .resource_mut::<ResolutionWork>()
        .events
        .get_mut(&event)
        .expect("modifiable event still exists")
        .context
        .proposed_value = Some(current);
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::EventValueChanged {
            event,
            operation,
            previous,
            current,
        });
    Ok(())
}

pub(super) const fn resolve_player(controller: PlayerId, selector: PlayerSelector) -> PlayerId {
    match selector {
        PlayerSelector::Controller => controller,
        PlayerSelector::Opponent => controller.opponent(),
        PlayerSelector::Player(player) => player,
    }
}
