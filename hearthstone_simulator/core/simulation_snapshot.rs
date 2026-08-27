use bevy::prelude::*;

use crate::{
    Armor, AttackState, Controller, CurrentStats, Damage, DeathEventCache, DeathRecord,
    DefinitionId, DeterministicRng, DisplayName, EntityKind, GameEntityId, GameObject, GameState,
    PlayOrder, Player, PlayerId, RngSnapshot, Ruleset, RulesetId, Zone, ZonePosition,
    entity::{GameEntityIndex, game_entity},
    zone::ZoneIndex,
};

use super::player::player;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub entity: GameEntityId,
    pub id: PlayerId,
    pub name: String,
    pub health: i32,
    pub armor: i32,
    pub available_resources: i32,
    pub maximum_resources: i32,
    pub used_resources: i32,
    pub temporary_resources: i32,
    pub pending_overload: i32,
    pub locked_overload: i32,
    pub resources_spent: i32,
    pub fatigue: u32,
    pub hand: Vec<GameEntityId>,
    pub deck: Vec<GameEntityId>,
    pub board: Vec<GameEntityId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameObjectSnapshot {
    pub id: GameEntityId,
    pub definition_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub controller: PlayerId,
    pub zone: Zone,
    pub zone_position: usize,
    pub play_order: u64,
    pub attack: Option<i32>,
    pub maximum_health: Option<i32>,
    pub damage: i32,
    pub exhausted: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSnapshot {
    pub ruleset: RulesetId,
    pub game: GameState,
    pub players: Vec<PlayerSnapshot>,
    pub objects: Vec<GameObjectSnapshot>,
    pub deaths: Vec<DeathRecord>,
    pub rng: RngSnapshot,
}

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
            let hero = zones
                .entities(player_data.id, Zone::Play)
                .iter()
                .find_map(|id| {
                    let entity = game_entity(world, *id)?;
                    (world.get::<EntityKind>(entity) == Some(&EntityKind::Hero)).then_some(entity)
                })
                .expect("every player has a hero");
            PlayerSnapshot {
                entity: *game_id,
                id: player_data.id,
                name: player_data.name.clone(),
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
                board: zones.entities(player_data.id, Zone::Play).to_vec(),
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
            )| {
                GameObjectSnapshot {
                    id: *id,
                    definition_id: definition.0.clone(),
                    name: name.0.clone(),
                    kind: *kind,
                    controller: controller.0,
                    zone: *zone,
                    zone_position: position.0,
                    play_order: order.0,
                    attack: stats.map(|stats| stats.attack),
                    maximum_health: stats.map(|stats| stats.maximum_health),
                    damage: damage.map_or(0, |damage| damage.0),
                    exhausted: attack.map(|attack| attack.exhausted),
                }
            },
        )
        .collect::<Vec<_>>();
    objects.sort_by_key(|object| object.id);

    let rng = world.resource::<DeterministicRng>().state();
    GameSnapshot {
        ruleset,
        game,
        players,
        objects,
        deaths: world.resource::<DeathEventCache>().records.clone(),
        rng,
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
