use bevy::prelude::*;

use crate::{
    Armor, AttackState, BaseStats, Card, Controller, CurrentStats, Damage, DefinitionId,
    DisplayName, Effect, EntityKind, GameEntityId, GameObject, Keywords, PlayOrder, Player,
    PlayerConfig, PlayerId, RuntimeAuras, RuntimeContinuousEffects, RuntimeTriggers,
    STARTING_HEALTH, Zone, entity::allocate_game_id, zone::insert_into_zone,
};

use super::error::SimulationError;

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub(super) struct CardRuntime {
    pub(super) cost: i32,
    pub(super) program: Vec<Effect>,
}

pub(super) fn setup_game(
    world: &mut World,
    players: [PlayerConfig; 2],
) -> Result<(), SimulationError> {
    for (index, config) in players.into_iter().enumerate() {
        let id = PlayerId::ALL[index];
        let starts = id == PlayerId::One;
        spawn_player(world, id, &config.name, starts)?;
        for card in config.deck {
            spawn_card(world, id, card, Zone::Deck)?;
        }
        for card in config.hand {
            spawn_card(world, id, card, Zone::Hand)?;
        }
    }
    Ok(())
}

fn spawn_player(
    world: &mut World,
    player_id: PlayerId,
    name: &str,
    starts: bool,
) -> Result<(), SimulationError> {
    let player_object_id = allocate_game_id(world);
    world.spawn((
        GameObject,
        player_object_id,
        DefinitionId("system:player".to_string()),
        EntityKind::Player,
        Controller(player_id),
        DisplayName(name.to_string()),
        PlayOrder::default(),
        Player {
            id: player_id,
            name: name.to_string(),
            maximum_resources: i32::from(starts),
            used_resources: 0,
            temporary_resources: 0,
            pending_overload: 0,
            locked_overload: 0,
            resources_spent: 0,
            fatigue: 0,
        },
    ));
    insert_into_zone(world, player_object_id, player_id, Zone::SetAside, None)?;

    let hero_id = allocate_game_id(world);
    world.spawn((
        GameObject,
        hero_id,
        DefinitionId("system:hero".to_string()),
        EntityKind::Hero,
        Controller(player_id),
        DisplayName(format!("{name}'s Hero")),
        PlayOrder::default(),
        BaseStats {
            attack: 0,
            health: STARTING_HEALTH,
        },
        CurrentStats {
            attack: 0,
            maximum_health: STARTING_HEALTH,
        },
        Damage::default(),
        Armor::default(),
        AttackState::default(),
        Keywords::default(),
    ));
    insert_into_zone(world, hero_id, player_id, Zone::Play, None)?;
    Ok(())
}

pub(super) fn spawn_card(
    world: &mut World,
    player_id: PlayerId,
    card: Card,
    zone: Zone,
) -> Result<GameEntityId, SimulationError> {
    let id = allocate_game_id(world);
    let entity = world
        .spawn((
            GameObject,
            id,
            DefinitionId(card.definition_id),
            card.kind,
            Controller(player_id),
            DisplayName(card.name),
            PlayOrder::default(),
            BaseStats {
                attack: card.attack,
                health: card.health,
            },
            CurrentStats {
                attack: card.attack,
                maximum_health: card.health,
            },
            Damage::default(),
            AttackState {
                attacks_this_turn: 0,
                exhausted: true,
            },
            Keywords::default(),
            CardRuntime {
                cost: card.mana_cost,
                program: card.effects,
            },
            RuntimeTriggers(card.triggers),
        ))
        .id();
    world.entity_mut(entity).insert((
        RuntimeAuras(card.auras),
        RuntimeContinuousEffects(card.continuous_effects),
    ));
    if let Err(error) = insert_into_zone(world, id, player_id, zone, None) {
        world.despawn(entity);
        return Err(error.into());
    }
    Ok(id)
}
