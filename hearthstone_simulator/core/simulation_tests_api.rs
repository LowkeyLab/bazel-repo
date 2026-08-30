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
