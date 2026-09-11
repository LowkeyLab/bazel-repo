use googletest::prelude::*;

use super::{test_support::*, *};
use crate::{
    AttackState, CurrentStats, Player, TargetAudience, TargetFilter, TargetKind, TargetRequirement,
};

fn card_named(simulation: &mut Simulation, name: &str) -> GameEntityId {
    simulation
        .snapshot()
        .objects
        .into_iter()
        .find(|object| object.name == name)
        .unwrap()
        .id
}

fn play_declaration(
    card: GameEntityId,
    target: Option<GameEntityId>,
    board_index: Option<usize>,
    choice: Option<ChoiceId>,
) -> GameAction {
    GameAction::PlayCard {
        player: PlayerId::One,
        card,
        target,
        board_index,
        choice,
    }
}

fn assert_rejected_action_is_atomic(
    simulation: &mut Simulation,
    action: GameAction,
    expected: SimulationError,
) {
    let player = action.player();
    let before = simulation.checkpoint().unwrap();

    assert_eq!(simulation.apply(action), Err(expected));

    let mut after = simulation.checkpoint().unwrap();
    assert!(matches!(
        after.trace.entries.pop(),
        Some(TraceEntry::ActionRejected {
            player: rejected_player,
            ..
        }) if rejected_player == player
    ));
    assert_eq!(after, before);
}

#[googletest::test]
fn untargeted_play_rejects_a_supplied_target() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);

    assert!(
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: Some(target),
                board_index: None,
                choice: None,
            })
            .is_err()
    );
}

#[googletest::test]
fn target_options_follow_the_requirement_truth_table() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let minion_filter = TargetFilter {
        audience: TargetAudience::Friendly,
        kind: TargetKind::Minion,
    };

    let empty_cases = [
        (TargetRequirement::None, vec![None]),
        (TargetRequirement::Required(minion_filter), Vec::new()),
        (TargetRequirement::Optional(minion_filter), vec![None]),
        (
            TargetRequirement::RequiredIfAvailable(minion_filter),
            vec![None],
        ),
    ];
    for (requirement, expected) in empty_cases {
        assert_eq!(
            action_validation::target_options(simulation.app.world(), PlayerId::One, requirement,),
            expected,
        );
    }

    let first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("First", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Second", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let populated_cases = [
        (TargetRequirement::None, vec![None]),
        (
            TargetRequirement::Required(minion_filter),
            vec![Some(first), Some(second)],
        ),
        (
            TargetRequirement::Optional(minion_filter),
            vec![None, Some(first), Some(second)],
        ),
        (
            TargetRequirement::RequiredIfAvailable(minion_filter),
            vec![Some(first), Some(second)],
        ),
    ];
    for (requirement, expected) in populated_cases {
        assert_eq!(
            action_validation::target_options(simulation.app.world(), PlayerId::One, requirement,),
            expected,
        );
    }
}

#[googletest::test]
fn eligible_targets_filter_audience_kind_zone_and_stale_entries() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let friendly_hero = hero(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    let friendly_first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Friendly First", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let friendly_second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Friendly Second", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let enemy_first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy First", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let enemy_second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy Second", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let wrong_zone = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Wrong Zone", 0, 1, 1),
        Zone::Hand,
    )
    .unwrap();
    simulation
        .app
        .world_mut()
        .resource_mut::<ZoneIndex>()
        .0
        .get_mut(&(PlayerId::One, Zone::Play))
        .unwrap()
        .extend([friendly_second, wrong_zone, GameEntityId(u64::MAX)]);

    let cases = [
        (
            TargetFilter {
                audience: TargetAudience::Friendly,
                kind: TargetKind::Hero,
            },
            vec![friendly_hero],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Hero,
            },
            vec![enemy_hero],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Either,
                kind: TargetKind::Hero,
            },
            vec![friendly_hero, enemy_hero],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Friendly,
                kind: TargetKind::Minion,
            },
            vec![friendly_first, friendly_second],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Minion,
            },
            vec![enemy_first, enemy_second],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Either,
                kind: TargetKind::Minion,
            },
            vec![friendly_first, friendly_second, enemy_first, enemy_second],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Friendly,
                kind: TargetKind::Character,
            },
            vec![friendly_hero, friendly_first, friendly_second],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Character,
            },
            vec![enemy_hero, enemy_first, enemy_second],
        ),
        (
            TargetFilter {
                audience: TargetAudience::Either,
                kind: TargetKind::Character,
            },
            vec![
                friendly_hero,
                enemy_hero,
                friendly_first,
                friendly_second,
                enemy_first,
                enemy_second,
            ],
        ),
    ];
    for (filter, expected) in cases {
        assert_eq!(
            action_validation::eligible_targets(simulation.app.world(), PlayerId::One, filter),
            expected,
        );
    }
}

#[googletest::test]
fn play_declarations_report_target_errors_and_normalize_positions() {
    let enemy_character = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let enemy_minion = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Minion,
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::spell("Untargeted", 0),
                Card::spell("Required", 0)
                    .with_targeting(TargetRequirement::Required(enemy_character)),
                Card::spell("Optional", 0)
                    .with_targeting(TargetRequirement::Optional(enemy_character)),
                Card::spell("Conditional", 0)
                    .with_targeting(TargetRequirement::RequiredIfAvailable(enemy_minion)),
                Card::minion("Positioned", 0, 1, 1),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let untargeted = card_named(&mut simulation, "Untargeted");
    let required = card_named(&mut simulation, "Required");
    let optional = card_named(&mut simulation, "Optional");
    let conditional = card_named(&mut simulation, "Conditional");
    let positioned = card_named(&mut simulation, "Positioned");
    let friendly_hero = hero(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    let stale = GameEntityId(u64::MAX);

    let cases = [
        (
            play_declaration(untargeted, Some(enemy_hero), None, None),
            Err(SimulationError::UnexpectedTarget(untargeted)),
        ),
        (
            play_declaration(required, None, None, None),
            Err(SimulationError::MissingTarget(required)),
        ),
        (
            play_declaration(required, Some(stale), None, None),
            Err(SimulationError::InvalidTarget {
                card: required,
                target: stale,
            }),
        ),
        (
            play_declaration(required, Some(required), None, None),
            Err(SimulationError::InvalidTarget {
                card: required,
                target: required,
            }),
        ),
        (
            play_declaration(required, Some(friendly_hero), None, None),
            Err(SimulationError::InvalidTarget {
                card: required,
                target: friendly_hero,
            }),
        ),
        (
            play_declaration(required, Some(enemy_hero), None, None),
            Ok(play_declaration(required, Some(enemy_hero), None, None)),
        ),
        (
            play_declaration(optional, None, None, None),
            Ok(play_declaration(optional, None, None, None)),
        ),
        (
            play_declaration(conditional, None, None, None),
            Ok(play_declaration(conditional, None, None, None)),
        ),
        (
            play_declaration(positioned, None, None, None),
            Ok(play_declaration(positioned, None, Some(0), None)),
        ),
        (
            play_declaration(positioned, None, Some(0), None),
            Ok(play_declaration(positioned, None, Some(0), None)),
        ),
        (
            play_declaration(optional, None, Some(0), None),
            Err(SimulationError::UnexpectedBoardPosition(optional)),
        ),
        (
            play_declaration(positioned, None, None, Some(ChoiceId(7))),
            Err(SimulationError::UnsupportedActionChoice(ChoiceId(7))),
        ),
    ];
    for (action, expected) in cases {
        assert_eq!(
            action_validation::validate_action(simulation.app.world(), &action),
            expected,
        );
    }

    let enemy = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Available Enemy", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    assert_eq!(
        action_validation::validate_action(
            simulation.app.world(),
            &play_declaration(conditional, None, None, None),
        ),
        Err(SimulationError::MissingTarget(conditional)),
    );
    assert_eq!(
        action_validation::validate_action(
            simulation.app.world(),
            &play_declaration(conditional, Some(enemy), None, None),
        ),
        Ok(play_declaration(conditional, Some(enemy), None, None)),
    );

    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Existing", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    assert_eq!(
        action_validation::validate_action(
            simulation.app.world(),
            &play_declaration(positioned, None, None, None),
        ),
        Ok(play_declaration(positioned, None, Some(1), None)),
    );
}

#[googletest::test]
fn combat_rejects_non_character_attackers_and_defenders() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let snapshot = simulation.snapshot();
    let friendly_hero = snapshot.players[0].hero;
    let friendly_power = snapshot.players[0].hero_power.unwrap();
    let enemy_power = snapshot.players[1].hero_power.unwrap();
    for id in [friendly_hero, friendly_power] {
        let entity = game_entity(simulation.app.world(), id).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<AttackState>(entity)
            .unwrap()
            .exhausted = false;
        simulation
            .app
            .world_mut()
            .get_mut::<CurrentStats>(entity)
            .unwrap()
            .attack = 1;
    }

    assert_eq!(
        action_validation::validate_action(
            simulation.app.world(),
            &GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_power,
                defender: enemy_power,
            },
        ),
        Err(SimulationError::CannotAttack(friendly_power)),
    );
    assert_eq!(
        action_validation::validate_action(
            simulation.app.world(),
            &GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_hero,
                defender: enemy_power,
            },
        ),
        Err(SimulationError::InvalidDefender(enemy_power)),
    );
}

#[googletest::test]
fn invalid_declarations_only_append_their_rejection_trace() {
    let mut wrong_turn = simulation();
    assert_rejected_action_is_atomic(
        &mut wrong_turn,
        GameAction::EndTurn {
            player: PlayerId::Two,
        },
        SimulationError::NotPlayersTurn(PlayerId::Two),
    );

    let mut busy = simulation();
    busy.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Resolving;
    assert_rejected_action_is_atomic(
        &mut busy,
        GameAction::EndTurn {
            player: PlayerId::One,
        },
        SimulationError::NotAwaitingAction,
    );

    let mut complete = simulation();
    complete.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Complete;
    complete.app.world_mut().resource_mut::<GameState>().outcome =
        Some(GameOutcome::Winner(PlayerId::Two));
    assert_rejected_action_is_atomic(
        &mut complete,
        GameAction::EndTurn {
            player: PlayerId::One,
        },
        SimulationError::GameOver,
    );

    let mut unsupported = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::weapon("Weapon", 0, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let weapon = hand_card(&mut unsupported, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut unsupported,
        play_declaration(weapon, None, None, None),
        SimulationError::NotPlayable(weapon),
    );

    let mut choice = simulation();
    let card = hand_card(&mut choice, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut choice,
        play_declaration(card, None, None, Some(ChoiceId(9))),
        SimulationError::UnsupportedActionChoice(ChoiceId(9)),
    );

    let mut mana = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::spell("Expensive", 2)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut mana, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut mana,
        play_declaration(card, None, None, None),
        SimulationError::NotEnoughMana {
            player: PlayerId::One,
            required: 2,
            available: 1,
        },
    );

    let mut capacity = simulation();
    capacity
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .board_limit = 0;
    let card = hand_card(&mut capacity, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut capacity,
        play_declaration(card, None, None, None),
        SimulationError::BoardFull(PlayerId::One),
    );

    let mut minion_position = simulation();
    let card = hand_card(&mut minion_position, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut minion_position,
        play_declaration(card, None, Some(1), None),
        SimulationError::Zone(ZoneError::InvalidPosition {
            zone: Zone::Play,
            position: 1,
            length: 0,
        }),
    );

    let mut spell_position = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::spell("Spell", 0)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut spell_position, PlayerId::One);
    assert_rejected_action_is_atomic(
        &mut spell_position,
        play_declaration(card, None, Some(0), None),
        SimulationError::UnexpectedBoardPosition(card),
    );
}

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
fn legal_actions_floor_negative_costs_before_checking_affordability() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Negative cost", -2, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let player_entity = player(simulation.app.world(), PlayerId::One).unwrap().0;
    simulation
        .app
        .world_mut()
        .get_mut::<Player>(player_entity)
        .unwrap()
        .used_resources = 2;

    assert_that!(
        simulation
            .legal_actions()
            .iter()
            .any(|action| matches!(action, GameAction::PlayCard { card: id, .. } if *id == card)),
        is_false()
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
            length: 0,
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
    assert_that!(snapshot.players[0].board.len(), eq(7));
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
