use googletest::prelude::*;

use super::{test_support::*, *};

#[googletest::test]
fn cards_keep_identity_when_played() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        })
        .expect("card should be playable");
    let snapshot = simulation.snapshot();

    assert_that!(snapshot.players[0].hand.is_empty(), is_true());
    assert_that!(snapshot.players[0].board.contains(&card), is_true());
    assert_that!(
        snapshot
            .objects
            .iter()
            .filter(|object| object.id == card)
            .count(),
        eq(1)
    );
    simulation
        .assert_invariants()
        .expect("invariants should hold");
}

#[googletest::test]
fn actions_use_stable_entity_targets() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        })
        .expect("card should be playable");
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .expect("turn should end");
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .expect("turn should end");
    let defender = hero(&mut simulation, PlayerId::Two);

    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker: card,
            defender,
        })
        .expect("minion should attack");

    assert_that!(simulation.snapshot().players[1].health, eq(27));
}

#[googletest::test]
fn accepted_actions_are_appended_in_chronological_order() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();

    let accepted = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::ActionAccepted { action, .. } => Some(action.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(accepted, eq(&["PlayCard", "EndTurn"]));
}

#[googletest::test]
fn negative_card_costs_are_floored_at_zero() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Discounted", -2, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let player = &simulation.snapshot().players[0];
    assert_that!(player.used_resources, eq(0));
    assert_that!(player.resources_spent, eq(0));
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::ResourceSpent {
                player: PlayerId::One,
                amount: 0,
            }
        )),
        is_true()
    );
}

#[googletest::test]
fn invalid_play_position_does_not_spend_mana_or_change_replay_state() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Invalid Position", 1, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let before = simulation.snapshot();

    assert_that!(
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: Some(999),
            choice: None,
        }),
        err(eq(&SimulationError::Zone(ZoneError::InvalidPosition {
            zone: Zone::Play,
            position: 999,
            length: 1,
        })))
    );

    assert_that!(simulation.snapshot(), eq(&before));
    assert_that!(simulation.fork().unwrap().snapshot(), eq(&before));
}

#[googletest::test]
fn play_zone_capacity_counts_minions_without_counting_the_hero() {
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            (0..7)
                .map(|index| Card::minion(format!("Minion {index}"), 0, 1, 1))
                .collect(),
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);

    for _ in 0..7 {
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .expect("all seven minion slots should be available");
    }

    let snapshot = simulation.snapshot();
    assert_that!(snapshot.players[0].board.len(), eq(8));
    assert_that!(
        snapshot
            .objects
            .iter()
            .filter(|object| {
                object.controller == PlayerId::One
                    && object.zone == Zone::Play
                    && object.kind == EntityKind::Minion
            })
            .count(),
        eq(7)
    );
    simulation.assert_invariants().unwrap();
}

#[googletest::test]
fn rejected_actions_leave_resolution_idle() {
    let mut simulation = simulation();
    let missing = GameEntityId(99_999);

    assert_that!(
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: missing,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::EntityNotFound(missing)))
    );
    assert_that!(
        simulation.snapshot().game.status,
        eq(SimulationStatus::AwaitingAction)
    );
    simulation
        .assert_invariants()
        .expect("invariants should hold");
}

#[googletest::test]
fn legal_actions_are_deterministic() {
    let mut simulation = simulation();
    let first = simulation.legal_actions();
    let second = simulation.legal_actions();

    assert_that!(first, eq(&second));
    assert_that!(first.len(), eq(2));
}

#[googletest::test]
fn action_validation_reports_each_rejection_and_concede_completes_game() {
    let mut wrong_turn = simulation();
    assert_that!(
        wrong_turn.apply(GameAction::EndTurn {
            player: PlayerId::Two,
        }),
        err(eq(&SimulationError::NotPlayersTurn(PlayerId::Two)))
    );

    let mut game_over = simulation();
    game_over
        .app
        .world_mut()
        .resource_mut::<GameState>()
        .outcome = Some(GameOutcome::Winner(PlayerId::Two));
    assert_that!(
        game_over.apply(GameAction::EndTurn {
            player: PlayerId::One,
        }),
        err(eq(&SimulationError::GameOver))
    );

    let mut busy = simulation();
    busy.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Resolving;
    assert_that!(
        busy.apply(GameAction::EndTurn {
            player: PlayerId::One,
        }),
        err(eq(&SimulationError::NotAwaitingAction))
    );
    assert_that!(busy.legal_actions().is_empty(), is_true());

    let mut invalid = simulation();
    invalid.app.update();
    let card = hand_card(&mut invalid, PlayerId::One);
    let own_hero = hero(&mut invalid, PlayerId::One);
    let opposing_hero = hero(&mut invalid, PlayerId::Two);
    assert_that!(
        invalid.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: opposing_hero,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NotControlled {
            entity: opposing_hero
        }))
    );
    assert_that!(
        invalid.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: own_hero,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::WrongZone {
            entity: own_hero,
            expected: Zone::Hand,
        }))
    );
    let card_entity = game_entity(invalid.app.world(), card).unwrap();
    invalid
        .app
        .world_mut()
        .entity_mut(card_entity)
        .insert(EntityKind::Weapon);
    assert_that!(
        invalid.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NotPlayable(card)))
    );

    let mut board_full = simulation();
    board_full
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .board_limit = 0;
    let card = hand_card(&mut board_full, PlayerId::One);
    assert_that!(
        board_full.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::BoardFull(PlayerId::One)))
    );

    let mut expensive = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::spell("Expensive", 2)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut expensive, PlayerId::One);
    assert_that!(
        expensive.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NotEnoughMana {
            player: PlayerId::One,
            required: 2,
            available: 1,
        }))
    );

    let mut concede = simulation();
    concede
        .apply(GameAction::Concede {
            player: PlayerId::One,
        })
        .unwrap();
    assert_that!(
        concede.snapshot().game.outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    assert_that!(
        concede.snapshot().game.status,
        eq(SimulationStatus::Complete)
    );
}

#[googletest::test]
fn legal_actions_ignore_stale_ids_and_deck_setup_spawns_cards() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::spell("Topdeck", 0)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    simulation
        .app
        .world_mut()
        .resource_mut::<ZoneIndex>()
        .0
        .insert((PlayerId::One, Zone::Hand), vec![GameEntityId(u64::MAX)]);

    assert_that!(
        simulation.legal_actions(),
        eq(&vec![GameAction::EndTurn {
            player: PlayerId::One
        }])
    );
    assert_that!(simulation.snapshot().players[0].deck.len(), eq(1));
}

#[googletest::test]
fn combat_checks_exhaustion_and_defenders_and_applies_counter_damage() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Attacker", 0, 2, 3)]),
        PlayerConfig::new("Rexxar", vec![Card::minion("Defender", 0, 1, 2)]),
    ]);
    let attacker = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: attacker,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    assert_that!(
        simulation.apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: enemy_hero,
        }),
        err(eq(&SimulationError::CannotAttack(attacker)))
    );
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    let defender = hand_card(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::Two,
            card: defender,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();
    let own_hero = hero(&mut simulation, PlayerId::One);
    assert_that!(
        simulation.apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: own_hero,
        }),
        err(eq(&SimulationError::InvalidDefender(own_hero)))
    );
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();

    let attacker_state = simulation
        .snapshot()
        .objects
        .into_iter()
        .find(|object| object.id == attacker)
        .unwrap();
    assert_that!(attacker_state.damage, eq(1));
}
