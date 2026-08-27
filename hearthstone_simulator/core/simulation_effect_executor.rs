use bevy::prelude::*;

use crate::{
    AttachedTo, AuraCache, BaseStats, CanonicalTrace, Card, Controller, CurrentStats, Damage,
    DefinitionId, DisplayName, Effect, EffectContext, EntityKind, EventContext, EventKind,
    EventValueOperation, GameEntityId, Keywords, NestedUnder, PendingDestroy, PlayerId,
    PlayerSelector, ResolutionCursor, ResolutionIdentity, ResolutionKind, Ruleset, RuntimeTriggers,
    Selector, StatModifier, TraceEntry, ValueExpression, Zone,
    enchantment::recalculate_stats,
    entity::{allocate_game_id, allocate_play_order, game_entity},
    native_effect::NativeEffectRegistry,
    resolver::{complete_active, consume_budget, push_resolution},
    rng::choose_game_entity,
    trigger::TriggersSuppressed,
    zone::{ZoneIndex, board_is_full, insert_into_zone, move_entity, validate_zone_position},
};

use super::{
    card_runtime::{CardRuntime, spawn_card},
    error::SimulationError,
    event_resolver::resolve_event_if_active,
    health::{
        DamageRequest, HealingRequest, SimultaneousEventOrder, apply_damage_batch,
        apply_healing_batch,
    },
    player::{draw_card, player_mut},
};

pub(super) fn execute_effects(
    world: &mut World,
    context: &EffectContext,
    effects: &[Effect],
) -> Result<(), SimulationError> {
    for effect in effects {
        push_resolution(world, ResolutionKind::Effect)?;
        consume_budget(world)?;
        let result = execute_effect(world, context, effect);
        complete_active(world)?;
        result?;
    }
    Ok(())
}

pub(super) fn execute_effect(
    world: &mut World,
    context: &EffectContext,
    effect: &Effect,
) -> Result<(), SimulationError> {
    match effect {
        Effect::DealDamage { targets, amount } => {
            let targets = select_entities(world, context, targets);
            let value = evaluate_value(world, context, *amount, targets.len());
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
            modify_active_event_value(world, context, *operation, *value)
        }
        Effect::Destroy { targets } => {
            for target in select_entities(world, context, targets) {
                if let Some(entity) = game_entity(world, target) {
                    world.entity_mut(entity).insert(PendingDestroy);
                }
            }
            Ok(())
        }
        Effect::Draw { player, count } => {
            let player = resolve_player(context.controller, *player);
            for _ in 0..*count {
                draw_card(world, player)?;
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
        Effect::Summon {
            player,
            card,
            board_index,
        } => {
            let player = resolve_player(context.controller, *player);
            if card.kind == EntityKind::Minion && board_is_full(world, player) {
                return Ok(());
            }
            validate_zone_position(world, player, Zone::Play, *board_index)?;
            let summoned = spawn_card(world, player, card.clone(), Zone::Play)?;
            if let Some(index) = board_index {
                move_entity(world, summoned, Zone::Play, Some(*index))?;
            }
            let order = allocate_play_order(world);
            let entity = game_entity(world, summoned).expect("summoned entity was indexed");
            world.entity_mut(entity).insert(order);
            resolve_event_if_active(
                world,
                EventContext {
                    kind: EventKind::Summoned,
                    source: context.source,
                    targets: vec![summoned],
                    controller: player,
                    proposed_value: None,
                    actual_value: None,
                    simultaneous_ordinal: 0,
                },
            )
        }
        Effect::AttachStatModifier { targets, modifier } => {
            for target in select_entities(world, context, targets) {
                attach_stat_modifier(world, context.controller, target, *modifier)?;
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
                if let Some(card) = copy_card_data(world, target) {
                    let _ = spawn_card(world, controller, card, *zone);
                }
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
            let event = nearest_active_event(world)
                .and_then(|entity| world.get::<EventContext>(entity))
                .map(|event| event.kind);
            validate_effect_program(world, &plan, event)?;
            execute_effects(world, context, &plan)
        }
        Effect::Sequence(nested) => execute_effects(world, context, nested),
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
                validate_effect_program(world, &card.effects, None)?;
                for trigger in &card.triggers {
                    validate_effect_program(world, &trigger.effect_program, Some(trigger.event))?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn attach_stat_modifier(
    world: &mut World,
    controller: PlayerId,
    target: GameEntityId,
    modifier: StatModifier,
) -> Result<(), SimulationError> {
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
    world
        .entity_mut(entity)
        .insert((AuraCache::default(), TriggersSuppressed));
    let enchantments = world
        .iter_entities()
        .filter_map(|candidate| {
            if candidate.get::<AttachedTo>().map(|attached| attached.0) == Some(entity)
                && candidate
                    .get::<StatModifier>()
                    .is_some_and(|modifier| modifier.silence_removable)
            {
                Some((*candidate.get::<GameEntityId>()?, candidate.id()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for (id, enchantment) in enchantments {
        world.entity_mut(enchantment).remove::<AttachedTo>();
        let _ = move_entity(world, id, Zone::RemovedFromGame, None);
    }
    recalculate_stats(world, target);
    Ok(())
}

pub(super) fn transform_entity(
    world: &mut World,
    target: GameEntityId,
    card: Card,
) -> Result<(), SimulationError> {
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
        Keywords::default(),
        CardRuntime {
            cost: card.mana_cost,
            program: card.effects,
        },
        RuntimeTriggers(card.triggers),
    ));
    world.entity_mut(entity).remove::<PendingDestroy>();
    world.entity_mut(entity).remove::<TriggersSuppressed>();
    Ok(())
}

pub(super) fn copy_card_data(world: &World, source: GameEntityId) -> Option<Card> {
    let entity = game_entity(world, source)?;
    let base = world.get::<BaseStats>(entity)?;
    let runtime = world.get::<CardRuntime>(entity)?;
    Some(Card {
        definition_id: world.get::<DefinitionId>(entity)?.0.clone(),
        name: world.get::<DisplayName>(entity)?.0.clone(),
        kind: *world.get::<EntityKind>(entity)?,
        mana_cost: runtime.cost,
        attack: base.attack,
        health: base.health,
        effects: runtime.program.clone(),
        triggers: world
            .get::<RuntimeTriggers>(entity)
            .map_or_else(Vec::new, |triggers| triggers.0.clone()),
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

fn nearest_active_event(world: &World) -> Option<Entity> {
    let mut current = world.resource::<ResolutionCursor>().active;
    while let Some(entity) = current {
        if world.get::<EventContext>(entity).is_some() {
            return Some(entity);
        }
        current = world.get::<NestedUnder>(entity).map(|parent| parent.0);
    }
    None
}

pub(super) fn modify_active_event_value(
    world: &mut World,
    context: &EffectContext,
    operation: EventValueOperation,
    expression: ValueExpression,
) -> Result<(), SimulationError> {
    let event_entity =
        nearest_active_event(world).ok_or(SimulationError::NoModifiableEventValue)?;
    let event = world
        .get::<EventContext>(event_entity)
        .filter(|event| {
            (event.kind == EventKind::ProposedDamage || event.kind == EventKind::ProposedHealing)
                && event.proposed_value.is_some()
        })
        .ok_or(SimulationError::NoModifiableEventValue)?;
    let previous = event
        .proposed_value
        .expect("modifiable event has a proposed value");
    let operand = evaluate_value(world, context, expression, event.targets.len());
    let current = match operation {
        EventValueOperation::Replace => operand,
        EventValueOperation::Add => previous.saturating_add(operand),
        EventValueOperation::Multiply => previous.saturating_mul(operand),
    }
    .max(0);
    world
        .get_mut::<EventContext>(event_entity)
        .expect("modifiable event still exists")
        .proposed_value = Some(current);
    let event = world
        .get::<ResolutionIdentity>(event_entity)
        .expect("event has a resolution identity")
        .id;
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
