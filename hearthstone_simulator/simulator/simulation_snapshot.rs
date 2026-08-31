use bevy::prelude::*;

use crate::{
    Armor, AttackState, Controller, CurrentStats, Damage, DeathEventCache, DefinitionId,
    DeterministicRng, DisplayName, DominantPlayer, EntityKind, GameEntityId, GameObject,
    GameObjectSnapshot, GameSnapshot, GameState, HeroMetadata, HeroPowerState, PlayOrder, Player,
    PlayerSnapshot, ResolutionWork, Ruleset, TurnSchedule, Zone, ZonePosition,
    entity::{GameEntityIndex, game_entity},
    zone::{ZoneIndex, board_entities, semantic_zone_position},
};

use super::player::player;

pub(super) fn build_snapshot(world: &mut World) -> GameSnapshot {
    let ruleset = world.resource::<Ruleset>().id;
    let game = world.resource::<GameState>().clone();
    let zones = world.resource::<ZoneIndex>().clone();
    let mut player_query = world.query::<(Entity, &GameEntityId, &Player)>();
    let mut players = player_query
        .iter(world)
        .map(|(_, game_id, player_data)| {
            let (_, _, stats, damage) =
                player(world, player_data.id).expect("every player has a hero");
            let (hero_id, hero) = zones
                .entities(player_data.id, Zone::Play)
                .iter()
                .find_map(|id| {
                    let entity = game_entity(world, *id)?;
                    (world.get::<EntityKind>(entity) == Some(&EntityKind::Hero))
                        .then_some((*id, entity))
                })
                .expect("every player has a hero");
            let hero_power = zones
                .entities(player_data.id, Zone::Play)
                .iter()
                .copied()
                .find(|id| {
                    game_entity(world, *id).is_some_and(|entity| {
                        world.get::<EntityKind>(entity) == Some(&EntityKind::HeroPower)
                    })
                });
            PlayerSnapshot {
                entity: *game_id,
                id: player_data.id,
                name: player_data.name.clone(),
                hero: hero_id,
                hero_power,
                hero_class: world
                    .get::<HeroMetadata>(hero)
                    .map(|metadata| metadata.class)
                    .unwrap_or_default(),
                health: stats.maximum_health - damage.0,
                armor: world.get::<Armor>(hero).map_or(0, |armor| armor.0),
                available_resources: player_data.available_resources(),
                maximum_resources: player_data.maximum_resources,
                used_resources: player_data.used_resources,
                temporary_resources: player_data.temporary_resources,
                pending_overload: player_data.pending_overload,
                locked_overload: player_data.locked_overload,
                resources_spent: player_data.resources_spent,
                fatigue: player_data.fatigue,
                hand: zones.entities(player_data.id, Zone::Hand).to_vec(),
                deck: zones.entities(player_data.id, Zone::Deck).to_vec(),
                board: board_entities(world, player_data.id),
            }
        })
        .collect::<Vec<_>>();
    players.sort_by_key(|player| player.id);

    let mut object_query = world.query::<(
        &GameEntityId,
        &DefinitionId,
        &DisplayName,
        &EntityKind,
        &Controller,
        &Zone,
        &ZonePosition,
        &PlayOrder,
        Option<&CurrentStats>,
        Option<&Damage>,
        Option<&AttackState>,
        Option<&HeroMetadata>,
        Option<&HeroPowerState>,
    )>();
    let mut objects = object_query
        .iter(world)
        .map(
            |(
                id,
                definition,
                name,
                kind,
                controller,
                zone,
                position,
                order,
                stats,
                damage,
                attack,
                hero,
                hero_power,
            )| {
                GameObjectSnapshot {
                    id: *id,
                    definition_id: definition.0.clone(),
                    name: name.0.clone(),
                    kind: *kind,
                    controller: controller.0,
                    zone: *zone,
                    zone_position: semantic_zone_position(world, *id, controller.0, *zone)
                        .unwrap_or(position.0),
                    play_order: order.0,
                    attack: stats.map(|stats| stats.attack),
                    maximum_health: stats.map(|stats| stats.maximum_health),
                    damage: damage.map_or(0, |damage| damage.0),
                    exhausted: attack.map(|attack| attack.exhausted),
                    hero_class: hero.map(|hero| hero.class),
                    hero_power_exhausted: hero_power.map(|power| power.exhausted),
                }
            },
        )
        .collect::<Vec<_>>();
    objects.sort_by_key(|object| object.id);

    let rng = world.resource::<DeterministicRng>().state();
    GameSnapshot {
        ruleset,
        game,
        turn_schedule: world.resource::<TurnSchedule>().clone(),
        dominant_player: world.resource::<DominantPlayer>().0,
        players,
        objects,
        deaths: world.resource::<DeathEventCache>().records.clone(),
        rng,
        resolution: world.resource::<ResolutionWork>().clone(),
    }
}

pub(super) fn assert_game_entity_index(world: &World) -> Result<(), String> {
    let index = world.resource::<GameEntityIndex>();
    for (id, entity) in &index.0 {
        if world.get::<GameObject>(*entity).is_none()
            || world.get::<GameEntityId>(*entity) != Some(id)
        {
            return Err(format!("game entity index disagrees for {id:?}"));
        }
    }
    let count = world
        .iter_entities()
        .filter(bevy::prelude::EntityRef::contains::<GameObject>)
        .count();
    if count != index.0.len() {
        return Err("not every GameObject is indexed".to_string());
    }
    Ok(())
}
