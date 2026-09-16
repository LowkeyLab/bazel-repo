use bevy::prelude::*;

use crate::{
    CanonicalTrace, Card, Controller, CurrentStats, EntityKind, GameEntityId, GameState, Keyword,
    PlayerId, ResolutionOp, ResolutionWork, SequenceStep, SimulationError, TraceEntry,
    WeaponEquipment, WeaponState, Zone, ZoneMoveOutcome, ZoneMoveRequest, ZoneMovementKind,
    entity::{allocate_play_order, game_entity},
    zone::{ZoneIndex, move_entity, move_entity_with_request},
};

pub(crate) fn validate_card(card: &Card) -> Result<(), SimulationError> {
    if card.kind == EntityKind::Weapon && card.durability <= 0 {
        return Err(SimulationError::Invariant(
            "weapon base durability must be positive".into(),
        ));
    }
    Ok(())
}

pub(crate) fn active(world: &World, player: PlayerId) -> Option<GameEntityId> {
    let id = *world
        .get_resource::<WeaponEquipment>()?
        .active
        .get(&player)?;
    let entity = game_entity(world, id)?;
    (world.get::<Zone>(entity) == Some(&Zone::Play)
        && world.get::<EntityKind>(entity) == Some(&EntityKind::Weapon)
        && world.get::<Controller>(entity) == Some(&Controller(player)))
    .then_some(id)
}

pub(crate) fn effective_attack(world: &World, entity: Entity) -> i32 {
    let own = world
        .get::<CurrentStats>(entity)
        .map_or(0, |stats| stats.attack);
    own.saturating_add(
        attack_weapon(world, entity)
            .and_then(|(_, id)| game_entity(world, id))
            .and_then(|weapon| world.get::<CurrentStats>(weapon))
            .map_or(0, |stats| stats.attack),
    )
}

// Attack contribution and durability payment share the live Hero/controller decision.
pub(crate) fn attack_weapon(world: &World, entity: Entity) -> Option<(PlayerId, GameEntityId)> {
    if world.get::<EntityKind>(entity) != Some(&EntityKind::Hero) {
        return None;
    }
    let controller = world.get::<Controller>(entity)?.0;
    if world.resource::<GameState>().active_player != controller {
        return None;
    }
    active(world, controller).map(|weapon| (controller, weapon))
}

pub(crate) fn grants_windfury(world: &World, entity: Entity) -> bool {
    attack_weapon(world, entity)
        .and_then(|(_, weapon)| game_entity(world, weapon))
        .is_some_and(|weapon| crate::aura::has_keyword(world, weapon, Keyword::Windfury))
}

// Movement never promotes an older, superseded weapon back into the active slot.
pub(crate) fn track_entry(world: &mut World, id: GameEntityId) {
    let Some(entity) = game_entity(world, id) else {
        return;
    };
    let controller = world.get::<Controller>(entity).map(|c| c.0);
    let in_play = world.get::<Zone>(entity) == Some(&Zone::Play)
        && world.get::<EntityKind>(entity) == Some(&EntityKind::Weapon);
    let Some(mut equipment) = world.get_resource_mut::<WeaponEquipment>() else {
        return;
    };
    equipment
        .active
        .retain(|player, weapon| *weapon != id || (in_play && Some(*player) == controller));
    if in_play
        && !equipment.pending.values().any(|old| *old == Some(id))
        && let Some(player) = controller
    {
        equipment.active.entry(player).or_insert(id);
    }
}

pub(crate) fn begin_equip(
    world: &mut World,
    player: PlayerId,
    weapon: GameEntityId,
) -> Result<(), SimulationError> {
    let previous = active(world, player);
    if previous == Some(weapon)
        || world
            .resource::<WeaponEquipment>()
            .pending
            .contains_key(&weapon)
    {
        return Err(SimulationError::Invariant(
            "weapon already being equipped".into(),
        ));
    }
    world
        .resource_mut::<WeaponEquipment>()
        .pending
        .insert(weapon, previous);
    let result = move_entity(world, weapon, Zone::Play, None);
    let Ok(ZoneMoveOutcome::Moved { from, .. }) = result else {
        world
            .resource_mut::<WeaponEquipment>()
            .pending
            .remove(&weapon);
        return Err(SimulationError::Invariant(format!(
            "equip move failed: {result:?}"
        )));
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: weapon,
            from,
            to: Zone::Play,
        });
    world
        .resource_mut::<WeaponEquipment>()
        .active
        .insert(player, weapon);
    let order = allocate_play_order(world);
    let entity = game_entity(world, weapon).expect("equipped entity exists");
    world.entity_mut(entity).insert(order);
    Ok(())
}

pub(crate) fn finish_equip(world: &mut World, weapon: GameEntityId) -> Result<(), SimulationError> {
    let previous = world
        .resource_mut::<WeaponEquipment>()
        .pending
        .remove(&weapon)
        .ok_or_else(|| SimulationError::Invariant("missing weapon replacement scope".into()))?;
    if let Some(previous) = previous
        && let Some(entity) = game_entity(world, previous)
        && world.get::<Zone>(entity) == Some(&Zone::Play)
        && world.get::<EntityKind>(entity) == Some(&EntityKind::Weapon)
    {
        // A superseded weapon remains an observer until replacement finishes. Moving now
        // frees the slot, while its Death Event joins the next ordinary death boundary.
        let position = retire_weapon(world, previous)?;
        crate::death::record_full_zone_death(world, previous, position);
    }
    Ok(())
}

pub(crate) fn consume_durability(world: &mut World, player: PlayerId, weapon: GameEntityId) {
    if active(world, player) != Some(weapon) {
        return;
    }
    let Some(entity) = game_entity(world, weapon) else {
        return;
    };
    let Some(mut state) = world.get_mut::<WeaponState>(entity) else {
        return;
    };
    let previous = state.durability;
    state.durability = state.durability.saturating_sub(1).max(0);
    let current = state.durability;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::WeaponDurability {
            weapon,
            previous,
            current,
        });
}

pub(crate) fn assert_invariants(world: &World) -> Result<(), String> {
    let equipment = world.resource::<WeaponEquipment>();
    for (player, id) in &equipment.active {
        if active(world, *player) != Some(*id) {
            return Err("invalid active weapon reference".into());
        }
    }
    for op in &world.resource::<ResolutionWork>().stack {
        if let ResolutionOp::RunSequenceStep(SequenceStep::FinishEquip { weapon }) = &op.operation
            && !equipment.pending.contains_key(weapon)
        {
            return Err("equip completion lacks a scope".into());
        }
    }
    for (new, old) in &equipment.pending {
        let Some(entity) = game_entity(world, *new) else {
            return Err("missing equip source".into());
        };
        if old == &Some(*new)
            || old.is_some_and(|id| game_entity(world, id).is_none())
            || world.get::<EntityKind>(entity) != Some(&EntityKind::Weapon)
            || world.get::<WeaponState>(entity).is_none()
            || old.is_some_and(|id| {
                game_entity(world, id)
                    .is_none_or(|e| world.get::<EntityKind>(e) != Some(&EntityKind::Weapon))
            })
            || world.get::<Controller>(entity).is_none()
        {
            return Err("invalid replacement scope".into());
        }
        if let Some(old_entity) = old.and_then(|id| game_entity(world, id))
            && world.get::<Controller>(old_entity) != world.get::<Controller>(entity)
        {
            return Err("replacement weapons must share the same controller".into());
        }
        let completions = world
            .resource::<ResolutionWork>()
            .stack
            .iter()
            .filter(|op| {
                matches!(&op.operation,
            ResolutionOp::RunSequenceStep(SequenceStep::FinishEquip { weapon }) if weapon == new)
            })
            .count();
        if completions != 1 {
            return Err("replacement scope must own one completion".into());
        }
    }
    for player in PlayerId::ALL {
        for id in world.resource::<ZoneIndex>().entities(player, Zone::Play) {
            let Some(entity) = game_entity(world, *id) else {
                continue;
            };
            if world.get::<EntityKind>(entity) == Some(&EntityKind::Weapon) {
                if world.get::<WeaponState>(entity).is_none() {
                    return Err("weapon lacks durability".into());
                }
                if active(world, player) != Some(*id)
                    && !equipment.pending.values().any(|old| *old == Some(*id))
                {
                    return Err("extra weapon lacks a replacement scope".into());
                }
            }
        }
    }
    Ok(())
}

// A failed sequence preserves gameplay mutations but must release its replacement scopes.
// Retire superseded sources without running additional effects after the failure.
pub(crate) fn abandon_equips(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<WeaponEquipment>().pending);
    for old in pending.values().flatten() {
        let Some(entity) = game_entity(world, *old) else {
            continue;
        };
        if world.get::<Zone>(entity) == Some(&Zone::Play)
            && world.get::<EntityKind>(entity) == Some(&EntityKind::Weapon)
            && !world
                .resource::<WeaponEquipment>()
                .active
                .values()
                .any(|id| id == old)
        {
            let _ = retire_weapon(world, *old);
        }
    }
}

// Retirement uses the same runtime/attachment reset as ordinary death, but event
// recording is left to the caller so failed sequences cannot schedule more effects.
fn retire_weapon(world: &mut World, weapon: GameEntityId) -> Result<usize, SimulationError> {
    let entity = game_entity(world, weapon).ok_or(SimulationError::EntityNotFound(weapon))?;
    let controller = world
        .get::<Controller>(entity)
        .ok_or(SimulationError::EntityNotFound(weapon))?
        .0;
    let position =
        crate::zone::semantic_zone_position(world, weapon, controller, Zone::Play).unwrap_or(0);
    let outcome = move_entity_with_request(
        world,
        ZoneMoveRequest {
            entity: weapon,
            destination_controller: controller,
            destination: Zone::Graveyard,
            position: None,
            kind: ZoneMovementKind::Death,
        },
    )?;
    let ZoneMoveOutcome::Moved { from, .. } = outcome else {
        return Err(SimulationError::Invariant(format!(
            "weapon retirement failed: {outcome:?}"
        )));
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: weapon,
            from,
            to: Zone::Graveyard,
        });
    Ok(position)
}
