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
fn choice_suspension_retains_lower_stack_work_and_resumes_selected_branch_first() {
    let mut simulation = simulation();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
    };
    let choice = ChoiceId(7);
    let option = ChoiceId(8);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [
            ResolutionOp::RequestChoice(ChoiceRequest {
                id: choice,
                player: PlayerId::One,
                options: vec![ChoiceOption {
                    id: option,
                    operations: vec![ResolutionOp::RunEffect {
                        context: context.clone(),
                        effect: Effect::GainResource {
                            player: PlayerSelector::Controller,
                            amount: 1,
                            temporary: true,
                        },
                        event: None,
                    }],
                }],
            }),
            ResolutionOp::RunEffect {
                context,
                effect: Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 10,
                    temporary: true,
                },
                event: None,
            },
        ],
    );
    drive_resolution(world).unwrap();

    assert_that!(simulation.pending_choice().unwrap().request.id, eq(choice));
    assert_that!(simulation.resolution_work().stack.len(), eq(1));
    let checkpoint = simulation.checkpoint().unwrap();
    let json = checkpoint.to_json().unwrap();
    let decoded = SimulationCheckpoint::from_json(&json).unwrap();
    assert_that!(decoded, eq(&checkpoint));
    let mut restored = Simulation::from_checkpoint(decoded).unwrap();
    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
    assert_that!(restored.snapshot(), eq(&simulation.snapshot()));
    assert_that!(restored.trace(), eq(simulation.trace()));

    assert_that!(
        simulation.choose(ChoiceId(99)),
        err(eq(&SimulationError::Resolution(
            ResolutionError::InvalidChoice(ChoiceId(99))
        )))
    );
    simulation.choose(option).unwrap();
    restored.choose(option).unwrap();

    assert_that!(simulation.pending_choice(), none());
    assert_that!(restored.snapshot(), eq(&simulation.snapshot()));
    assert_that!(restored.trace(), eq(simulation.trace()));
    assert_that!(simulation.resolution_work().stack.is_empty(), is_true());
    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(11)
    );
    let popped = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::OperationPopped { id, kind } => Some((*id, kind.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(
        popped.iter().map(|(_, kind)| *kind).collect::<Vec<_>>(),
        eq(&vec!["RequestChoice", "RunEffect", "RunEffect"])
    );
    assert_that!(popped[1].0 > popped[0].0, is_true());
    assert_that!(popped[2].0 < popped[0].0, is_true());
}

#[googletest::test]
fn failed_choice_resume_abandons_retained_work_and_recovers_input_state() {
    let mut simulation = simulation();
    let choice = ChoiceId(20);
    let option = ChoiceId(21);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [
            ResolutionOp::RequestChoice(ChoiceRequest {
                id: choice,
                player: PlayerId::One,
                options: vec![ChoiceOption {
                    id: option,
                    operations: vec![ResolutionOp::CheckOutcome],
                }],
            }),
            ResolutionOp::CheckOutcome,
        ],
    );
    drive_resolution(world).unwrap();
    world.resource_mut::<ResolutionWork>().remaining_budget = 0;

    assert_that!(
        matches!(
            simulation.choose(option),
            Err(SimulationError::Resolution(
                ResolutionError::BudgetExhausted { .. }
            ))
        ),
        is_true()
    );
    assert_that!(simulation.pending_choice(), none());
    assert_that!(simulation.resolution_work().sequence_active, is_false());
    assert_that!(simulation.resolution_work().stack.is_empty(), is_true());
    assert_that!(
        simulation.app.world().resource::<GameState>().status,
        eq(SimulationStatus::AwaitingAction)
    );
    simulation.assert_invariants().unwrap();
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
}

#[googletest::test]
fn checkpoints_reject_missing_logical_relationship_targets() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    checkpoint.entities[0].attached_to = Some(GameEntityId(u64::MAX));

    assert_that!(
        matches!(
            Simulation::from_checkpoint(checkpoint),
            Err(SimulationError::Checkpoint(_))
        ),
        is_true()
    );
}

#[googletest::test]
fn checkpoints_reject_non_monotonic_resolution_counters() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let mut malformed = Vec::new();

    let mut resolution = base.clone();
    resolution.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(7),
        operation: ResolutionOp::CheckOutcome,
    });
    resolution.resolution.next_resolution_id = 7;
    malformed.push(resolution);

    let mut event = base.clone();
    event.resolution.events.insert(
        EventId(8),
        PreparedEvent {
            context: EventContext {
                kind: EventKind::Damage,
                source: None,
                targets: Vec::new(),
                controller: PlayerId::One,
                proposed_value: None,
                actual_value: Some(1),
                simultaneous_ordinal: 0,
            },
            prechecked_triggers: None,
            candidates: None,
        },
    );
    event.resolution.next_event_id = 8;
    malformed.push(event);

    let mut slot = base;
    slot.resolution
        .event_slots
        .insert(EventSlotId(9), PreparedEventSlot::default());
    slot.resolution.next_event_slot_id = 9;
    malformed.push(slot);

    for checkpoint in malformed {
        assert_that!(
            matches!(
                Simulation::from_checkpoint(checkpoint),
                Err(SimulationError::Checkpoint(_))
            ),
            is_true()
        );
    }
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
