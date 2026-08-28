use googletest::prelude::*;

use super::{test_support::*, *};

#[googletest::test]
fn fork_replays_to_an_equivalent_snapshot_and_trace() {
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
        .expect("card should resolve");

    let mut fork = simulation.fork().expect("accepted actions should replay");

    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    assert_that!(simulation.trace(), eq(fork.trace()));
}

#[googletest::test]
fn fork_preserves_snapshot_trace_and_continuation_equivalence() {
    let first = Card::minion("Migration Vanguard", 0, 2, 3);
    let second = Card::spell("Migration Follow-up", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::EnemyCharacters,
        amount: ValueExpression::Constant(2),
    }]);
    let players = [
        PlayerConfig::new("Jaina", vec![first, second]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ];
    let mut simulation = Simulation::with_seed(players, 0x5eed);
    let first = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: first,
            target: None,
            board_index: Some(0),
            choice: None,
        })
        .unwrap();

    let mut fork = simulation.fork().expect("migration prefix should replay");
    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    assert_that!(simulation.trace(), eq(fork.trace()));

    let second = hand_card(&mut simulation, PlayerId::One);
    let continuation = GameAction::PlayCard {
        player: PlayerId::One,
        card: second,
        target: None,
        board_index: None,
        choice: None,
    };
    simulation.apply(continuation.clone()).unwrap();
    fork.apply(continuation).unwrap();

    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    assert_that!(simulation.trace(), eq(fork.trace()));
    assert_that!(simulation.snapshot().players[1].health, eq(28));
    simulation.assert_invariants().unwrap();
    fork.assert_invariants().unwrap();
}

#[googletest::test]
fn migration_baseline_fixture_captures_nested_trace_snapshot_and_quiescence() {
    let reactive =
        Card::minion("Migration Reactor", 0, 2, 3).with_triggers(vec![self_event_trigger(
            EventKind::Damage,
            vec![Effect::DealDamage {
                targets: Selector::EnemyCharacters,
                amount: ValueExpression::Constant(2),
            }],
        )]);
    let bolt = Card::spell("Migration Bolt", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::with_seed(
        [
            PlayerConfig::new("Jaina", vec![reactive, bolt]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ],
        0x5eed,
    );
    let reactive = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: reactive,
            target: None,
            board_index: Some(0),
            choice: None,
        })
        .unwrap();
    let trace_start = simulation.trace().len();
    let bolt = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: bolt,
            target: Some(reactive),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(
        format!("{:#?}\n", simulation.snapshot()),
        eq(include_str!(
            "test_fixtures/migration_baseline_snapshot.txt"
        ))
    );
    assert_that!(
        format!("{:#?}\n", &simulation.trace()[trace_start..]),
        eq(include_str!("test_fixtures/migration_baseline_trace.txt"))
    );
    assert_that!(
        simulation.snapshot().game.status,
        eq(SimulationStatus::AwaitingAction)
    );
    assert_that!(
        simulation.app.world().resource::<ResolutionCursor>(),
        eq(&ResolutionCursor::default())
    );
    simulation.assert_invariants().unwrap();
}

#[googletest::test]
fn keyword_markers_are_canonical_snapshot_state() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::spell("Marker", 0)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let entity = game_entity(simulation.app.world(), card).unwrap();
    let without_keyword = simulation.snapshot();

    insert_keyword(simulation.app.world_mut(), entity, Keyword::Taunt);
    let with_keyword = simulation.snapshot();
    assert_that!(with_keyword == without_keyword, is_false());
    assert_that!(
        with_keyword
            .objects
            .iter()
            .find(|object| object.id == card)
            .unwrap()
            .keywords,
        eq(&std::collections::BTreeSet::from([Keyword::Taunt]))
    );

    remove_keyword(simulation.app.world_mut(), entity, Keyword::Taunt);
    assert_that!(simulation.snapshot(), eq(&without_keyword));
}

#[googletest::test]
fn equal_keyword_snapshots_have_equal_keyword_dependent_continuations() {
    let shielded = Card::minion("Shielded", 0, 2, 3).with_keywords([Keyword::DivineShield]);
    let ping = Card::spell("Ping", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![shielded, ping]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let shielded = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: shielded,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let mut fork = simulation.fork().unwrap();
    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == shielded)
            .unwrap()
            .keywords,
        eq(&std::collections::BTreeSet::from([Keyword::DivineShield]))
    );

    let ping = hand_card(&mut simulation, PlayerId::One);
    let continuation = GameAction::PlayCard {
        player: PlayerId::One,
        card: ping,
        target: Some(shielded),
        board_index: None,
        choice: None,
    };
    simulation.apply(continuation.clone()).unwrap();
    fork.apply(continuation).unwrap();

    let snapshot = simulation.snapshot();
    assert_that!(snapshot, eq(&fork.snapshot()));
    let shielded = snapshot
        .objects
        .iter()
        .find(|object| object.id == shielded)
        .unwrap();
    assert_that!(shielded.keywords.is_empty(), is_true());
    assert_that!(shielded.damage, eq(0));
}

#[googletest::test]
fn completed_actions_validate_runtime_shapes() {
    let mut simulation = simulation();
    let hero = hero(&mut simulation, PlayerId::One);
    let entity = game_entity(simulation.app.world(), hero).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .remove::<Armor>();

    assert_that!(
        simulation.apply(GameAction::EndTurn {
            player: PlayerId::One,
        }),
        err(eq(&SimulationError::Invariant(
            "Hero-form entity is missing Armor".to_string()
        )))
    );
}

#[googletest::test]
fn forked_migration_fixture_is_isolated_after_divergent_continuations() {
    let players = [
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ];
    let mut simulation = Simulation::with_seed(players, 0x5eed);
    let mut fork = simulation.fork().unwrap();

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    fork.apply(GameAction::Concede {
        player: PlayerId::One,
    })
    .unwrap();

    assert_that!(simulation.snapshot().game.outcome, none());
    assert_that!(
        fork.snapshot().game.outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    assert_that!(simulation.trace() == fork.trace(), is_false());
}

#[googletest::test]
fn draw_burn_fatigue_outcomes_and_private_helper_errors_are_testable() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::spell("Burn Me", 0)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let world = simulation.app.world_mut();
    world.resource_mut::<Ruleset>().hand_limit = 0;
    draw_card(world, PlayerId::One).unwrap();
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Graveyard)
            .len(),
        eq(1)
    );
    draw_card(world, PlayerId::One).unwrap();
    assert_that!(player(world, PlayerId::One).unwrap().1.fatigue, eq(1));

    let first_hero = hero_id(world, PlayerId::One).unwrap();
    let second_hero = hero_id(world, PlayerId::Two).unwrap();
    let first_entity = game_entity(world, first_hero).unwrap();
    let second_entity = game_entity(world, second_hero).unwrap();
    world.get_mut::<Damage>(first_entity).unwrap().0 = STARTING_HEALTH;
    crate::death::create_deaths(world);
    check_outcome(world);
    assert_that!(
        world.resource::<GameState>().outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    world.resource_mut::<GameState>().outcome = None;
    world.get_mut::<Damage>(second_entity).unwrap().0 = STARTING_HEALTH;
    crate::death::create_deaths(world);
    check_outcome(world);
    assert_that!(
        world.resource::<GameState>().outcome,
        eq(Some(GameOutcome::Draw))
    );

    assert_that!(
        attach_stat_modifier(
            world,
            PlayerId::One,
            GameEntityId(999),
            StatModifier {
                attack: 1,
                health: 1,
                silence_removable: true,
            }
        ),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(
        silence_entity(world, GameEntityId(999)),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(
        transform_entity(world, GameEntityId(999), Card::minion("Missing", 0, 1, 1)),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(copy_card_data(world, GameEntityId(999)), none());
    assert_that!(hero_id(world, PlayerId::One), eq(Some(first_hero)));
}

#[googletest::test]
fn spawn_and_index_helpers_report_cleanup_and_drift() {
    let mut simulation = simulation();
    let world = simulation.app.world_mut();
    world.resource_mut::<Ruleset>().hand_limit = 0;
    assert_that!(
        matches!(
            spawn_card(world, PlayerId::One, Card::spell("No Space", 0), Zone::Hand),
            Err(SimulationError::Zone(ZoneError::Full { .. }))
        ),
        is_true()
    );

    let indexed = *world.resource::<GameEntityIndex>().0.keys().next().unwrap();
    let original = world.resource::<GameEntityIndex>().0[&indexed];
    let replacement = world.spawn_empty().id();
    world
        .resource_mut::<GameEntityIndex>()
        .0
        .insert(indexed, replacement);
    assert_that!(
        assert_game_entity_index(world),
        err(eq(&format!("game entity index disagrees for {indexed:?}")))
    );
    world
        .resource_mut::<GameEntityIndex>()
        .0
        .insert(indexed, original);
    world.spawn(GameObject);
    assert_that!(
        assert_game_entity_index(world),
        err(eq(&"not every GameObject is indexed".to_string()))
    );
}
