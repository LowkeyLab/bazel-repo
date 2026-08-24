use std::collections::VecDeque;

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    Card, GameState, MAX_BOARD_SIZE, MAX_MANA, Minion, MinionCard, MinionId, Player, PlayerConfig,
    PlayerId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GameAction {
    PlayCard {
        player: PlayerId,
        hand_index: usize,
    },
    AttackHero {
        player: PlayerId,
        attacker: MinionId,
    },
    EndTurn {
        player: PlayerId,
    },
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SimulationError {
    #[error("the game is already over")]
    GameOver,
    #[error("it is not {0:?}'s turn")]
    NotPlayersTurn(PlayerId),
    #[error("player {0:?} does not exist")]
    PlayerNotFound(PlayerId),
    #[error("card index {hand_index} does not exist for player {player:?}")]
    CardNotFound { player: PlayerId, hand_index: usize },
    #[error("player {player:?} needs {required} mana but only has {available}")]
    NotEnoughMana {
        player: PlayerId,
        required: u8,
        available: u8,
    },
    #[error("player {0:?}'s board is full")]
    BoardFull(PlayerId),
    #[error("minion {0:?} does not exist")]
    MinionNotFound(MinionId),
    #[error("minion {minion:?} does not belong to player {player:?}")]
    MinionNotOwned { player: PlayerId, minion: MinionId },
    #[error("minion {0:?} cannot attack")]
    MinionExhausted(MinionId),
    #[error("the simulation did not produce an action result")]
    MissingActionResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSnapshot {
    pub id: PlayerId,
    pub name: String,
    pub health: i32,
    pub mana: u8,
    pub max_mana: u8,
    pub hand: Vec<Card>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinionSnapshot {
    pub id: MinionId,
    pub owner: PlayerId,
    pub name: String,
    pub attack: i32,
    pub health: i32,
    pub can_attack: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameSnapshot {
    pub game: GameState,
    pub players: Vec<PlayerSnapshot>,
    pub minions: Vec<MinionSnapshot>,
}

#[derive(Default, Resource)]
struct PendingActions(VecDeque<GameAction>);

#[derive(Default, Resource)]
struct ActionResults(VecDeque<Result<(), SimulationError>>);

#[derive(Default, Resource)]
struct NextMinionId(u32);

pub struct HearthstoneSimulationPlugin;

impl Plugin for HearthstoneSimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameState>()
            .init_resource::<PendingActions>()
            .init_resource::<ActionResults>()
            .init_resource::<NextMinionId>()
            .add_systems(Update, process_next_action);
    }
}

pub struct Simulation {
    app: App,
}

impl Simulation {
    pub fn new(players: [PlayerConfig; 2]) -> Self {
        let [player_one, player_two] = players;
        let mut app = App::new();
        app.add_plugins(HearthstoneSimulationPlugin);
        app.world_mut()
            .spawn(Player::from_config(PlayerId::One, player_one, true));
        app.world_mut()
            .spawn(Player::from_config(PlayerId::Two, player_two, false));
        Self { app }
    }

    pub fn apply(&mut self, action: GameAction) -> Result<(), SimulationError> {
        self.app
            .world_mut()
            .resource_mut::<PendingActions>()
            .0
            .push_back(action);
        self.app.update();
        self.app
            .world_mut()
            .resource_mut::<ActionResults>()
            .0
            .pop_front()
            .ok_or(SimulationError::MissingActionResult)?
    }

    pub fn snapshot(&mut self) -> GameSnapshot {
        let world = self.app.world_mut();
        let game = world.resource::<GameState>().clone();

        let mut player_query = world.query::<&Player>();
        let mut players = player_query
            .iter(world)
            .map(|player| PlayerSnapshot {
                id: player.id,
                name: player.name.clone(),
                health: player.health,
                mana: player.mana,
                max_mana: player.max_mana,
                hand: player.hand.clone(),
            })
            .collect::<Vec<_>>();
        players.sort_by_key(|player| player.id);

        let mut minion_query = world.query::<&Minion>();
        let mut minions = minion_query
            .iter(world)
            .map(|minion| MinionSnapshot {
                id: minion.id,
                owner: minion.owner,
                name: minion.name.clone(),
                attack: minion.attack,
                health: minion.health,
                can_attack: minion.can_attack,
            })
            .collect::<Vec<_>>();
        minions.sort_by_key(|minion| minion.id);

        GameSnapshot {
            game,
            players,
            minions,
        }
    }
}

fn process_next_action(world: &mut World) {
    let Some(action) = world.resource_mut::<PendingActions>().0.pop_front() else {
        return;
    };
    let result = apply_action(world, action);
    world.resource_mut::<ActionResults>().0.push_back(result);
}

fn apply_action(world: &mut World, action: GameAction) -> Result<(), SimulationError> {
    if world.resource::<GameState>().winner.is_some() {
        return Err(SimulationError::GameOver);
    }

    match action {
        GameAction::PlayCard { player, hand_index } => play_card(world, player, hand_index),
        GameAction::AttackHero { player, attacker } => attack_hero(world, player, attacker),
        GameAction::EndTurn { player } => end_turn(world, player),
    }
}

fn validate_turn(world: &World, player: PlayerId) -> Result<(), SimulationError> {
    if world.resource::<GameState>().active_player == player {
        Ok(())
    } else {
        Err(SimulationError::NotPlayersTurn(player))
    }
}

fn player_entity(world: &mut World, id: PlayerId) -> Result<Entity, SimulationError> {
    let mut query = world.query::<(Entity, &Player)>();
    query
        .iter(world)
        .find_map(|(entity, player)| (player.id == id).then_some(entity))
        .ok_or(SimulationError::PlayerNotFound(id))
}

fn minion_entity(world: &mut World, id: MinionId) -> Result<Entity, SimulationError> {
    let mut query = world.query::<(Entity, &Minion)>();
    query
        .iter(world)
        .find_map(|(entity, minion)| (minion.id == id).then_some(entity))
        .ok_or(SimulationError::MinionNotFound(id))
}

fn play_card(
    world: &mut World,
    player_id: PlayerId,
    hand_index: usize,
) -> Result<(), SimulationError> {
    validate_turn(world, player_id)?;

    let mut minion_query = world.query::<&Minion>();
    let board_size = minion_query
        .iter(world)
        .filter(|minion| minion.owner == player_id)
        .count();
    if board_size >= MAX_BOARD_SIZE {
        return Err(SimulationError::BoardFull(player_id));
    }

    let player_entity = player_entity(world, player_id)?;
    let card = world
        .get::<Player>(player_entity)
        .and_then(|player| player.hand.get(hand_index))
        .cloned()
        .ok_or(SimulationError::CardNotFound {
            player: player_id,
            hand_index,
        })?;
    let available = world
        .get::<Player>(player_entity)
        .map(|player| player.mana)
        .ok_or(SimulationError::PlayerNotFound(player_id))?;
    let required = card.mana_cost();
    if available < required {
        return Err(SimulationError::NotEnoughMana {
            player: player_id,
            required,
            available,
        });
    }

    let Card::Minion(MinionCard {
        name,
        attack,
        health,
        ..
    }) = card;
    let minion_id = {
        let mut next = world.resource_mut::<NextMinionId>();
        let id = MinionId(next.0);
        next.0 += 1;
        id
    };
    let mut player = world
        .get_mut::<Player>(player_entity)
        .ok_or(SimulationError::PlayerNotFound(player_id))?;
    player.mana -= required;
    player.hand.remove(hand_index);
    world.spawn(Minion {
        id: minion_id,
        owner: player_id,
        name,
        attack,
        health,
        can_attack: false,
    });
    Ok(())
}

fn attack_hero(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: MinionId,
) -> Result<(), SimulationError> {
    validate_turn(world, player_id)?;
    let attacker_entity = minion_entity(world, attacker_id)?;
    let attacker = world
        .get::<Minion>(attacker_entity)
        .ok_or(SimulationError::MinionNotFound(attacker_id))?;
    if attacker.owner != player_id {
        return Err(SimulationError::MinionNotOwned {
            player: player_id,
            minion: attacker_id,
        });
    }
    if !attacker.can_attack {
        return Err(SimulationError::MinionExhausted(attacker_id));
    }
    let damage = attacker.attack;

    world
        .get_mut::<Minion>(attacker_entity)
        .ok_or(SimulationError::MinionNotFound(attacker_id))?
        .can_attack = false;
    let defender_id = player_id.opponent();
    let defender_entity = player_entity(world, defender_id)?;
    let mut defender = world
        .get_mut::<Player>(defender_entity)
        .ok_or(SimulationError::PlayerNotFound(defender_id))?;
    defender.health = defender.health.saturating_sub(damage);
    if defender.health <= 0 {
        world.resource_mut::<GameState>().winner = Some(player_id);
    }
    Ok(())
}

fn end_turn(world: &mut World, player_id: PlayerId) -> Result<(), SimulationError> {
    validate_turn(world, player_id)?;
    let next_player = player_id.opponent();
    {
        let mut game = world.resource_mut::<GameState>();
        game.active_player = next_player;
        game.turn_number += 1;
    }

    let next_player_entity = player_entity(world, next_player)?;
    let mut player = world
        .get_mut::<Player>(next_player_entity)
        .ok_or(SimulationError::PlayerNotFound(next_player))?;
    player.max_mana = (player.max_mana + 1).min(MAX_MANA);
    player.mana = player.max_mana;

    let mut minion_query = world.query::<&mut Minion>();
    for mut minion in minion_query.iter_mut(world) {
        if minion.owner == next_player {
            minion.can_attack = true;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulation() -> Simulation {
        Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Training Minion", 1, 3, 2)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ])
    }

    #[test]
    fn starts_with_player_one_and_one_mana() {
        let mut simulation = simulation();

        let snapshot = simulation.snapshot();

        assert_eq!(snapshot.game.active_player, PlayerId::One);
        assert_eq!(snapshot.players[0].mana, 1);
        assert_eq!(snapshot.players[1].mana, 0);
    }

    #[test]
    fn playing_a_minion_spends_mana_and_spawns_an_entity() {
        let mut simulation = simulation();

        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            })
            .expect("player one should be able to play the card");
        let snapshot = simulation.snapshot();

        assert_eq!(snapshot.players[0].mana, 0);
        assert!(snapshot.players[0].hand.is_empty());
        assert_eq!(snapshot.minions.len(), 1);
        assert!(!snapshot.minions[0].can_attack);
    }

    #[test]
    fn rejects_actions_from_the_inactive_player() {
        let mut simulation = simulation();

        let result = simulation.apply(GameAction::EndTurn {
            player: PlayerId::Two,
        });

        assert_eq!(result, Err(SimulationError::NotPlayersTurn(PlayerId::Two)));
    }

    #[test]
    fn minion_can_attack_on_its_owners_next_turn() {
        let mut simulation = simulation();
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            })
            .expect("the minion should be playable");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .expect("player one should be able to end the turn");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .expect("player two should be able to end the turn");

        simulation
            .apply(GameAction::AttackHero {
                player: PlayerId::One,
                attacker: MinionId(0),
            })
            .expect("the minion should be ready");
        let snapshot = simulation.snapshot();

        assert_eq!(snapshot.players[1].health, 27);
        assert!(!snapshot.minions[0].can_attack);
    }

    #[test]
    fn update_without_an_action_is_a_noop() {
        let mut simulation = simulation();

        simulation.app.update();

        assert_eq!(simulation.snapshot().game, GameState::default());
    }

    #[test]
    fn rejects_missing_and_unaffordable_cards() {
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Giant", 10, 8, 8)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);

        assert_eq!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 1,
            }),
            Err(SimulationError::CardNotFound {
                player: PlayerId::One,
                hand_index: 1,
            })
        );
        assert_eq!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            }),
            Err(SimulationError::NotEnoughMana {
                player: PlayerId::One,
                required: 10,
                available: 1,
            })
        );
    }

    #[test]
    fn rejects_playing_a_minion_on_a_full_board() {
        let cards = (0..=MAX_BOARD_SIZE)
            .map(|index| Card::minion(format!("Minion {index}"), 0, 1, 1))
            .collect();
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", cards),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        for _ in 0..MAX_BOARD_SIZE {
            simulation
                .apply(GameAction::PlayCard {
                    player: PlayerId::One,
                    hand_index: 0,
                })
                .expect("a board slot should be available");
        }

        assert_eq!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            }),
            Err(SimulationError::BoardFull(PlayerId::One))
        );
    }

    #[test]
    fn rejects_invalid_minion_attacks() {
        let mut simulation = simulation();

        assert_eq!(
            simulation.apply(GameAction::AttackHero {
                player: PlayerId::One,
                attacker: MinionId(99),
            }),
            Err(SimulationError::MinionNotFound(MinionId(99)))
        );
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            })
            .expect("the minion should be playable");
        assert_eq!(
            simulation.apply(GameAction::AttackHero {
                player: PlayerId::One,
                attacker: MinionId(0),
            }),
            Err(SimulationError::MinionExhausted(MinionId(0)))
        );
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .expect("player one should be able to end the turn");
        assert_eq!(
            simulation.apply(GameAction::AttackHero {
                player: PlayerId::Two,
                attacker: MinionId(0),
            }),
            Err(SimulationError::MinionNotOwned {
                player: PlayerId::Two,
                minion: MinionId(0),
            })
        );
    }

    #[test]
    fn winning_the_game_rejects_further_actions() {
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![Card::minion("Finisher", 1, 30, 1)]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                hand_index: 0,
            })
            .expect("the minion should be playable");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .expect("player one should be able to end the turn");
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::Two,
            })
            .expect("player two should be able to end the turn");
        simulation
            .apply(GameAction::AttackHero {
                player: PlayerId::One,
                attacker: MinionId(0),
            })
            .expect("the attack should win the game");

        assert_eq!(simulation.snapshot().game.winner, Some(PlayerId::One));
        assert_eq!(
            simulation.apply(GameAction::EndTurn {
                player: PlayerId::One,
            }),
            Err(SimulationError::GameOver)
        );
    }
}
