use googletest::prelude::*;

use super::{test_support::*, *};
use crate::{
    AttackState, CurrentStats, EffectOrigin, EnchantmentDuration, Player, PlayerSelector,
    SequenceStep, SubjectGuard, TargetAudience, TargetFilter, TargetKind, TargetRequirement,
    TransformKind, ZoneMoveRequest, ZoneMovementKind,
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

fn captured_target_fixture() -> (Simulation, GameEntityId, GameEntityId) {
    let enemy_character = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let control_change = self_event_trigger(
        EventKind::CardPlayed,
        vec![Effect::Move {
            targets: Selector::Source,
            player: PlayerSelector::Player(PlayerId::One),
            zone: Zone::Play,
            kind: ZoneMovementKind::Normal,
        }],
    );
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::spell("Captured Bolt", 0)
                    .with_targeting(TargetRequirement::Required(enemy_character))
                    .with_effects(vec![Effect::DealDamage {
                        targets: Selector::DeclaredTarget,
                        amount: ValueExpression::Constant(3),
                    }]),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let spell = hand_card(&mut simulation, PlayerId::One);
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Captured target", 0, 1, 5).with_triggers(vec![control_change]),
        Zone::Play,
    )
    .unwrap();

    (simulation, spell, target)
}

#[googletest::test]
fn declared_spell_target_remains_captured_after_card_played_changes_control() {
    let (mut simulation, spell, target) = captured_target_fixture();
    let rng = simulation.snapshot().rng;

    simulation
        .apply(play_declaration(spell, Some(target), None, None))
        .unwrap();

    let snapshot = simulation.snapshot();
    let target_snapshot = snapshot
        .objects
        .iter()
        .find(|object| object.id == target)
        .unwrap();
    assert_that!(target_snapshot.controller, eq(PlayerId::One));
    assert_that!(target_snapshot.zone, eq(Zone::Play));
    assert_that!(target_snapshot.damage, eq(3));
    assert_that!(snapshot.rng, eq(rng));
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::Damage {
                    target: damaged,
                    proposed: 3,
                    actual: 3,
                    ..
                } if *damaged == target
            ))
            .count(),
        eq(1),
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .any(|entry| matches!(entry, TraceEntry::RngChoice { .. })),
        is_false(),
    );
}

#[googletest::test]
fn target_control_changed_before_declaration_is_rejected_atomically() {
    let (mut simulation, spell, target) = captured_target_fixture();
    crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: target,
            destination_controller: PlayerId::One,
            destination: Zone::Play,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();

    assert_rejected_action_is_atomic(
        &mut simulation,
        play_declaration(spell, Some(target), None, None),
        SimulationError::InvalidTarget {
            card: spell,
            target,
        },
    );
}

#[googletest::test]
fn guarded_sequence_step_skips_and_keeps_later_sequence_work() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    let world = simulation.app.world_mut();
    let rng = world.resource::<DeterministicRng>().state();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![SubjectGuard {
                    subject: card,
                    required_zone: Zone::Play,
                }],
                step: SequenceStep::Concede {
                    player: PlayerId::One,
                },
            },
            ResolutionOp::RunSequenceStep(SequenceStep::EndTurn {
                player: PlayerId::One,
            }),
        ],
    );

    drive_resolution(world).unwrap();

    assert_that!(world.resource::<GameState>().outcome, eq(None));
    assert_that!(
        world.resource::<GameState>().active_player,
        eq(PlayerId::Two)
    );
    assert_that!(world.resource::<ResolutionWork>().stack, is_empty());
    assert_that!(world.resource::<DeterministicRng>().state(), eq(rng));
    assert_that!(
        world
            .resource::<CanonicalTrace>()
            .entries
            .iter()
            .any(|entry| matches!(entry, TraceEntry::OperationPopped { kind, .. } if kind == "CheckOutcome")),
        is_true(),
    );
    assert_that!(
        world
            .resource::<CanonicalTrace>()
            .entries
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::SequenceStepSkipped {
                    step: SequenceStep::Concede { player: PlayerId::One },
                    subject,
                    expected_zone: Zone::Play,
                    actual_zone: Some(Zone::Hand),
                } if *subject == card
            ))
            .count(),
        eq(1),
    );
}

fn resolve_guarded_sequence_step(
    simulation: &mut Simulation,
    guards: Vec<SubjectGuard>,
    step: SequenceStep,
) {
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [ResolutionOp::RunGuardedSequenceStep { guards, step }],
    );
    drive_resolution(world).unwrap();
}

#[googletest::test]
fn empty_guards_execute_the_sequence_step() {
    let mut simulation = simulation();

    resolve_guarded_sequence_step(
        &mut simulation,
        Vec::new(),
        SequenceStep::Concede {
            player: PlayerId::One,
        },
    );

    assert_that!(
        simulation.app.world().resource::<GameState>().outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .any(|entry| matches!(entry, TraceEntry::SequenceStepSkipped { .. })),
        is_false()
    );
}

#[googletest::test]
fn missing_subject_and_first_failed_guard_skip_once() {
    let mut missing = simulation();
    let absent = GameEntityId(u64::MAX);

    resolve_guarded_sequence_step(
        &mut missing,
        vec![SubjectGuard {
            subject: absent,
            required_zone: Zone::Play,
        }],
        SequenceStep::Concede {
            player: PlayerId::One,
        },
    );

    assert_that!(
        missing.app.world().resource::<GameState>().outcome,
        eq(None)
    );
    assert!(matches!(
        missing.trace().last(),
        Some(TraceEntry::SequenceStepSkipped {
            subject,
            expected_zone: Zone::Play,
            actual_zone: None,
            ..
        }) if *subject == absent
    ));

    let mut first = simulation();
    let card = hand_card(&mut first, PlayerId::One);
    let second = hero(&mut first, PlayerId::Two);
    resolve_guarded_sequence_step(
        &mut first,
        vec![
            SubjectGuard {
                subject: card,
                required_zone: Zone::Play,
            },
            SubjectGuard {
                subject: second,
                required_zone: Zone::Hand,
            },
        ],
        SequenceStep::Concede {
            player: PlayerId::One,
        },
    );

    assert!(matches!(
        first.trace().last(),
        Some(TraceEntry::SequenceStepSkipped {
            subject,
            actual_zone: Some(Zone::Hand),
            ..
        }) if *subject == card
    ));
}

#[googletest::test]
fn guards_evaluate_after_preceding_effects() {
    let mut simulation = simulation();
    let card = hand_card(&mut simulation, PlayerId::One);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [
            ResolutionOp::RunEffect {
                context: EffectContext {
                    source: None,
                    controller: PlayerId::One,
                    declared_target: None,
                    drawn_card: None,
                    origin: EffectOrigin::Other,
                },
                effect: Effect::Move {
                    targets: Selector::Entity(card),
                    player: PlayerSelector::Controller,
                    zone: Zone::Graveyard,
                    kind: ZoneMovementKind::Normal,
                },
                event: None,
            },
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![SubjectGuard {
                    subject: card,
                    required_zone: Zone::Hand,
                }],
                step: SequenceStep::Concede {
                    player: PlayerId::One,
                },
            },
        ],
    );

    drive_resolution(world).unwrap();

    assert_that!(world.resource::<GameState>().outcome, eq(None));
    assert_that!(
        world.get::<Zone>(game_entity(world, card).unwrap()),
        eq(Some(&Zone::Graveyard))
    );
    assert!(matches!(
        world.resource::<CanonicalTrace>().entries.last(),
        Some(TraceEntry::SequenceStepSkipped {
            subject,
            expected_zone: Zone::Hand,
            actual_zone: Some(Zone::Graveyard),
            ..
        }) if *subject == card
    ));
}

#[googletest::test]
fn returned_and_transformed_subjects_pass_zone_guards() {
    let mut returned = simulation();
    let card = hand_card(&mut returned, PlayerId::One);
    let world = returned.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    let move_context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    push_resolution_ops(
        world,
        [
            ResolutionOp::RunEffect {
                context: move_context.clone(),
                effect: Effect::Move {
                    targets: Selector::Entity(card),
                    player: PlayerSelector::Controller,
                    zone: Zone::Deck,
                    kind: ZoneMovementKind::Normal,
                },
                event: None,
            },
            ResolutionOp::RunEffect {
                context: move_context,
                effect: Effect::Move {
                    targets: Selector::Entity(card),
                    player: PlayerSelector::Controller,
                    zone: Zone::Hand,
                    kind: ZoneMovementKind::Normal,
                },
                event: None,
            },
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![SubjectGuard {
                    subject: card,
                    required_zone: Zone::Hand,
                }],
                step: SequenceStep::Concede {
                    player: PlayerId::One,
                },
            },
        ],
    );
    drive_resolution(world).unwrap();
    assert_that!(
        world.resource::<GameState>().outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );

    let mut transformed = simulation();
    let card = hand_card(&mut transformed, PlayerId::One);
    let world = transformed.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [
            ResolutionOp::TransformEntity {
                play_scope: None,
                target: card,
                source: None,
                card: Card::minion("Replacement", 0, 1, 1),
                kind: TransformKind::Spell,
            },
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![SubjectGuard {
                    subject: card,
                    required_zone: Zone::Hand,
                }],
                step: SequenceStep::Concede {
                    player: PlayerId::One,
                },
            },
        ],
    );
    drive_resolution(world).unwrap();
    assert_that!(
        world.resource::<GameState>().outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    assert_that!(
        world.get::<Zone>(game_entity(world, card).unwrap()),
        eq(Some(&Zone::Hand))
    );
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
            .readiness_blocked = false;
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
        PlayerConfig::new("Jaina", vec![Card::hero("Unsupported Hero", 30)]),
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
    let before = simulation.checkpoint().unwrap();
    let first = simulation.legal_actions();
    assert_eq!(first, simulation.legal_actions());
    assert_eq!(simulation.checkpoint().unwrap(), before);
    for action in &first {
        assert_eq!(
            super::action_validation::validate_action(simulation.app.world(), action).unwrap(),
            *action,
        );
    }
}

#[googletest::test]
fn legal_actions_offer_each_required_target_at_the_explicit_minion_position() {
    let enemy_character = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Targeted", 0, 1, 1)
                    .with_targeting(TargetRequirement::Required(enemy_character)),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    let enemy_minion = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let mut targets = vec![enemy_hero, enemy_minion];
    targets.sort_unstable();

    assert_eq!(
        simulation.legal_actions(),
        vec![
            GameAction::EndTurn {
                player: PlayerId::One,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: Some(targets[0]),
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: Some(targets[1]),
                board_index: Some(0),
                choice: None,
            },
            GameAction::Concede {
                player: PlayerId::One,
            },
        ],
    );
}

#[googletest::test]
fn legal_actions_use_canonical_order_and_deduplicate_scrambled_indexes() {
    let enemy_character = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Optional positioned", 0, 1, 1)
                    .with_targeting(TargetRequirement::Optional(enemy_character)),
                Card::spell("Plain spell", 0),
                Card::minion("Plain positioned", 0, 1, 1),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let optional_positioned = card_named(&mut simulation, "Optional positioned");
    let plain_spell = card_named(&mut simulation, "Plain spell");
    let plain_positioned = card_named(&mut simulation, "Plain positioned");
    let friendly_hero = hero(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    let snapshot = simulation.snapshot();
    let friendly_hero_power = snapshot.players[0].hero_power.unwrap();
    let enemy_hero_power = snapshot.players[1].hero_power.unwrap();
    let friendly_first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Friendly first", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let friendly_second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Friendly second", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let enemy_first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy first", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let enemy_second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy second", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();

    assert!(optional_positioned < plain_spell && plain_spell < plain_positioned);
    assert!(friendly_hero < friendly_first && friendly_first < friendly_second);
    assert!(enemy_hero < enemy_first && enemy_first < enemy_second);
    for attacker in [friendly_hero, friendly_first, friendly_second] {
        let entity = game_entity(simulation.app.world(), attacker).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<CurrentStats>(entity)
            .unwrap()
            .attack = 1;
        simulation
            .app
            .world_mut()
            .get_mut::<AttackState>(entity)
            .unwrap()
            .readiness_blocked = false;
    }

    let mut index = simulation.app.world_mut().resource_mut::<ZoneIndex>();
    index.0.insert(
        (PlayerId::One, Zone::Hand),
        vec![
            plain_positioned,
            optional_positioned,
            plain_spell,
            plain_positioned,
        ],
    );
    index.0.insert(
        (PlayerId::One, Zone::Play),
        vec![
            friendly_second,
            friendly_hero,
            friendly_hero_power,
            friendly_first,
            friendly_hero,
        ],
    );
    index.0.insert(
        (PlayerId::Two, Zone::Play),
        vec![
            enemy_second,
            enemy_hero,
            enemy_hero_power,
            enemy_first,
            enemy_hero,
        ],
    );

    assert_eq!(
        simulation.legal_actions(),
        vec![
            GameAction::EndTurn {
                player: PlayerId::One,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: None,
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: None,
                board_index: Some(1),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: None,
                board_index: Some(2),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_hero),
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_hero),
                board_index: Some(1),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_hero),
                board_index: Some(2),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_first),
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_first),
                board_index: Some(1),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_first),
                board_index: Some(2),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_second),
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_second),
                board_index: Some(1),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: optional_positioned,
                target: Some(enemy_second),
                board_index: Some(2),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: plain_spell,
                target: None,
                board_index: None,
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: plain_positioned,
                target: None,
                board_index: Some(0),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: plain_positioned,
                target: None,
                board_index: Some(1),
                choice: None,
            },
            GameAction::PlayCard {
                player: PlayerId::One,
                card: plain_positioned,
                target: None,
                board_index: Some(2),
                choice: None,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_hero,
                defender: enemy_hero,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_hero,
                defender: enemy_first,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_hero,
                defender: enemy_second,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_first,
                defender: enemy_hero,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_first,
                defender: enemy_first,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_first,
                defender: enemy_second,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_second,
                defender: enemy_hero,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_second,
                defender: enemy_first,
            },
            GameAction::Attack {
                player: PlayerId::One,
                attacker: friendly_second,
                defender: enemy_second,
            },
            GameAction::Concede {
                player: PlayerId::One,
            },
        ],
    );
}

#[googletest::test]
fn legal_actions_contain_every_successfully_validated_small_fixture_candidate() {
    let enemy_character = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Targeted", 0, 1, 1)
                    .with_targeting(TargetRequirement::Required(enemy_character)),
                Card::spell("Spell", 0),
                Card::hero("Unsupported", 30),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let enemy_minion = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let friendly_hero = hero(&mut simulation, PlayerId::One);
    let friendly_hero_entity = game_entity(simulation.app.world(), friendly_hero).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<CurrentStats>(friendly_hero_entity)
        .unwrap()
        .attack = 1;
    simulation
        .app
        .world_mut()
        .get_mut::<AttackState>(friendly_hero_entity)
        .unwrap()
        .readiness_blocked = false;

    let legal_actions = simulation.legal_actions();
    let mut hand_ids = simulation.snapshot().players[0].hand.clone();
    let stale = GameEntityId(u64::MAX);
    hand_ids.push(stale);
    let mut entity_ids = simulation
        .snapshot()
        .objects
        .into_iter()
        .map(|object| object.id)
        .collect::<Vec<_>>();
    entity_ids.push(stale);
    let board_len = simulation.snapshot().players[0].board.len();

    for card in hand_ids {
        for target in std::iter::once(None).chain(entity_ids.iter().copied().map(Some)) {
            for board_index in std::iter::once(None).chain((0..=board_len + 1).map(Some)) {
                for choice in [None, Some(ChoiceId(999))] {
                    let candidate = play_declaration(card, target, board_index, choice);
                    if let Ok(normalized) = super::action_validation::validate_action(
                        simulation.app.world(),
                        &candidate,
                    ) {
                        assert!(
                            legal_actions.contains(&normalized),
                            "missing normalized play action: {normalized:?}"
                        );
                    }
                }
            }
        }
    }
    for attacker in &entity_ids {
        for defender in &entity_ids {
            let candidate = GameAction::Attack {
                player: PlayerId::One,
                attacker: *attacker,
                defender: *defender,
            };
            if let Ok(normalized) =
                super::action_validation::validate_action(simulation.app.world(), &candidate)
            {
                assert!(
                    legal_actions.contains(&normalized),
                    "missing normalized attack action: {normalized:?}"
                );
            }
        }
    }
    for player in PlayerId::ALL {
        for candidate in [
            GameAction::EndTurn { player },
            GameAction::Concede { player },
        ] {
            if let Ok(normalized) =
                super::action_validation::validate_action(simulation.app.world(), &candidate)
            {
                assert!(
                    legal_actions.contains(&normalized),
                    "missing normalized turn action: {normalized:?}"
                );
            }
        }
    }

    assert!(
        legal_actions.iter().any(
            |action| matches!(action, GameAction::Attack { attacker, defender, .. } if *attacker == friendly_hero && *defender == enemy_minion)
        ),
        "fixture should exercise an executable attack"
    );
    for action in legal_actions {
        simulation
            .fork()
            .unwrap()
            .apply(action)
            .expect("every enumerated action should execute on a fresh fork");
    }
}

#[googletest::test]
fn legal_actions_exclude_exhausted_zero_attack_full_board_and_unsupported_candidates() {
    let mut exhausted_and_zero_attack = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let exhausted = spawn_card(
        exhausted_and_zero_attack.app.world_mut(),
        PlayerId::One,
        Card::minion("Exhausted", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let zero_attack = spawn_card(
        exhausted_and_zero_attack.app.world_mut(),
        PlayerId::One,
        Card::minion("Zero attack", 0, 0, 1),
        Zone::Play,
    )
    .unwrap();
    spawn_card(
        exhausted_and_zero_attack.app.world_mut(),
        PlayerId::Two,
        Card::minion("Defender", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let exhausted_entity = game_entity(exhausted_and_zero_attack.app.world(), exhausted).unwrap();
    let zero_attack_entity =
        game_entity(exhausted_and_zero_attack.app.world(), zero_attack).unwrap();
    exhausted_and_zero_attack
        .app
        .world_mut()
        .get_mut::<AttackState>(exhausted_entity)
        .unwrap()
        .readiness_blocked = true;
    exhausted_and_zero_attack
        .app
        .world_mut()
        .get_mut::<AttackState>(zero_attack_entity)
        .unwrap()
        .readiness_blocked = false;
    assert!(
        exhausted_and_zero_attack
            .legal_actions()
            .iter()
            .all(|action| !matches!(action, GameAction::Attack { attacker, .. } if *attacker == exhausted || *attacker == zero_attack)),
    );

    let mut full_board = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Blocked", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    full_board
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .board_limit = 0;
    assert!(
        full_board
            .legal_actions()
            .iter()
            .all(|action| !matches!(action, GameAction::PlayCard { .. })),
    );

    let mut unsupported = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::hero("Unsupported", 30)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    assert!(
        unsupported
            .legal_actions()
            .iter()
            .all(|action| !matches!(action, GameAction::PlayCard { .. })),
    );
}

#[googletest::test]
fn legal_actions_normalize_negative_cost_minion_append_to_the_final_position() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Negative", -1, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Existing", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let player_entity = player(simulation.app.world(), PlayerId::One).unwrap().0;
    simulation
        .app
        .world_mut()
        .get_mut::<Player>(player_entity)
        .unwrap()
        .used_resources = 1;

    let normalized = super::action_validation::validate_action(
        simulation.app.world(),
        &play_declaration(card, None, None, None),
    )
    .unwrap();
    assert_eq!(
        normalized,
        play_declaration(card, None, Some(1), None),
        "the implicit minion append is canonicalized to the final explicit position"
    );
    let legal_actions = simulation.legal_actions();
    assert!(legal_actions.contains(&normalized));
    assert!(
        legal_actions.iter().all(
            |action| !matches!(action, GameAction::PlayCard { card: action_card, board_index: None, .. } if *action_card == card)
        ),
    );
}

#[googletest::test]
fn legal_actions_are_empty_when_the_game_is_not_awaiting_an_open_action() {
    for status in [
        SimulationStatus::Resolving,
        SimulationStatus::AwaitingChoice,
        SimulationStatus::Complete,
    ] {
        let mut simulation = simulation();
        simulation
            .app
            .world_mut()
            .resource_mut::<GameState>()
            .status = status;
        assert!(simulation.legal_actions().is_empty(), "status: {status:?}");
    }

    let mut finished = simulation();
    finished.app.world_mut().resource_mut::<GameState>().outcome =
        Some(GameOutcome::Winner(PlayerId::Two));
    assert!(finished.legal_actions().is_empty());
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
        eq(&vec![
            GameAction::EndTurn {
                player: PlayerId::One
            },
            GameAction::Concede {
                player: PlayerId::One
            },
        ])
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

fn keyword_attacker(simulation: &mut Simulation, card: Card) -> GameEntityId {
    let id = spawn_card(simulation.app.world_mut(), PlayerId::One, card, Zone::Play).unwrap();
    let entity = game_entity(simulation.app.world(), id).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<AttackState>(entity)
        .unwrap()
        .readiness_blocked = false;
    id
}

fn has_stealth(simulation: &Simulation, id: GameEntityId) -> bool {
    crate::aura::has_keyword(
        simulation.app.world(),
        game_entity(simulation.app.world(), id).unwrap(),
        Keyword::Stealth,
    )
}

fn keyword_grant(
    simulation: &mut Simulation,
    id: GameEntityId,
    keyword: Keyword,
    duration: crate::EnchantmentDuration,
) {
    super::effect_executor::attach_keyword_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        id,
        crate::KeywordModifier {
            keyword,
            granted: true,
            silence_removable: true,
        },
        duration,
    )
    .unwrap();
}

#[test]
fn taunt_stealth_and_immune_attack_matrix_is_pure_and_atomic() {
    for hidden in [None, Some(Keyword::Stealth), Some(Keyword::Immune)] {
        let mut simulation = simulation();
        let attacker = keyword_attacker(&mut simulation, Card::minion("Attacker", 0, 2, 5));
        let mut card = Card::minion("Guard", 0, 1, 5).with_keyword(Keyword::Taunt);
        if let Some(keyword) = hidden {
            card = card.with_keyword(keyword);
        }
        let guard =
            spawn_card(simulation.app.world_mut(), PlayerId::Two, card, Zone::Play).unwrap();
        let enemy_hero = hero(&mut simulation, PlayerId::Two);
        let face = GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: enemy_hero,
        };
        let guarded = GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: guard,
        };
        let before = simulation.checkpoint().unwrap();
        let actions = simulation.legal_actions();
        assert_eq!(actions, simulation.legal_actions());
        assert_eq!(simulation.checkpoint().unwrap(), before);
        assert_eq!(actions.contains(&face), hidden.is_some());
        assert_eq!(actions.contains(&guarded), hidden.is_none());
        for action in &actions {
            assert!(
                super::action_validation::validate_action(simulation.app.world(), action).is_ok()
            );
        }
        if hidden.is_some() {
            assert_rejected_action_is_atomic(
                &mut simulation,
                guarded,
                SimulationError::InvalidDefender(guard),
            );
        } else {
            assert_rejected_action_is_atomic(
                &mut simulation,
                face.clone(),
                SimulationError::InvalidDefender(enemy_hero),
            );
        }
        // A second, visible Taunt still blocks attacks past a concealed guard.
        let visible = spawn_card(
            simulation.app.world_mut(),
            PlayerId::Two,
            Card::minion("Visible guard", 0, 1, 5).with_keyword(Keyword::Taunt),
            Zone::Play,
        )
        .unwrap();
        assert!(!simulation.legal_actions().contains(&face));
        assert!(simulation.legal_actions().contains(&GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: visible
        }));
        silence_entity(simulation.app.world_mut(), visible).unwrap();
        silence_entity(simulation.app.world_mut(), guard).unwrap();
        assert!(simulation.legal_actions().contains(&face));
    }
}

#[test]
fn keyword_targeting_applies_to_every_declaration_and_target_requirement() {
    for kind in [EntityKind::Minion, EntityKind::Spell, EntityKind::HeroPower] {
        for keyword in [Keyword::Stealth, Keyword::Immune] {
            for owner in PlayerId::ALL {
                let filter = TargetFilter {
                    audience: TargetAudience::Either,
                    kind: TargetKind::Minion,
                };
                for requirement in [
                    TargetRequirement::None,
                    TargetRequirement::Required(filter),
                    TargetRequirement::Optional(filter),
                    TargetRequirement::RequiredIfAvailable(filter),
                ] {
                    let mut simulation = Simulation::new([
                        PlayerConfig::new("One", vec![]),
                        PlayerConfig::new("Two", vec![]),
                    ]);
                    let target = spawn_card(
                        simulation.app.world_mut(),
                        owner,
                        Card::minion("Hidden", 0, 1, 5).with_keyword(keyword),
                        Zone::Play,
                    )
                    .unwrap();
                    let card = match kind {
                        EntityKind::Minion => spawn_card(
                            simulation.app.world_mut(),
                            PlayerId::One,
                            Card::minion("Targeted minion", 0, 1, 2).with_targeting(requirement),
                            Zone::Hand,
                        )
                        .unwrap(),
                        EntityKind::Spell => spawn_card(
                            simulation.app.world_mut(),
                            PlayerId::One,
                            Card::spell("Targeted spell", 0).with_targeting(requirement),
                            Zone::Hand,
                        )
                        .unwrap(),
                        EntityKind::HeroPower => {
                            let id = simulation.snapshot().players[0].hero_power.unwrap();
                            let entity = game_entity(simulation.app.world(), id).unwrap();
                            let mut runtime = simulation
                                .app
                                .world_mut()
                                .get_mut::<super::card_runtime::CardRuntime>(entity)
                                .unwrap();
                            runtime.targeting = requirement;
                            runtime.base_cost = 0;
                            runtime.cost = 0;
                            id
                        }
                        _ => unreachable!(),
                    };
                    let declaration = |target| {
                        if kind == EntityKind::HeroPower {
                            GameAction::UseHeroPower {
                                player: PlayerId::One,
                                power: card,
                                target,
                            }
                        } else {
                            play_declaration(
                                card,
                                target,
                                if kind == EntityKind::Minion {
                                    Some(0)
                                } else {
                                    None
                                },
                                None,
                            )
                        }
                    };
                    let targeted = declaration(Some(target));
                    let untargeted = declaration(None);
                    let friendly = owner == PlayerId::One;
                    let expected_target = friendly && requirement != TargetRequirement::None;
                    let expected_none = matches!(
                        requirement,
                        TargetRequirement::None | TargetRequirement::Optional(_)
                    ) || (!friendly
                        && matches!(requirement, TargetRequirement::RequiredIfAvailable(_)));
                    let before = simulation.checkpoint().unwrap();
                    let actions = simulation.legal_actions();
                    assert_eq!(
                        actions.contains(&targeted),
                        expected_target,
                        "{kind:?} {keyword:?} {owner:?} {requirement:?}"
                    );
                    assert_eq!(actions.contains(&untargeted), expected_none);
                    assert_eq!(simulation.checkpoint().unwrap(), before);
                    for (action, expected) in
                        [(targeted, expected_target), (untargeted, expected_none)]
                    {
                        let result = super::action_validation::validate_action(
                            simulation.app.world(),
                            &action,
                        );
                        assert_eq!(result.is_ok(), expected);
                        if let Err(error) = result {
                            assert_rejected_action_is_atomic(&mut simulation, action, error);
                        } else {
                            let mut fork = simulation.fork().unwrap();
                            fork.apply(action).unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn stealth_consumption_survives_recalculation_and_allows_later_timed_grants() {
    let mut simulation = simulation();
    let attacker = keyword_attacker(
        &mut simulation,
        Card::minion("Hidden attacker", 0, 2, 5).with_keyword(Keyword::Stealth),
    );
    // Both innate and attached Stealth must be consumed by the attack.
    keyword_grant(
        &mut simulation,
        attacker,
        Keyword::Stealth,
        crate::EnchantmentDuration::Permanent,
    );
    let defender = hero(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();
    assert!(!has_stealth(&simulation, attacker));
    keyword_grant(
        &mut simulation,
        attacker,
        Keyword::Taunt,
        crate::EnchantmentDuration::Permanent,
    );
    assert!(!has_stealth(&simulation, attacker));
    let mut restored = Simulation::from_checkpoint(
        SimulationCheckpoint::from_json(&simulation.checkpoint().unwrap().to_json().unwrap())
            .unwrap(),
    )
    .unwrap();
    crate::enchantment::recalculate_keywords(restored.app.world_mut(), attacker);
    assert!(!has_stealth(&restored, attacker));
    keyword_grant(
        &mut simulation,
        attacker,
        Keyword::Stealth,
        crate::EnchantmentDuration::EndOfTurn(PlayerId::One),
    );
    assert!(has_stealth(&simulation, attacker));
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    assert!(!has_stealth(&simulation, attacker));
    keyword_grant(
        &mut simulation,
        attacker,
        Keyword::Stealth,
        crate::EnchantmentDuration::Permanent,
    );
    assert!(has_stealth(&simulation, attacker));
    silence_entity(simulation.app.world_mut(), attacker).unwrap();
    assert!(!has_stealth(&simulation, attacker));
    simulation.assert_invariants().unwrap();
}

#[test]
fn consumed_stealth_follows_copy_transform_and_backward_movement_policies() {
    let mut simulation = simulation();
    let attacker = keyword_attacker(
        &mut simulation,
        Card::minion("Hidden original", 0, 2, 5).with_keyword(Keyword::Stealth),
    );
    let defender = hero(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();
    for (destination, policy) in [
        (Zone::Play, crate::CopyStatePolicy::InPlayState),
        (Zone::Hand, crate::CopyStatePolicy::CurrentForm),
    ] {
        let before = simulation
            .snapshot()
            .objects
            .into_iter()
            .map(|object| object.id)
            .collect::<Vec<_>>();
        super::effect_executor::copy_entity(
            simulation.app.world_mut(),
            crate::CopyRequest {
                source: attacker,
                originating_source: None,
                controller: PlayerId::One,
                destination,
                board_index: None,
                policy,
            },
        )
        .unwrap();
        drive_resolution(simulation.app.world_mut()).unwrap();
        let copy = simulation
            .snapshot()
            .objects
            .into_iter()
            .find(|object| object.name == "Hidden original" && !before.contains(&object.id))
            .unwrap()
            .id;
        crate::enchantment::recalculate_keywords(simulation.app.world_mut(), copy);
        assert_eq!(has_stealth(&simulation, copy), destination == Zone::Hand);
    }
    crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: attacker,
            destination_controller: PlayerId::One,
            destination: Zone::Hand,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();
    assert!(has_stealth(&simulation, attacker));
    simulation
        .apply(play_declaration(attacker, None, None, None))
        .unwrap();
    super::action::run_sequence_step(
        simulation.app.world_mut(),
        &SequenceStep::BreakAttackStealth { attacker },
    )
    .unwrap();
    transform_entity(
        simulation.app.world_mut(),
        attacker,
        Card::minion("Fresh hidden form", 0, 1, 5).with_keyword(Keyword::Stealth),
        TransformKind::Spell,
    )
    .unwrap();
    assert!(has_stealth(&simulation, attacker));
    simulation.assert_invariants().unwrap();
}

#[test]
fn suspended_attack_breaks_stealth_after_reactions_before_damage_and_restores_exactly() {
    let mut simulation = simulation();
    simulation
        .register_native_effect(
            "pause_hidden_attack",
            |context: &EffectContext, world: &World| {
                let source = context.source.unwrap();
                assert!(crate::aura::has_keyword(
                    world,
                    game_entity(world, source).unwrap(),
                    Keyword::Stealth
                ));
                vec![Effect::Choose {
                    id: ChoiceId(70),
                    player: PlayerSelector::Controller,
                    options: vec![hearthstone_simulator_core::EffectChoiceOption {
                        id: ChoiceId(71),
                        effects: vec![],
                    }],
                }]
            },
        )
        .unwrap();
    let mut trigger = self_event_trigger(
        EventKind::Attack,
        vec![Effect::Native("pause_hidden_attack".into())],
    );
    trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
    let attacker = keyword_attacker(
        &mut simulation,
        Card::minion("Paused attacker", 0, 2, 5)
            .with_keyword(Keyword::Windfury)
            .with_keyword(Keyword::Stealth)
            .with_triggers(vec![trigger]),
    );
    let defender = hero(&mut simulation, PlayerId::Two);
    simulation
        .register_native_effect(
            "check_visible_before_damage",
            move |_: &EffectContext, world: &World| {
                assert!(!crate::aura::has_keyword(
                    world,
                    game_entity(world, attacker).unwrap(),
                    Keyword::Stealth
                ));
                vec![]
            },
        )
        .unwrap();
    let defender_entity = game_entity(simulation.app.world(), defender).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(defender_entity)
        .insert(RuntimeTriggers(vec![self_event_trigger(
            EventKind::ProposedDamage,
            vec![Effect::Native("check_visible_before_damage".into())],
        )]));
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();
    assert_eq!(
        simulation.snapshot().game.status,
        SimulationStatus::AwaitingChoice
    );
    assert!(has_stealth(&simulation, attacker));
    assert_eq!(
        windfury_attack_state(&simulation, attacker).attacks_this_turn,
        0
    );
    let checkpoint = simulation.checkpoint().unwrap();
    let mut invalid = checkpoint.clone();
    let step = invalid
        .resolution
        .stack
        .iter_mut()
        .find_map(|op| match &mut op.operation {
            ResolutionOp::RunGuardedSequenceStep {
                step: SequenceStep::BreakAttackStealth { attacker },
                ..
            } => Some(attacker),
            _ => None,
        })
        .unwrap();
    *step = GameEntityId(u64::MAX);
    assert!(simulation.restore(invalid).is_err());
    let mut fork = simulation.fork().unwrap();
    let mut restored = simulation.fork().unwrap();
    restored
        .restore(SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap())
        .unwrap();
    for candidate in [&mut simulation, &mut fork, &mut restored] {
        candidate.choose(ChoiceId(71)).unwrap();
        assert!(!has_stealth(candidate, attacker));
        assert_eq!(
            windfury_attack_state(candidate, attacker).attacks_this_turn,
            1
        );
        assert!(!windfury_snapshot_exhausted(candidate, attacker));
        candidate.assert_invariants().unwrap();
    }
    assert_eq!(simulation.checkpoint().unwrap(), fork.checkpoint().unwrap());
    assert_eq!(
        simulation.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
}

#[test]
fn area_and_random_effects_can_still_select_stealthed_characters() {
    let mut simulation = simulation();
    let hidden = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Hidden target", 0, 1, 5).with_keyword(Keyword::Stealth),
        Zone::Play,
    )
    .unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Spell,
    };
    let selected = select_entities(simulation.app.world_mut(), &context, &Selector::AllMinions);
    assert!(selected.contains(&hidden));
    assert_eq!(
        select_entities(
            simulation.app.world_mut(),
            &context,
            &Selector::Random(Box::new(Selector::EnemyMinions))
        ),
        vec![hidden]
    );
    apply_damage(simulation.app.world_mut(), None, hidden, 1).unwrap();
    let victim = hero(&mut simulation, PlayerId::One);
    apply_damage(simulation.app.world_mut(), Some(hidden), victim, 1).unwrap();
    assert!(has_stealth(&simulation, hidden));
}

#[test]
fn stealth_breaks_even_when_attack_damage_is_prevented() {
    let mut simulation = simulation();
    let attacker = keyword_attacker(
        &mut simulation,
        Card::minion("Hidden attacker", 0, 2, 5).with_keyword(Keyword::Stealth),
    );
    let defender = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Shielded", 0, 0, 5).with_keyword(Keyword::DivineShield),
        Zone::Play,
    )
    .unwrap();
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();
    assert!(!has_stealth(&simulation, attacker));
    let entity = game_entity(simulation.app.world(), defender).unwrap();
    assert_eq!(
        simulation.app.world().get::<Damage>(entity),
        Some(&Damage(0))
    );
}

#[test]
fn aura_immune_suppresses_taunt_and_direct_targeting_until_removed() {
    let mut simulation = simulation();
    let attacker = keyword_attacker(&mut simulation, Card::minion("Attacker", 0, 1, 5));
    let guard = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Guard", 0, 1, 5).with_keyword(Keyword::Taunt),
        Zone::Play,
    )
    .unwrap();
    let provider = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Immune provider", 0, 0, 5).with_aura(crate::AuraDefinition {
            targets: crate::AuraTarget::OtherFriendlyCharacters,
            attack: 0,
            health: 0,
            other: vec![crate::OtherAuraModifier::Immune],
        }),
        Zone::Play,
    )
    .unwrap();
    crate::aura::refresh_all_auras(simulation.app.world_mut());
    let attack_provider = GameAction::Attack {
        player: PlayerId::One,
        attacker,
        defender: provider,
    };
    assert!(simulation.legal_actions().contains(&attack_provider));
    assert!(
        !super::action_validation::eligible_targets(
            simulation.app.world(),
            PlayerId::One,
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Minion
            }
        )
        .contains(&guard)
    );
    silence_entity(simulation.app.world_mut(), provider).unwrap();
    crate::aura::refresh_all_auras(simulation.app.world_mut());
    assert!(!simulation.legal_actions().contains(&attack_provider));
    assert!(
        super::action_validation::eligible_targets(
            simulation.app.world(),
            PlayerId::One,
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Minion
            }
        )
        .contains(&guard)
    );
}

fn windfury_attack_state(simulation: &Simulation, attacker: GameEntityId) -> AttackState {
    *simulation
        .app
        .world()
        .get::<AttackState>(game_entity(simulation.app.world(), attacker).unwrap())
        .unwrap()
}

fn windfury_snapshot_exhausted(simulation: &mut Simulation, attacker: GameEntityId) -> bool {
    simulation
        .snapshot()
        .objects
        .iter()
        .find(|object| object.id == attacker)
        .unwrap()
        .exhausted
        .unwrap()
}

fn windfury_fixture(is_hero: bool, windfury: bool) -> (Simulation, GameEntityId, GameAction) {
    let mut simulation = simulation();
    let attacker = if is_hero {
        let id = hero(&mut simulation, PlayerId::One);
        let entity = game_entity(simulation.app.world(), id).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<CurrentStats>(entity)
            .unwrap()
            .attack = 1;
        if windfury {
            keyword_grant(
                &mut simulation,
                id,
                Keyword::Windfury,
                crate::EnchantmentDuration::Permanent,
            );
        }
        id
    } else {
        let mut card = Card::minion("Windfury fixture", 0, 1, 20);
        if windfury {
            card = card.with_keyword(Keyword::Windfury);
        }
        keyword_attacker(&mut simulation, card)
    };
    let defender = hero(&mut simulation, PlayerId::Two);
    (
        simulation,
        attacker,
        GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        },
    )
}

fn assert_windfury_legality(
    simulation: &mut Simulation,
    attacker: GameEntityId,
    action: &GameAction,
    allowed: bool,
) {
    let before = simulation.checkpoint().unwrap();
    let actions = simulation.legal_actions();
    assert_eq!(actions, simulation.legal_actions());
    assert_eq!(before, simulation.checkpoint().unwrap());
    assert_eq!(actions.contains(action), allowed);
    for attack in actions
        .iter()
        .filter(|candidate| matches!(candidate, GameAction::Attack { .. }))
    {
        assert_eq!(
            action_validation::validate_action(simulation.app.world(), attack).as_ref(),
            Ok(attack)
        );
    }
    assert_eq!(windfury_snapshot_exhausted(simulation, attacker), !allowed);
    if !allowed {
        assert_rejected_action_is_atomic(
            simulation,
            action.clone(),
            SimulationError::CannotAttack(attacker),
        );
    }
}

#[test]
fn windfury_and_ordinary_heroes_and_minions_have_bounded_attack_allowances() {
    for is_hero in [false, true] {
        for windfury in [false, true] {
            let (mut simulation, attacker, action) = windfury_fixture(is_hero, windfury);
            let allowance = if windfury { 2 } else { 1 };
            for spent in 0..allowance {
                assert_eq!(
                    windfury_attack_state(&simulation, attacker).attacks_this_turn,
                    spent
                );
                assert_windfury_legality(&mut simulation, attacker, &action, true);
                simulation.apply(action.clone()).unwrap();
            }
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                allowance
            );
            assert!(!windfury_attack_state(&simulation, attacker).readiness_blocked);
            assert_windfury_legality(&mut simulation, attacker, &action, false);
        }
    }
}

#[test]
fn windfury_gains_removals_silence_and_duplicate_grants_preserve_spent_attacks() {
    for is_hero in [false, true] {
        for innate in [false, true] {
            let (mut simulation, attacker, action) = windfury_fixture(is_hero, innate);
            simulation.apply(action.clone()).unwrap();
            for _ in 0..2 {
                keyword_grant(
                    &mut simulation,
                    attacker,
                    Keyword::Windfury,
                    crate::EnchantmentDuration::Permanent,
                );
            }
            assert_windfury_legality(&mut simulation, attacker, &action, true);
            super::effect_executor::attach_keyword_modifier(
                simulation.app.world_mut(),
                PlayerId::One,
                attacker,
                crate::KeywordModifier {
                    keyword: Keyword::Windfury,
                    granted: false,
                    silence_removable: true,
                },
                crate::EnchantmentDuration::Permanent,
            )
            .unwrap();
            assert_windfury_legality(&mut simulation, attacker, &action, false);
            keyword_grant(
                &mut simulation,
                attacker,
                Keyword::Windfury,
                crate::EnchantmentDuration::Permanent,
            );
            assert_windfury_legality(&mut simulation, attacker, &action, true);
            let mut silenced = simulation.fork().unwrap();
            silence_entity(silenced.app.world_mut(), attacker).unwrap();
            assert_eq!(
                windfury_attack_state(&silenced, attacker).attacks_this_turn,
                1
            );
            assert_windfury_legality(&mut silenced, attacker, &action, false);
            simulation.apply(action.clone()).unwrap();
            super::effect_executor::attach_keyword_modifier(
                simulation.app.world_mut(),
                PlayerId::One,
                attacker,
                crate::KeywordModifier {
                    keyword: Keyword::Windfury,
                    granted: false,
                    silence_removable: true,
                },
                crate::EnchantmentDuration::Permanent,
            )
            .unwrap();
            assert_windfury_legality(&mut simulation, attacker, &action, false);
            keyword_grant(
                &mut simulation,
                attacker,
                Keyword::Windfury,
                crate::EnchantmentDuration::Permanent,
            );
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                2
            );
            assert_windfury_legality(&mut simulation, attacker, &action, false);
        }
    }
}

#[test]
fn windfury_reactions_change_next_declaration_without_changing_attack_completion() {
    for event in [EventKind::Attack, EventKind::AfterAttack] {
        for granted in [false, true] {
            let mut simulation = simulation();
            let mut trigger = self_event_trigger(
                event,
                vec![Effect::AttachKeywordModifier {
                    targets: Selector::Source,
                    modifier: crate::KeywordModifier {
                        keyword: Keyword::Windfury,
                        granted,
                        silence_removable: true,
                    },
                    duration: crate::EnchantmentDuration::Permanent,
                }],
            );
            trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
            let mut card = Card::minion("Reactive Windfury", 0, 1, 20).with_triggers(vec![trigger]);
            if !granted {
                card = card.with_keyword(Keyword::Windfury);
            }
            let attacker = keyword_attacker(&mut simulation, card);
            let defender = hero(&mut simulation, PlayerId::Two);
            let action = GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender,
            };
            simulation.apply(action.clone()).unwrap();
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                1
            );
            assert_windfury_legality(&mut simulation, attacker, &action, granted);
            if granted {
                simulation.apply(action.clone()).unwrap();
                assert_windfury_legality(&mut simulation, attacker, &action, false);
            }
        }
    }
}

#[test]
fn windfury_first_attack_checkpoint_json_and_fork_continue_identically() {
    for is_hero in [false, true] {
        let (mut original, attacker, action) = windfury_fixture(is_hero, true);
        original.apply(action.clone()).unwrap();
        let checkpoint = original.checkpoint().unwrap();
        assert_eq!(checkpoint.schema_version, 16);
        let mut restored = Simulation::from_checkpoint(
            SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
        )
        .unwrap();
        let mut fork = original.fork().unwrap();
        for simulation in [&mut original, &mut restored, &mut fork] {
            assert_windfury_legality(simulation, attacker, &action, true);
            simulation.apply(action.clone()).unwrap();
            assert_windfury_legality(simulation, attacker, &action, false);
        }
        assert_eq!(
            original.checkpoint().unwrap(),
            restored.checkpoint().unwrap()
        );
        assert_eq!(original.checkpoint().unwrap(), fork.checkpoint().unwrap());
        assert_eq!(original.trace(), restored.trace());
        assert_eq!(original.trace(), fork.trace());
    }
}

#[test]
fn windfury_readiness_blocks_summons_and_spent_characters_until_natural_or_extra_turn() {
    for extra_turn in [false, true] {
        for spent in [0, 1] {
            let mut simulation = simulation();
            let attacker = spawn_card(
                simulation.app.world_mut(),
                PlayerId::One,
                Card::minion("New Windfury", 0, 1, 20).with_keyword(Keyword::Windfury),
                Zone::Play,
            )
            .unwrap();
            let entity = game_entity(simulation.app.world(), attacker).unwrap();
            simulation
                .app
                .world_mut()
                .get_mut::<AttackState>(entity)
                .unwrap()
                .attacks_this_turn = spent;
            let defender = hero(&mut simulation, PlayerId::Two);
            let action = GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender,
            };
            assert_windfury_legality(&mut simulation, attacker, &action, false);
            if extra_turn {
                execute_effect(
                    simulation.app.world_mut(),
                    &EffectContext {
                        source: None,
                        controller: PlayerId::One,
                        declared_target: None,
                        drawn_card: None,
                        origin: EffectOrigin::Other,
                    },
                    &Effect::ScheduleExtraTurns {
                        player: PlayerSelector::Controller,
                        count: 1,
                        timing: crate::ExtraTurnTiming::AfterCurrentTurn,
                    },
                )
                .unwrap();
            }
            simulation
                .apply(GameAction::EndTurn {
                    player: PlayerId::One,
                })
                .unwrap();
            if !extra_turn {
                simulation
                    .apply(GameAction::EndTurn {
                        player: PlayerId::Two,
                    })
                    .unwrap();
            }
            assert_eq!(simulation.snapshot().game.active_player, PlayerId::One);
            assert_eq!(
                windfury_attack_state(&simulation, attacker),
                AttackState {
                    attacks_this_turn: 0,
                    readiness_blocked: false
                }
            );
            for _ in 0..2 {
                simulation.apply(action.clone()).unwrap();
            }
            assert_windfury_legality(&mut simulation, attacker, &action, false);
        }
    }
}

#[test]
fn windfury_in_play_copy_is_fresh_and_readiness_blocked() {
    let (mut simulation, attacker, action) = windfury_fixture(false, true);
    simulation.apply(action).unwrap();
    let before = simulation.snapshot().players[0].board.clone();
    super::effect_executor::copy_entity(
        simulation.app.world_mut(),
        crate::CopyRequest {
            source: attacker,
            originating_source: None,
            controller: PlayerId::One,
            destination: Zone::Play,
            board_index: None,
            policy: crate::CopyStatePolicy::InPlayState,
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    let copy = *simulation.snapshot().players[0]
        .board
        .iter()
        .find(|id| !before.contains(id))
        .unwrap();
    assert_eq!(
        windfury_attack_state(&simulation, copy),
        AttackState {
            attacks_this_turn: 0,
            readiness_blocked: true
        }
    );
    assert!(crate::aura::has_keyword(
        simulation.app.world(),
        game_entity(simulation.app.world(), copy).unwrap(),
        Keyword::Windfury
    ));
    let defender = hero(&mut simulation, PlayerId::Two);
    assert_windfury_legality(
        &mut simulation,
        copy,
        &GameAction::Attack {
            player: PlayerId::One,
            attacker: copy,
            defender,
        },
        false,
    );
}

fn charge_rush_fixture(
    keywords: &[Keyword],
    granted: bool,
    summoned: bool,
) -> (Simulation, GameEntityId, GameAction, GameAction) {
    let mut simulation = simulation();
    let mut card = Card::minion("Immediate attacker", 0, 1, 20);
    if !granted {
        for keyword in keywords {
            card = card.with_keyword(*keyword);
        }
    }
    let attacker = if summoned {
        execute_effect(
            simulation.app.world_mut(),
            &EffectContext {
                source: None,
                controller: PlayerId::One,
                declared_target: None,
                drawn_card: None,
                origin: EffectOrigin::Other,
            },
            &Effect::Summon {
                player: PlayerSelector::Controller,
                card,
                board_index: None,
            },
        )
        .unwrap();
        drive_resolution(simulation.app.world_mut()).unwrap();
        card_named(&mut simulation, "Immediate attacker")
    } else {
        let id = spawn_card(simulation.app.world_mut(), PlayerId::One, card, Zone::Hand).unwrap();
        simulation
            .apply(play_declaration(id, None, None, None))
            .unwrap();
        id
    };
    if granted {
        for keyword in keywords {
            keyword_grant(
                &mut simulation,
                attacker,
                *keyword,
                crate::EnchantmentDuration::Permanent,
            );
        }
    }
    let defender = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Surviving defender", 0, 0, 20),
        Zone::Play,
    )
    .unwrap();
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    (
        simulation,
        attacker,
        GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        },
        GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: enemy_hero,
        },
    )
}

fn assert_charge_rush_actions(
    simulation: &mut Simulation,
    minion_attack: &GameAction,
    hero_attack: &GameAction,
    minion_allowed: bool,
    hero_allowed: bool,
) {
    let before = simulation.checkpoint().unwrap();
    let actions = simulation.legal_actions();
    assert_eq!(actions, simulation.legal_actions());
    assert_eq!(before, simulation.checkpoint().unwrap());
    assert_eq!(actions.contains(minion_attack), minion_allowed);
    assert_eq!(actions.contains(hero_attack), hero_allowed);
    for action in actions
        .iter()
        .filter(|action| matches!(action, GameAction::Attack { .. }))
    {
        assert_eq!(
            action_validation::validate_action(simulation.app.world(), action).as_ref(),
            Ok(action)
        );
    }
}

fn charge_rush_modifier(keyword: Keyword, granted: bool) -> Effect {
    Effect::AttachKeywordModifier {
        targets: Selector::Source,
        modifier: crate::KeywordModifier {
            keyword,
            granted,
            silence_removable: true,
        },
        duration: crate::EnchantmentDuration::Permanent,
    }
}

fn change_charge_rush(
    simulation: &mut Simulation,
    attacker: GameEntityId,
    keyword: Keyword,
    granted: bool,
) {
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: Some(attacker),
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &charge_rush_modifier(keyword, granted),
    )
    .unwrap();
}

#[test]
fn charge_rush_played_and_effect_summoned_keyword_matrix_is_pure_and_atomic() {
    for keywords in [
        vec![],
        vec![Keyword::Rush],
        vec![Keyword::Charge],
        vec![Keyword::Rush, Keyword::Charge],
    ] {
        for granted in [false, true] {
            for summoned in [false, true] {
                let (mut simulation, attacker, minion, face) =
                    charge_rush_fixture(&keywords, granted, summoned);
                let can_attack = !keywords.is_empty();
                let can_face = keywords.contains(&Keyword::Charge);
                assert_eq!(
                    windfury_attack_state(&simulation, attacker),
                    AttackState {
                        attacks_this_turn: 0,
                        readiness_blocked: true
                    }
                );
                assert_charge_rush_actions(&mut simulation, &minion, &face, can_attack, can_face);
                assert_eq!(
                    windfury_snapshot_exhausted(&mut simulation, attacker),
                    !can_attack
                );
                if !can_face {
                    let GameAction::Attack { defender, .. } = face else {
                        unreachable!()
                    };
                    let error = if can_attack {
                        SimulationError::InvalidDefender(defender)
                    } else {
                        SimulationError::CannotAttack(attacker)
                    };
                    assert_rejected_action_is_atomic(&mut simulation, face.clone(), error);
                }
                if can_attack {
                    let mut face_fork = simulation.fork().unwrap();
                    simulation.apply(minion.clone()).unwrap();
                    assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
                    assert_eq!(
                        windfury_attack_state(&simulation, attacker),
                        AttackState {
                            attacks_this_turn: 1,
                            readiness_blocked: true
                        }
                    );
                    if can_face {
                        face_fork.apply(face).unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn charge_rush_windfury_and_dynamic_keywords_preserve_history_and_allowance() {
    for keyword in [Keyword::Rush, Keyword::Charge] {
        let (mut simulation, attacker, minion, face) =
            charge_rush_fixture(&[keyword, Keyword::Windfury], false, false);
        for spent in 0..2 {
            assert_charge_rush_actions(
                &mut simulation,
                &minion,
                &face,
                true,
                keyword == Keyword::Charge,
            );
            simulation
                .apply(if keyword == Keyword::Charge {
                    face.clone()
                } else {
                    minion.clone()
                })
                .unwrap();
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                spent + 1
            );
        }
        for keyword in [Keyword::Rush, Keyword::Charge, Keyword::Windfury] {
            change_charge_rush(&mut simulation, attacker, keyword, false);
            change_charge_rush(&mut simulation, attacker, keyword, true);
            assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
        }
    }
    let (mut simulation, attacker, minion, face) =
        charge_rush_fixture(&[Keyword::Rush, Keyword::Windfury], false, true);
    simulation.apply(minion.clone()).unwrap();
    let history = windfury_attack_state(&simulation, attacker);
    for _ in 0..2 {
        change_charge_rush(&mut simulation, attacker, Keyword::Charge, true);
    }
    assert_eq!(windfury_attack_state(&simulation, attacker), history);
    assert_charge_rush_actions(&mut simulation, &minion, &face, true, true);
    change_charge_rush(&mut simulation, attacker, Keyword::Charge, false);
    assert_charge_rush_actions(&mut simulation, &minion, &face, true, false);
    change_charge_rush(&mut simulation, attacker, Keyword::Charge, true);
    change_charge_rush(&mut simulation, attacker, Keyword::Rush, false);
    assert_charge_rush_actions(&mut simulation, &minion, &face, true, true);
    let mut silenced = simulation.fork().unwrap();
    silence_entity(silenced.app.world_mut(), attacker).unwrap();
    assert_eq!(windfury_attack_state(&silenced, attacker), history);
    assert_charge_rush_actions(&mut silenced, &minion, &face, false, false);
    simulation.apply(face.clone()).unwrap();
    change_charge_rush(&mut simulation, attacker, Keyword::Rush, true);
    assert_eq!(
        windfury_attack_state(&simulation, attacker),
        AttackState {
            attacks_this_turn: 2,
            readiness_blocked: true
        }
    );
    assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
}

#[test]
fn charge_rush_expiry_and_natural_or_extra_turn_refresh_keep_separate_history() {
    for extra_turn in [false, true] {
        let (mut simulation, attacker, minion, face) =
            charge_rush_fixture(&[Keyword::Rush, Keyword::Windfury], false, true);
        keyword_grant(
            &mut simulation,
            attacker,
            Keyword::Charge,
            crate::EnchantmentDuration::EndOfTurn(PlayerId::One),
        );
        simulation.apply(face.clone()).unwrap();
        if extra_turn {
            execute_effect(
                simulation.app.world_mut(),
                &EffectContext {
                    source: None,
                    controller: PlayerId::One,
                    declared_target: None,
                    drawn_card: None,
                    origin: EffectOrigin::Other,
                },
                &Effect::ScheduleExtraTurns {
                    player: PlayerSelector::Controller,
                    count: 1,
                    timing: crate::ExtraTurnTiming::AfterCurrentTurn,
                },
            )
            .unwrap();
        }
        simulation
            .apply(GameAction::EndTurn {
                player: PlayerId::One,
            })
            .unwrap();
        assert!(!crate::aura::has_keyword(
            simulation.app.world(),
            game_entity(simulation.app.world(), attacker).unwrap(),
            Keyword::Charge
        ));
        if !extra_turn {
            assert_eq!(
                windfury_attack_state(&simulation, attacker),
                AttackState {
                    attacks_this_turn: 1,
                    readiness_blocked: true
                }
            );
            simulation
                .apply(GameAction::EndTurn {
                    player: PlayerId::Two,
                })
                .unwrap();
        }
        assert_eq!(
            windfury_attack_state(&simulation, attacker),
            AttackState {
                attacks_this_turn: 0,
                readiness_blocked: false
            }
        );
        change_charge_rush(&mut simulation, attacker, Keyword::Rush, false);
        change_charge_rush(&mut simulation, attacker, Keyword::Rush, true);
        assert_charge_rush_actions(&mut simulation, &minion, &face, true, true);
        simulation.apply(face.clone()).unwrap();
        simulation.apply(face.clone()).unwrap();
        assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
    }
}

#[test]
fn charge_rush_respects_target_protection_and_empty_board_snapshot_semantics() {
    for keyword in [Keyword::Rush, Keyword::Charge] {
        for protection in [Keyword::Stealth, Keyword::Immune] {
            let (mut simulation, attacker, minion, face) =
                charge_rush_fixture(&[keyword], false, false);
            let GameAction::Attack { defender, .. } = minion else {
                unreachable!()
            };
            keyword_grant(
                &mut simulation,
                defender,
                protection,
                crate::EnchantmentDuration::Permanent,
            );
            assert_charge_rush_actions(
                &mut simulation,
                &minion,
                &face,
                false,
                keyword == Keyword::Charge,
            );
            assert!(!windfury_snapshot_exhausted(&mut simulation, attacker));
            assert_rejected_action_is_atomic(
                &mut simulation,
                minion.clone(),
                SimulationError::InvalidDefender(defender),
            );
            crate::zone::move_entity_with_request(
                simulation.app.world_mut(),
                ZoneMoveRequest {
                    entity: defender,
                    destination_controller: PlayerId::Two,
                    destination: Zone::Hand,
                    position: None,
                    kind: ZoneMovementKind::Normal,
                },
            )
            .unwrap();
            assert!(!windfury_snapshot_exhausted(&mut simulation, attacker));
            if keyword == Keyword::Rush {
                assert!(!simulation.legal_actions().iter().any(|action| matches!(action, GameAction::Attack { attacker: id, .. } if *id == attacker)));
            }
        }
        let (mut simulation, attacker, minion, face) = charge_rush_fixture(&[keyword], false, true);
        let guard = spawn_card(
            simulation.app.world_mut(),
            PlayerId::Two,
            Card::minion("Guard", 0, 0, 20).with_keyword(Keyword::Taunt),
            Zone::Play,
        )
        .unwrap();
        assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
        let guard_action = GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender: guard,
        };
        assert!(simulation.legal_actions().contains(&guard_action));
        let entity = game_entity(simulation.app.world(), attacker).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<CurrentStats>(entity)
            .unwrap()
            .attack = 0;
        assert_rejected_action_is_atomic(
            &mut simulation,
            guard_action,
            SimulationError::CannotAttack(attacker),
        );
    }
}

#[test]
fn charge_rush_does_not_bypass_hero_readiness_or_windfury_allowance() {
    for keyword in [Keyword::Charge, Keyword::Rush] {
        let (mut simulation, attacker, face) = windfury_fixture(true, true);
        keyword_grant(
            &mut simulation,
            attacker,
            keyword,
            crate::EnchantmentDuration::Permanent,
        );
        let entity = game_entity(simulation.app.world(), attacker).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<AttackState>(entity)
            .unwrap()
            .readiness_blocked = true;
        assert_windfury_legality(&mut simulation, attacker, &face, false);
        simulation
            .app
            .world_mut()
            .get_mut::<AttackState>(entity)
            .unwrap()
            .readiness_blocked = false;
        for _ in 0..2 {
            simulation.apply(face.clone()).unwrap();
        }
        assert_windfury_legality(&mut simulation, attacker, &face, false);
    }
}

#[test]
fn charge_rush_attack_and_after_attack_reactions_only_change_next_declaration() {
    for keyword in [Keyword::Charge, Keyword::Rush] {
        for event in [EventKind::Attack, EventKind::AfterAttack] {
            let (mut simulation, attacker, minion, face) =
                charge_rush_fixture(&[keyword, Keyword::Windfury], false, false);
            let mut trigger = self_event_trigger(event, vec![charge_rush_modifier(keyword, false)]);
            trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
            let entity = game_entity(simulation.app.world(), attacker).unwrap();
            simulation
                .app
                .world_mut()
                .entity_mut(entity)
                .insert(RuntimeTriggers(vec![trigger]));
            simulation
                .apply(if keyword == Keyword::Charge {
                    face.clone()
                } else {
                    minion.clone()
                })
                .unwrap();
            assert_eq!(
                windfury_attack_state(&simulation, attacker),
                AttackState {
                    attacks_this_turn: 1,
                    readiness_blocked: true
                }
            );
            assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
        }
    }
    let (mut simulation, attacker, minion, face) =
        charge_rush_fixture(&[Keyword::Rush, Keyword::Windfury], false, true);
    let mut trigger = self_event_trigger(
        EventKind::AfterAttack,
        vec![charge_rush_modifier(Keyword::Charge, true)],
    );
    trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
    let entity = game_entity(simulation.app.world(), attacker).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    simulation.apply(minion.clone()).unwrap();
    assert_charge_rush_actions(&mut simulation, &minion, &face, true, true);
    simulation.apply(face.clone()).unwrap();
    assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
}

#[test]
fn charge_rush_suspended_attack_keyword_loss_restores_and_finishes_exactly_once() {
    for keyword in [Keyword::Charge, Keyword::Rush] {
        let (mut original, attacker, minion, face) =
            charge_rush_fixture(&[keyword, Keyword::Windfury], false, true);
        let mut trigger = self_event_trigger(
            EventKind::Attack,
            vec![
                charge_rush_modifier(keyword, false),
                Effect::Choose {
                    id: ChoiceId(80),
                    player: PlayerSelector::Controller,
                    options: vec![hearthstone_simulator_core::EffectChoiceOption {
                        id: ChoiceId(81),
                        effects: vec![],
                    }],
                },
            ],
        );
        trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
        let entity = game_entity(original.app.world(), attacker).unwrap();
        original
            .app
            .world_mut()
            .entity_mut(entity)
            .insert(RuntimeTriggers(vec![trigger]));
        let action = if keyword == Keyword::Charge {
            face.clone()
        } else {
            minion.clone()
        };
        let GameAction::Attack { defender, .. } = action else {
            unreachable!()
        };
        original.apply(action.clone()).unwrap();
        assert_eq!(
            original.snapshot().game.status,
            SimulationStatus::AwaitingChoice
        );
        assert_eq!(
            windfury_attack_state(&original, attacker).attacks_this_turn,
            0
        );
        assert!(!crate::aura::has_keyword(
            original.app.world(),
            entity,
            keyword
        ));
        let checkpoint = original.checkpoint().unwrap();
        assert_eq!(checkpoint.schema_version, 16);
        let mut restored = Simulation::from_checkpoint(
            SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
        )
        .unwrap();
        let mut fork = original.fork().unwrap();
        assert_eq!(original.legal_actions(), restored.legal_actions());
        assert_eq!(original.legal_actions(), fork.legal_actions());
        for candidate in [&mut original, &mut restored, &mut fork] {
            candidate.choose(ChoiceId(81)).unwrap();
            assert_eq!(
                windfury_attack_state(candidate, attacker),
                AttackState {
                    attacks_this_turn: 1,
                    readiness_blocked: true
                }
            );
            assert_eq!(
                candidate
                    .snapshot()
                    .objects
                    .iter()
                    .find(|object| object.id == defender)
                    .unwrap()
                    .damage,
                1
            );
            assert_charge_rush_actions(candidate, &minion, &face, false, false);
            change_charge_rush(candidate, attacker, Keyword::Charge, true);
            assert_charge_rush_actions(candidate, &minion, &face, true, true);
            candidate.apply(face.clone()).unwrap();
            candidate.choose(ChoiceId(81)).unwrap();
            assert_eq!(
                windfury_attack_state(candidate, attacker).attacks_this_turn,
                2
            );
            assert_charge_rush_actions(candidate, &minion, &face, false, false);
            candidate.assert_invariants().unwrap();
        }
        assert_eq!(
            original.checkpoint().unwrap(),
            restored.checkpoint().unwrap()
        );
        assert_eq!(original.checkpoint().unwrap(), fork.checkpoint().unwrap());
        assert_eq!(original.trace(), restored.trace());
        assert_eq!(original.trace(), fork.trace());
    }
}

#[test]
fn charge_rush_does_not_bypass_ownership_turn_or_zone_restrictions() {
    for keyword in [Keyword::Rush, Keyword::Charge] {
        let (mut simulation, attacker, minion, face) =
            charge_rush_fixture(&[keyword], false, false);
        let GameAction::Attack { defender, .. } = minion else {
            unreachable!()
        };
        assert_rejected_action_is_atomic(
            &mut simulation,
            GameAction::Attack {
                player: PlayerId::Two,
                attacker,
                defender,
            },
            SimulationError::NotPlayersTurn(PlayerId::Two),
        );
        let own_hero = hero(&mut simulation, PlayerId::One);
        assert_rejected_action_is_atomic(
            &mut simulation,
            GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender: own_hero,
            },
            SimulationError::InvalidDefender(own_hero),
        );
        keyword_grant(
            &mut simulation,
            defender,
            keyword,
            crate::EnchantmentDuration::Permanent,
        );
        assert_rejected_action_is_atomic(
            &mut simulation,
            GameAction::Attack {
                player: PlayerId::One,
                attacker: defender,
                defender: attacker,
            },
            SimulationError::NotControlled { entity: defender },
        );
        crate::zone::move_entity_with_request(
            simulation.app.world_mut(),
            ZoneMoveRequest {
                entity: attacker,
                destination_controller: PlayerId::One,
                destination: Zone::Hand,
                position: None,
                kind: ZoneMovementKind::Normal,
            },
        )
        .unwrap();
        assert_rejected_action_is_atomic(
            &mut simulation,
            minion.clone(),
            SimulationError::WrongZone {
                entity: attacker,
                expected: Zone::Play,
            },
        );
        assert_charge_rush_actions(&mut simulation, &minion, &face, false, false);
    }
}

fn is_frozen(simulation: &Simulation, id: GameEntityId) -> bool {
    crate::aura::has_keyword(
        simulation.app.world(),
        game_entity(simulation.app.world(), id).unwrap(),
        Keyword::Frozen,
    )
}

fn freeze(simulation: &mut Simulation, id: GameEntityId) {
    keyword_grant(
        simulation,
        id,
        Keyword::Frozen,
        crate::EnchantmentDuration::Permanent,
    );
}

fn end_active_turn(simulation: &mut Simulation) {
    let player = simulation.snapshot().game.active_player;
    simulation.apply(GameAction::EndTurn { player }).unwrap();
}

#[test]
fn frozen_declarations_are_atomic_but_defensive_damage_and_readiness_are_preserved() {
    for is_hero in [false, true] {
        let (mut simulation, attacker, action) = windfury_fixture(is_hero, true);
        let state = windfury_attack_state(&simulation, attacker);
        freeze(&mut simulation, attacker);
        assert!(!windfury_snapshot_exhausted(&mut simulation, attacker));
        let before = simulation.checkpoint().unwrap();
        assert!(!simulation.legal_actions().contains(&action));
        assert_eq!(before, simulation.checkpoint().unwrap());
        assert_rejected_action_is_atomic(
            &mut simulation,
            action,
            SimulationError::CannotAttack(attacker),
        );
        assert_eq!(state, windfury_attack_state(&simulation, attacker));
    }
    let (mut simulation, attacker, _) = windfury_fixture(false, false);
    end_active_turn(&mut simulation);
    freeze(&mut simulation, attacker);
    let enemy = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Enemy", 0, 1, 5).with_keyword(Keyword::Charge),
        Zone::Play,
    )
    .unwrap();
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::Two,
            attacker: enemy,
            defender: attacker,
        })
        .unwrap();
    assert_eq!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|o| o.id == enemy)
            .unwrap()
            .damage,
        1
    );
    assert!(is_frozen(&simulation, attacker));
}

#[test]
fn frozen_thaw_matrix_uses_current_attack_allowance_and_controller_turn() {
    for is_hero in [false, true] {
        for windfury in [false, true] {
            for spent in 0..=2 {
                let (mut simulation, attacker, _) = windfury_fixture(is_hero, windfury);
                let entity = game_entity(simulation.app.world(), attacker).unwrap();
                simulation
                    .app
                    .world_mut()
                    .get_mut::<AttackState>(entity)
                    .unwrap()
                    .attacks_this_turn = spent;
                freeze(&mut simulation, attacker);
                freeze(&mut simulation, attacker);
                end_active_turn(&mut simulation);
                let exhausted = spent >= if windfury { 2 } else { 1 };
                assert_eq!(is_frozen(&simulation, attacker), exhausted);
                end_active_turn(&mut simulation);
                assert_eq!(is_frozen(&simulation, attacker), exhausted);
                end_active_turn(&mut simulation);
                assert!(!is_frozen(&simulation, attacker));
            }
        }
    }
}

#[test]
fn frozen_fresh_minions_use_live_charge_rush_and_enemy_minion_presence() {
    for keywords in [
        vec![],
        vec![Keyword::Charge],
        vec![Keyword::Rush],
        vec![Keyword::Rush, Keyword::Charge],
    ] {
        for enemy_present in [false, true] {
            let (mut simulation, attacker, minion, _) = charge_rush_fixture(&keywords, false, true);
            let GameAction::Attack { defender, .. } = minion else {
                unreachable!()
            };
            if !enemy_present {
                crate::zone::move_entity(
                    simulation.app.world_mut(),
                    defender,
                    Zone::Graveyard,
                    None,
                )
                .unwrap();
            }
            freeze(&mut simulation, attacker);
            let entity = game_entity(simulation.app.world(), attacker).unwrap();
            simulation
                .app
                .world_mut()
                .get_mut::<CurrentStats>(entity)
                .unwrap()
                .attack = 0;
            end_active_turn(&mut simulation);
            let thaws = keywords.contains(&Keyword::Charge)
                || (keywords.contains(&Keyword::Rush) && enemy_present);
            assert_eq!(is_frozen(&simulation, attacker), !thaws);
        }
    }
    let (mut simulation, attacker, _) = windfury_fixture(false, false);
    let entity = game_entity(simulation.app.world(), attacker).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<AttackState>(entity)
        .unwrap()
        .attacks_this_turn = 1;
    freeze(&mut simulation, attacker);
    keyword_grant(
        &mut simulation,
        attacker,
        Keyword::Windfury,
        crate::EnchantmentDuration::EndOfTurn(PlayerId::One),
    );
    end_active_turn(&mut simulation);
    assert!(!is_frozen(&simulation, attacker));
    assert!(!crate::aura::has_keyword(
        simulation.app.world(),
        entity,
        Keyword::Windfury
    ));
}

#[test]
fn frozen_thaw_consumes_grants_and_lifecycle_changes_remove_freeze() {
    let (mut simulation, attacker, _) = windfury_fixture(false, false);
    freeze(&mut simulation, attacker);
    end_active_turn(&mut simulation);
    crate::enchantment::recalculate_keywords(simulation.app.world_mut(), attacker);
    assert!(!is_frozen(&simulation, attacker));
    freeze(&mut simulation, attacker);
    assert!(is_frozen(&simulation, attacker));
    let mut silenced = simulation.fork().unwrap();
    silence_entity(silenced.app.world_mut(), attacker).unwrap();
    assert!(!is_frozen(&silenced, attacker));
    let mut transformed = simulation.fork().unwrap();
    transform_entity(
        transformed.app.world_mut(),
        attacker,
        Card::minion("Thawed form", 0, 1, 5),
        TransformKind::Spell,
    )
    .unwrap();
    assert!(!is_frozen(&transformed, attacker));
    let mut moved = simulation.fork().unwrap();
    crate::zone::move_entity_with_request(
        moved.app.world_mut(),
        ZoneMoveRequest {
            entity: attacker,
            destination_controller: PlayerId::One,
            destination: Zone::Hand,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();
    assert!(!is_frozen(&moved, attacker));
    let before = simulation
        .snapshot()
        .objects
        .iter()
        .map(|o| o.id)
        .collect::<Vec<_>>();
    super::effect_executor::copy_entity(
        simulation.app.world_mut(),
        crate::CopyRequest {
            source: attacker,
            originating_source: None,
            controller: PlayerId::One,
            destination: Zone::Play,
            board_index: None,
            policy: crate::CopyStatePolicy::InPlayState,
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    let copy = simulation
        .snapshot()
        .objects
        .iter()
        .find(|o| o.name == "Windfury fixture" && !before.contains(&o.id))
        .unwrap()
        .id;
    assert!(is_frozen(&simulation, copy));
    assert!(windfury_attack_state(&simulation, copy).readiness_blocked);
}

#[test]
fn frozen_suspended_end_turn_thaws_after_reactions_and_restores_exactly() {
    let (mut original, attacker, _) = windfury_fixture(false, false);
    let mut trigger = self_event_trigger(
        EventKind::TurnEnded,
        vec![
            charge_rush_modifier(Keyword::Frozen, true),
            Effect::Choose {
                id: ChoiceId(90),
                player: PlayerSelector::Controller,
                options: vec![hearthstone_simulator_core::EffectChoiceOption {
                    id: ChoiceId(91),
                    effects: vec![],
                }],
            },
        ],
    );
    trigger.conditions.clear();
    let entity = game_entity(original.app.world(), attacker).unwrap();
    original
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    end_active_turn(&mut original);
    assert!(is_frozen(&original, attacker));
    assert!(original.pending_choice().is_some());
    let checkpoint = original.checkpoint().unwrap();
    assert!(checkpoint.to_json().unwrap().contains("ThawCharacters"));
    let mut restored = Simulation::from_checkpoint(
        SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
    )
    .unwrap();
    let mut fork = original.fork().unwrap();
    for candidate in [&mut original, &mut restored, &mut fork] {
        candidate.choose(ChoiceId(91)).unwrap();
        assert!(!is_frozen(candidate, attacker));
        candidate.assert_invariants().unwrap();
    }
    assert_eq!(
        original.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
    assert_eq!(original.checkpoint().unwrap(), fork.checkpoint().unwrap());
}

#[test]
fn frozen_during_attack_reactions_preserves_accepted_attack() {
    let (mut original, attacker, action) = windfury_fixture(false, true);
    let mut trigger = self_event_trigger(
        EventKind::Attack,
        vec![
            charge_rush_modifier(Keyword::Frozen, true),
            Effect::Choose {
                id: ChoiceId(90),
                player: PlayerSelector::Controller,
                options: vec![hearthstone_simulator_core::EffectChoiceOption {
                    id: ChoiceId(91),
                    effects: vec![],
                }],
            },
        ],
    );
    trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
    let entity = game_entity(original.app.world(), attacker).unwrap();
    original
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    original.apply(action.clone()).unwrap();
    assert!(is_frozen(&original, attacker));
    let mut fork = original.fork().unwrap();
    for candidate in [&mut original, &mut fork] {
        candidate.choose(ChoiceId(91)).unwrap();
        assert_eq!(
            windfury_attack_state(candidate, attacker).attacks_this_turn,
            1
        );
        let defender = hero(candidate, PlayerId::Two);
        assert_eq!(
            candidate
                .snapshot()
                .objects
                .iter()
                .find(|o| o.id == defender)
                .unwrap()
                .damage,
            1
        );
        assert!(!candidate.legal_actions().contains(&action));
    }
    assert_eq!(original.checkpoint().unwrap(), fork.checkpoint().unwrap());
}

#[test]
fn frozen_extra_turn_is_a_thaw_opportunity_and_hero_power_remains_usable() {
    let (mut simulation, attacker, action) = windfury_fixture(false, false);
    simulation.apply(action).unwrap();
    freeze(&mut simulation, attacker);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 1,
            timing: crate::ExtraTurnTiming::AfterCurrentTurn,
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    end_active_turn(&mut simulation);
    assert_eq!(simulation.snapshot().game.active_player, PlayerId::One);
    assert!(is_frozen(&simulation, attacker));
    end_active_turn(&mut simulation);
    assert!(!is_frozen(&simulation, attacker));

    let mut simulation = super::test_support::simulation();
    let hero = hero(&mut simulation, PlayerId::One);
    freeze(&mut simulation, hero);
    let power = simulation.snapshot().players[0].hero_power.unwrap();
    let power_entity = game_entity(simulation.app.world(), power).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<super::card_runtime::CardRuntime>(power_entity)
        .unwrap()
        .cost = 0;
    let action = GameAction::UseHeroPower {
        player: PlayerId::One,
        power,
        target: None,
    };
    assert!(simulation.legal_actions().contains(&action));
    simulation.apply(action).unwrap();
    assert!(is_frozen(&simulation, hero));
}

#[test]
fn frozen_thaw_observes_end_turn_deaths_and_live_readiness_keywords() {
    let (mut simulation, attacker, minion, _) = charge_rush_fixture(&[Keyword::Rush], false, true);
    let GameAction::Attack { defender, .. } = minion else {
        unreachable!()
    };
    freeze(&mut simulation, attacker);
    let mut trigger = self_event_trigger(
        EventKind::TurnEnded,
        vec![Effect::Destroy {
            targets: Selector::Source,
        }],
    );
    trigger.conditions.clear();
    let entity = game_entity(simulation.app.world(), defender).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    end_active_turn(&mut simulation);
    assert!(is_frozen(&simulation, attacker));
    assert_eq!(
        *simulation.app.world().get::<Zone>(entity).unwrap(),
        Zone::Graveyard
    );

    for (initial, changed, granted, thawed) in [
        (Keyword::Rush, Keyword::Charge, true, true),
        (Keyword::Charge, Keyword::Charge, false, false),
        (Keyword::Rush, Keyword::Rush, false, false),
    ] {
        let (mut simulation, attacker, _, _) = charge_rush_fixture(&[initial], false, true);
        freeze(&mut simulation, attacker);
        change_charge_rush(&mut simulation, attacker, changed, granted);
        end_active_turn(&mut simulation);
        assert_eq!(is_frozen(&simulation, attacker), !thawed);
    }
}

fn weapon_fixture(cards: Vec<Card>) -> Simulation {
    Simulation::new([
        PlayerConfig::new("One", cards),
        PlayerConfig::new("Two", vec![]),
    ])
}

fn weapon_attack(sim: &mut Simulation) {
    let attacker = hero(sim, PlayerId::One);
    let defender = hero(sim, PlayerId::Two);
    sim.apply(GameAction::Attack {
        player: PlayerId::One,
        attacker,
        defender,
    })
    .unwrap();
}

fn weapon_play(sim: &mut Simulation) -> GameEntityId {
    let card = hand_card(sim, PlayerId::One);
    sim.apply(play_declaration(card, None, None, None)).unwrap();
    card
}

fn weapon_pause() -> Effect {
    Effect::Choose {
        id: ChoiceId(501),
        player: PlayerSelector::Controller,
        options: vec![hearthstone_simulator_core::EffectChoiceOption {
            id: ChoiceId(502),
            effects: vec![],
        }],
    }
}

fn weapon_restore_and_finish(original: &mut Simulation) {
    let checkpoint = original.checkpoint().unwrap();
    let mut restored = Simulation::from_checkpoint(
        SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
    )
    .unwrap();
    let mut fork = original.fork().unwrap();
    for sim in [&mut *original, &mut restored, &mut fork] {
        sim.choose(ChoiceId(502)).unwrap();
        sim.assert_invariants().unwrap();
    }
    assert_eq!(
        original.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
    assert_eq!(original.checkpoint().unwrap(), fork.checkpoint().unwrap());
}

fn weapon_replacement_effect() -> Effect {
    Effect::ReplaceHero {
        player: PlayerSelector::Controller,
        replacement: Box::new(crate::HeroReplacement {
            hero: Card::hero("Replacement", 30),
            hero_power: Card::hero_power("Power", 2),
            armor_gain: 0,
            health: crate::HeroHealthPolicy::Preserve,
            class: crate::HeroClassPolicy::Keep,
            weapon: Some(Card::weapon("Replacement blade", 0, 5, 3)),
        }),
    }
}

#[test]
fn weapon_two_attacks_break_and_run_deathrattle_once() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 1, 3, 2).with_deathrattle(vec![
        Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 4,
            temporary: true,
        },
    ])]);
    let weapon = weapon_play(&mut sim);
    assert_eq!(sim.snapshot().players[0].available_resources, 0);
    assert_eq!(sim.snapshot().players[0].weapon, Some(weapon));
    weapon_attack(&mut sim);
    assert_eq!(sim.snapshot().players[1].health, 27);
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == weapon)
            .unwrap()
            .durability,
        Some(1)
    );
    end_active_turn(&mut sim);
    end_active_turn(&mut sim);
    weapon_attack(&mut sim);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.players[1].health, 24);
    assert_eq!(snapshot.players[0].weapon, None);
    assert_eq!(snapshot.players[0].temporary_resources, 4);
    assert_eq!(
        snapshot
            .deaths
            .iter()
            .filter(|d| d.entity == weapon)
            .count(),
        1
    );
    assert_eq!(
        snapshot
            .objects
            .iter()
            .find(|o| o.id == weapon)
            .unwrap()
            .zone,
        Zone::Graveyard
    );
    sim.assert_invariants().unwrap();
}

#[test]
fn weapon_replacement_preserves_attack_usage_and_does_not_use_board_slots() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("First", 0, 2, 2),
        Card::weapon("Second", 0, 5, 3),
    ]);
    for _ in 0..7 {
        spawn_card(
            sim.app.world_mut(),
            PlayerId::One,
            Card::minion("Blocker", 0, 0, 1),
            Zone::Play,
        )
        .unwrap();
    }
    let old = weapon_play(&mut sim);
    weapon_attack(&mut sim);
    let new = weapon_play(&mut sim);
    assert_eq!(sim.snapshot().players[0].weapon, Some(new));
    assert_eq!(sim.snapshot().players[0].board.len(), 7);
    assert!(sim.snapshot().deaths.iter().any(|d| d.entity == old));
    let attacker = hero(&mut sim, PlayerId::One);
    assert!(
        !sim.legal_actions()
            .iter()
            .any(|a| matches!(a, GameAction::Attack { attacker: id, .. } if *id == attacker))
    );
}

#[test]
fn weapon_attack_adds_personal_attack_and_is_inactive_off_turn() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
    weapon_play(&mut sim);
    let hero_id = hero(&mut sim, PlayerId::One);
    let entity = game_entity(sim.app.world(), hero_id).unwrap();
    sim.app
        .world_mut()
        .get_mut::<crate::BaseStats>(entity)
        .unwrap()
        .attack = 2;
    sim.app
        .world_mut()
        .get_mut::<CurrentStats>(entity)
        .unwrap()
        .attack = 2;
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == hero_id)
            .unwrap()
            .attack,
        Some(5)
    );
    weapon_attack(&mut sim);
    assert_eq!(sim.snapshot().players[1].health, 25);
    end_active_turn(&mut sim);
    // Aura boundaries recalculate base stats; set personal Attack for the off-turn query.
    sim.app
        .world_mut()
        .get_mut::<crate::BaseStats>(entity)
        .unwrap()
        .attack = 2;
    sim.app
        .world_mut()
        .get_mut::<CurrentStats>(entity)
        .unwrap()
        .attack = 2;
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == hero_id)
            .unwrap()
            .attack,
        Some(2)
    );
}

#[test]
fn weapon_replacement_keeps_old_observer_through_play_effects_and_restores_scope() {
    let old = Card::weapon("Old", 0, 1, 2).with_deathrattle(vec![Effect::GainResource {
        player: PlayerSelector::Controller,
        amount: 7,
        temporary: true,
    }]);
    let new = Card::weapon("New", 0, 3, 2).with_effects(vec![weapon_pause()]);
    let mut sim = weapon_fixture(vec![old, new]);
    let old_id = weapon_play(&mut sim);
    let new_id = weapon_play(&mut sim);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.players[0].weapon, Some(new_id));
    assert_eq!(
        snapshot
            .objects
            .iter()
            .find(|o| o.id == old_id)
            .unwrap()
            .zone,
        Zone::Play
    );
    assert_eq!(snapshot.players[0].temporary_resources, 0);
    weapon_restore_and_finish(&mut sim);
    assert_eq!(sim.snapshot().players[0].temporary_resources, 7);
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == old_id)
            .unwrap()
            .zone,
        Zone::Graveyard
    );
}

#[test]
fn weapon_breakage_deathrattle_choice_restores_exactly() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Blade", 0, 3, 1).with_deathrattle(vec![weapon_pause()]),
    ]);
    weapon_play(&mut sim);
    weapon_attack(&mut sim);
    assert!(sim.pending_choice().is_some());
    assert_eq!(sim.snapshot().players[0].weapon, None);
    weapon_restore_and_finish(&mut sim);
}

#[test]
fn weapon_damage_preparation_reads_live_attack_after_reactions() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
    let weapon = weapon_play(&mut sim);
    let hero_id = hero(&mut sim, PlayerId::One);
    let mut trigger = self_event_trigger(EventKind::Attack, vec![weapon_pause()]);
    trigger.conditions.clear();
    let entity = game_entity(sim.app.world(), hero_id).unwrap();
    sim.app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    weapon_attack(&mut sim);
    attach_stat_modifier(
        sim.app.world_mut(),
        PlayerId::One,
        weapon,
        crate::StatModifier {
            attack: 3,
            health: 0,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    weapon_restore_and_finish(&mut sim);
    assert_eq!(sim.snapshot().players[1].health, 24);
}

#[test]
fn weapon_replaced_during_damage_does_not_charge_new_weapon() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
    let weapon = weapon_play(&mut sim);
    let blade = game_entity(sim.app.world(), weapon).unwrap();
    let mut trigger = self_event_trigger(
        EventKind::Damage,
        vec![weapon_replacement_effect(), weapon_pause()],
    );
    trigger.conditions.clear();
    sim.app
        .world_mut()
        .entity_mut(blade)
        .insert(RuntimeTriggers(vec![trigger]));
    weapon_attack(&mut sim);
    weapon_restore_and_finish(&mut sim);
    let snapshot = sim.snapshot();
    let active = snapshot.players[0].weapon.unwrap();
    assert_ne!(active, weapon);
    assert_eq!(
        snapshot
            .objects
            .iter()
            .find(|o| o.id == active)
            .unwrap()
            .durability,
        Some(3)
    );
    assert_eq!(snapshot.players[1].health, 27);
}

#[test]
fn weapon_nested_replacement_does_not_restore_superseded_weapon() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Old", 0, 1, 2),
        Card::weapon("Outer", 0, 3, 2)
            .with_effects(vec![weapon_replacement_effect(), weapon_pause()]),
    ]);
    let old = weapon_play(&mut sim);
    let outer = weapon_play(&mut sim);
    weapon_restore_and_finish(&mut sim);
    let snapshot = sim.snapshot();
    assert!(![old, outer].contains(&snapshot.players[0].weapon.unwrap()));
    for id in [old, outer] {
        assert_eq!(snapshot.deaths.iter().filter(|d| d.entity == id).count(), 1);
    }
}

#[test]
fn weapon_action_enumeration_is_pure_and_invalid_declarations_are_atomic() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 1, 3, 2)]);
    let id = hand_card(&mut sim, PlayerId::One);
    let before = sim.checkpoint().unwrap();
    assert!(
        sim.legal_actions()
            .contains(&play_declaration(id, None, None, None))
    );
    assert_eq!(before, sim.checkpoint().unwrap());
    assert_rejected_action_is_atomic(
        &mut sim,
        play_declaration(id, None, Some(0), None),
        SimulationError::UnexpectedBoardPosition(id),
    );
    assert_rejected_action_is_atomic(
        &mut sim,
        play_declaration(id, None, None, Some(ChoiceId(1))),
        SimulationError::UnsupportedActionChoice(ChoiceId(1)),
    );
    let entity = game_entity(sim.app.world(), id).unwrap();
    sim.app
        .world_mut()
        .get_mut::<crate::WeaponState>(entity)
        .unwrap()
        .durability = 0;
    assert_rejected_action_is_atomic(
        &mut sim,
        play_declaration(id, None, None, None),
        SimulationError::NotPlayable(id),
    );
}

#[test]
fn weapon_checkpoint_rejects_missing_state_stale_active_and_orphaned_scope() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
    let id = weapon_play(&mut sim);
    let checkpoint = sim.checkpoint().unwrap();
    let mut missing = checkpoint.clone();
    missing
        .entities
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap()
        .weapon_state = None;
    assert!(Simulation::from_checkpoint(missing).is_err());
    let mut stale = checkpoint.clone();
    stale
        .weapons
        .active
        .insert(PlayerId::One, GameEntityId(99999));
    assert!(Simulation::from_checkpoint(stale).is_err());
    let mut orphan = checkpoint;
    orphan.weapons.pending.insert(id, None);
    assert!(Simulation::from_checkpoint(orphan).is_err());
}

#[test]
fn weapon_prevented_damage_consumes_durability_and_removed_defender_aborts() {
    for removed in [false, true] {
        let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
        let weapon = weapon_play(&mut sim);
        let defender = hero(&mut sim, PlayerId::Two);
        let target = game_entity(sim.app.world(), defender).unwrap();
        if removed {
            let mut trigger = self_event_trigger(
                EventKind::Attack,
                vec![Effect::Move {
                    targets: Selector::Source,
                    player: PlayerSelector::Controller,
                    zone: Zone::SetAside,
                    kind: ZoneMovementKind::Normal,
                }],
            );
            trigger.conditions.clear();
            // Use a Minion defender so removing it preserves the mandatory Hero invariant.
            let minion = spawn_card(
                sim.app.world_mut(),
                PlayerId::Two,
                Card::minion("Retreat", 0, 1, 2).with_triggers(vec![trigger]),
                Zone::Play,
            )
            .unwrap();
            let attacker = hero(&mut sim, PlayerId::One);
            sim.apply(GameAction::Attack {
                player: PlayerId::One,
                attacker,
                defender: minion,
            })
            .unwrap();
        } else {
            sim.app
                .world_mut()
                .entity_mut(target)
                .insert(Keywords([Keyword::DivineShield].into()));
            weapon_attack(&mut sim);
        }
        assert_eq!(sim.snapshot().players[1].health, 30);
        assert_eq!(
            sim.snapshot()
                .objects
                .iter()
                .find(|o| o.id == weapon)
                .unwrap()
                .durability,
            Some(if removed { 2 } else { 1 })
        );
    }
}

#[test]
fn weapon_old_observer_and_deathrattle_precede_after_play() {
    let mut observer = self_event_trigger(
        EventKind::WeaponEquipped,
        vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 2,
            temporary: true,
        }],
    );
    observer.conditions.clear();
    let old = Card::weapon("Old", 0, 1, 2)
        .with_triggers(vec![observer])
        .with_deathrattle(vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 3,
            temporary: true,
        }]);
    let new = Card::weapon("New", 0, 2, 2);
    let mut sim = weapon_fixture(vec![old, new]);
    let old_id = weapon_play(&mut sim);
    let hero_id = hero(&mut sim, PlayerId::One);
    let entity = game_entity(sim.app.world(), hero_id).unwrap();
    let mut after_observer = self_event_trigger(
        EventKind::AfterPlay,
        vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 11,
            temporary: true,
        }],
    );
    after_observer.conditions.clear();
    sim.app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![after_observer]));
    let before = sim.snapshot().players[0].temporary_resources;
    let new_id = weapon_play(&mut sim);
    assert_eq!(sim.snapshot().players[0].temporary_resources - before, 16);
    let trace = &sim.checkpoint().unwrap().trace.entries;
    let death = trace
        .iter()
        .rposition(|entry| matches!(entry, TraceEntry::EntityDied { entity } if *entity == old_id))
        .unwrap();
    let after = trace.iter().rposition(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::AfterPlay, source: Some(id), .. } if *id == new_id)).unwrap();
    // Prepared AfterPlay is created early, but resolves only after death processing.
    assert!(after < death);
    let last_trigger = trace.iter().rposition(|entry| matches!(entry, TraceEntry::TriggerResolved { source, .. } if *source == old_id)).unwrap();
    assert!(last_trigger > death);
    let after_trigger = trace.iter().rposition(|entry| matches!(entry, TraceEntry::TriggerResolved { source, .. } if *source == hero_id)).unwrap();
    assert!(after_trigger > last_trigger);
}

#[test]
fn weapon_and_minion_combat_deaths_share_play_order() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 1)]);
    let blade = weapon_play(&mut sim);
    let target = spawn_card(
        sim.app.world_mut(),
        PlayerId::Two,
        Card::minion("Target", 0, 1, 2),
        Zone::Play,
    )
    .unwrap();
    let target_entity = game_entity(sim.app.world(), target).unwrap();
    let order = crate::entity::allocate_play_order(sim.app.world_mut());
    sim.app.world_mut().entity_mut(target_entity).insert(order);
    let attacker = hero(&mut sim, PlayerId::One);
    sim.apply(GameAction::Attack {
        player: PlayerId::One,
        attacker,
        defender: target,
    })
    .unwrap();
    let deaths = sim.snapshot().deaths;
    assert_eq!(
        deaths.iter().map(|d| d.entity).collect::<Vec<_>>(),
        vec![blade, target]
    );
    let trace = sim.checkpoint().unwrap().trace.entries;
    let after_attack = trace
        .iter()
        .position(|entry| {
            matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::AfterAttack,
                    ..
                }
            )
        })
        .unwrap();
    let first_death = trace
        .iter()
        .position(|entry| matches!(entry, TraceEntry::EntityDied { .. }))
        .unwrap();
    assert!(after_attack < first_death);
}

#[test]
fn weapon_changed_before_damage_preparation_supplies_attack_and_pays_durability() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Old", 0, 1, 2),
        Card::weapon("New", 0, 5, 3),
    ]);
    let old = weapon_play(&mut sim);
    let new = hand_card(&mut sim, PlayerId::One);
    let mut trigger = self_event_trigger(
        EventKind::Attack,
        vec![
            Effect::Move {
                targets: Selector::Entity(old),
                player: PlayerSelector::Controller,
                zone: Zone::Graveyard,
                kind: ZoneMovementKind::Normal,
            },
            Effect::Move {
                targets: Selector::Entity(new),
                player: PlayerSelector::Controller,
                zone: Zone::Play,
                kind: ZoneMovementKind::Normal,
            },
            weapon_pause(),
        ],
    );
    trigger.conditions.clear();
    let attacker = hero(&mut sim, PlayerId::One);
    let entity = game_entity(sim.app.world(), attacker).unwrap();
    sim.app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
    weapon_attack(&mut sim);
    weapon_restore_and_finish(&mut sim);
    let snapshot = sim.snapshot();
    assert_eq!(snapshot.players[1].health, 25);
    assert_eq!(
        snapshot
            .objects
            .iter()
            .find(|o| o.id == new)
            .unwrap()
            .durability,
        Some(2)
    );
    assert_eq!(
        snapshot
            .objects
            .iter()
            .find(|o| o.id == old)
            .unwrap()
            .durability,
        Some(2)
    );
}

#[test]
fn weapon_bounce_resets_durability_and_copy_uses_base_definition() {
    let mut sim = weapon_fixture(vec![Card::weapon("Blade", 0, 3, 2)]);
    let weapon = weapon_play(&mut sim);
    weapon_attack(&mut sim);
    assert_eq!(
        copy_card_data(sim.app.world(), weapon).unwrap().durability,
        2
    );
    crate::zone::move_entity(sim.app.world_mut(), weapon, Zone::Hand, None).unwrap();
    assert_eq!(sim.snapshot().players[0].weapon, None);
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == weapon)
            .unwrap()
            .durability,
        Some(2)
    );
    weapon_play(&mut sim);
    sim.assert_invariants().unwrap();
}

#[test]
fn weapon_failed_sequence_releases_replacement_scope_without_rollback() {
    let mut sim = weapon_fixture(vec![
        Card::weapon("Old", 0, 1, 2),
        Card::weapon("New", 0, 2, 2).with_effects(vec![weapon_pause()]),
    ]);
    let old = weapon_play(&mut sim);
    let new = weapon_play(&mut sim);
    sim.app
        .world_mut()
        .resource_mut::<ResolutionWork>()
        .remaining_budget = 0;
    assert!(sim.choose(ChoiceId(502)).is_err());
    assert_eq!(sim.snapshot().players[0].weapon, Some(new));
    assert_eq!(
        sim.snapshot()
            .objects
            .iter()
            .find(|o| o.id == old)
            .unwrap()
            .zone,
        Zone::Graveyard
    );
    assert!(sim.checkpoint().unwrap().weapons.pending.is_empty());
    sim.assert_invariants().unwrap();
    end_active_turn(&mut sim);
}

#[test]
fn weapon_targeted_play_uses_declared_target_without_spell_damage() {
    let filter = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Character,
    };
    let blade = Card::weapon("Battlecry blade", 0, 2, 2)
        .with_targeting(TargetRequirement::Required(filter))
        .with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(2),
        }]);
    let mut sim = weapon_fixture(vec![blade]);
    spawn_card(
        sim.app.world_mut(),
        PlayerId::One,
        Card::minion("Spell booster", 0, 0, 1).with_spell_damage(5),
        Zone::Play,
    )
    .unwrap();
    let card = hand_card(&mut sim, PlayerId::One);
    let target = hero(&mut sim, PlayerId::Two);
    let action = play_declaration(card, Some(target), None, None);
    assert!(sim.legal_actions().contains(&action));
    assert!(
        !sim.legal_actions()
            .contains(&play_declaration(card, None, None, None))
    );
    sim.apply(action).unwrap();
    assert_eq!(sim.snapshot().players[1].health, 28);
    assert_eq!(sim.snapshot().players[0].weapon, Some(card));
}

fn combat_reaction_fixture() -> (Simulation, GameEntityId, GameEntityId) {
    let mut simulation = simulation();
    let attacker = keyword_attacker(&mut simulation, Card::minion("Attacker", 0, 2, 10));
    let defender = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Defender", 0, 3, 10),
        Zone::Play,
    )
    .unwrap();
    (simulation, attacker, defender)
}

fn install_attack_reaction(
    simulation: &mut Simulation,
    attacker: GameEntityId,
    effects: Vec<Effect>,
) {
    let mut trigger = self_event_trigger(EventKind::Attack, effects);
    trigger.conditions[0].condition = crate::TriggerCondition::EventSourceIsSelf;
    let entity = game_entity(simulation.app.world(), attacker).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(RuntimeTriggers(vec![trigger]));
}

fn declare_combat(simulation: &mut Simulation, attacker: GameEntityId, defender: GameEntityId) {
    simulation
        .apply(GameAction::Attack {
            player: PlayerId::One,
            attacker,
            defender,
        })
        .unwrap();
}

fn combat_damage(simulation: &mut Simulation, id: GameEntityId) -> i32 {
    simulation
        .snapshot()
        .objects
        .iter()
        .find(|object| object.id == id)
        .unwrap()
        .damage
}

fn after_attack_count(simulation: &Simulation) -> usize {
    simulation
        .app
        .world()
        .resource::<CanonicalTrace>()
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::AfterAttack,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn combat_reactions_refresh_both_damage_values_including_zero_attack() {
    for attack_bonus in [4, -2] {
        let (mut simulation, attacker, defender) = combat_reaction_fixture();
        let effects = [(attacker, attack_bonus), (defender, 2)]
            .into_iter()
            .map(|(id, attack)| Effect::AttachStatModifier {
                targets: Selector::Entity(id),
                modifier: crate::StatModifier {
                    attack,
                    health: 0,
                    silence_removable: true,
                },
                duration: EnchantmentDuration::Permanent,
            })
            .collect();
        install_attack_reaction(&mut simulation, attacker, effects);
        declare_combat(&mut simulation, attacker, defender);
        assert_eq!(combat_damage(&mut simulation, defender), 2 + attack_bonus);
        assert_eq!(combat_damage(&mut simulation, attacker), 5);
        assert_eq!(
            windfury_attack_state(&simulation, attacker).attacks_this_turn,
            1
        );
        assert_eq!(after_attack_count(&simulation), 1);
        simulation.assert_invariants().unwrap();
    }
}

#[test]
fn combat_reactions_cancel_after_removal_destroy_or_lethal_damage() {
    for remove_attacker in [false, true] {
        for method in 0..3 {
            let (mut simulation, attacker, defender) = combat_reaction_fixture();
            let target = if remove_attacker { attacker } else { defender };
            let effect = match method {
                0 => Effect::Move {
                    targets: Selector::Entity(target),
                    player: PlayerSelector::Player(if remove_attacker {
                        PlayerId::One
                    } else {
                        PlayerId::Two
                    }),
                    zone: Zone::Hand,
                    kind: ZoneMovementKind::Normal,
                },
                1 => Effect::Destroy {
                    targets: Selector::Entity(target),
                },
                _ => Effect::DealDamage {
                    targets: Selector::Entity(target),
                    amount: ValueExpression::Constant(10),
                },
            };
            install_attack_reaction(&mut simulation, attacker, vec![effect]);
            declare_combat(&mut simulation, attacker, defender);
            let survivor = if remove_attacker { defender } else { attacker };
            assert_eq!(combat_damage(&mut simulation, survivor), 0);
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                0
            );
            assert_eq!(after_attack_count(&simulation), 0);
            assert!(simulation.app.world().resource::<CanonicalTrace>().entries.iter().any(|entry| matches!(entry,
                TraceEntry::SequenceStepSkipped { step: SequenceStep::PrepareCombatDamage { .. }, subject, .. } if *subject == target
            )));
            simulation.assert_invariants().unwrap();
        }
    }
}

#[test]
fn combat_reactions_continue_with_transformed_or_newly_controlled_participants() {
    for change_attacker in [false, true] {
        for transform in [false, true] {
            let (mut simulation, attacker, defender) = combat_reaction_fixture();
            let target = if change_attacker { attacker } else { defender };
            let effect = if transform {
                Effect::Transform {
                    targets: Selector::Entity(target),
                    card: Card::minion("New form", 0, 4, 10),
                    kind: TransformKind::NonSpell,
                }
            } else {
                Effect::Move {
                    targets: Selector::Entity(target),
                    player: PlayerSelector::Player(if change_attacker {
                        PlayerId::Two
                    } else {
                        PlayerId::One
                    }),
                    zone: Zone::Play,
                    kind: ZoneMovementKind::Normal,
                }
            };
            install_attack_reaction(&mut simulation, attacker, vec![effect]);
            declare_combat(&mut simulation, attacker, defender);
            assert_eq!(
                combat_damage(&mut simulation, defender),
                if transform && change_attacker { 4 } else { 2 }
            );
            assert_eq!(
                combat_damage(&mut simulation, attacker),
                if transform && !change_attacker { 4 } else { 3 }
            );
            assert_eq!(
                windfury_attack_state(&simulation, attacker).attacks_this_turn,
                1
            );
            assert_eq!(after_attack_count(&simulation), 1);
            simulation.assert_invariants().unwrap();
        }
    }
}

#[test]
fn combat_preparation_hero_defeat_prevents_damage_and_after_attack() {
    let (mut simulation, attacker, _) = combat_reaction_fixture();
    let friendly_hero = hero(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    install_attack_reaction(
        &mut simulation,
        attacker,
        vec![Effect::DealDamage {
            targets: Selector::Entity(friendly_hero),
            amount: ValueExpression::Constant(30),
        }],
    );
    declare_combat(&mut simulation, attacker, enemy_hero);
    assert_eq!(
        simulation.snapshot().game.outcome,
        Some(crate::GameOutcome::Winner(PlayerId::Two))
    );
    assert_eq!(combat_damage(&mut simulation, enemy_hero), 0);
    assert_eq!(
        windfury_attack_state(&simulation, attacker).attacks_this_turn,
        0
    );
    assert_eq!(after_attack_count(&simulation), 0);
    simulation.assert_invariants().unwrap();
}

#[test]
fn combat_reaction_choices_restore_live_damage_and_cancellation_exactly() {
    for cancel in [false, true] {
        let (mut simulation, attacker, defender) = combat_reaction_fixture();
        let effect = if cancel {
            Effect::Destroy {
                targets: Selector::Entity(defender),
            }
        } else {
            Effect::AttachStatModifier {
                targets: Selector::Entity(attacker),
                modifier: crate::StatModifier {
                    attack: 4,
                    health: 0,
                    silence_removable: true,
                },
                duration: EnchantmentDuration::Permanent,
            }
        };
        install_attack_reaction(
            &mut simulation,
            attacker,
            vec![Effect::Choose {
                id: ChoiceId(500),
                player: PlayerSelector::Controller,
                options: vec![hearthstone_simulator_core::EffectChoiceOption {
                    id: ChoiceId(501),
                    effects: vec![effect],
                }],
            }],
        );
        declare_combat(&mut simulation, attacker, defender);
        let checkpoint = simulation.checkpoint().unwrap();
        assert_eq!(checkpoint.schema_version, 16);
        assert_eq!(combat_damage(&mut simulation, defender), 0);
        for missing_attacker in [false, true] {
            let mut invalid = checkpoint.clone();
            for operation in &mut invalid.resolution.stack {
                if let ResolutionOp::RunGuardedSequenceStep {
                    step:
                        SequenceStep::PrepareCombatDamage {
                            attacker, defender, ..
                        },
                    ..
                } = &mut operation.operation
                {
                    if missing_attacker {
                        *attacker = GameEntityId(u64::MAX);
                    } else {
                        *defender = GameEntityId(u64::MAX);
                    }
                }
            }
            assert!(simulation.restore(invalid).is_err());
        }
        let mut old = checkpoint.clone();
        old.schema_version = 14;
        assert!(simulation.restore(old).is_err());
        let mut fork = simulation.fork().unwrap();
        let mut restored = simulation.fork().unwrap();
        restored
            .restore(SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap())
            .unwrap();
        for candidate in [&mut simulation, &mut fork, &mut restored] {
            candidate.choose(ChoiceId(501)).unwrap();
            assert_eq!(
                windfury_attack_state(candidate, attacker).attacks_this_turn,
                u8::from(!cancel)
            );
            assert_eq!(after_attack_count(candidate), usize::from(!cancel));
            if !cancel {
                assert_eq!(combat_damage(candidate, defender), 6);
            }
            candidate.assert_invariants().unwrap();
        }
        assert_eq!(simulation.checkpoint().unwrap(), fork.checkpoint().unwrap());
        assert_eq!(
            simulation.checkpoint().unwrap(),
            restored.checkpoint().unwrap()
        );
    }
}

#[test]
fn combat_preparation_expires_attack_auras_from_dead_providers() {
    let (mut simulation, attacker, defender) = combat_reaction_fixture();
    let provider = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Attack provider", 0, 0, 1).with_aura(crate::AuraDefinition {
            targets: crate::AuraTarget::OtherFriendlyMinions,
            attack: 5,
            health: 0,
            other: vec![],
        }),
        Zone::Play,
    )
    .unwrap();
    crate::aura::refresh_all_auras(simulation.app.world_mut());
    install_attack_reaction(
        &mut simulation,
        attacker,
        vec![Effect::Destroy {
            targets: Selector::Entity(provider),
        }],
    );
    declare_combat(&mut simulation, attacker, defender);
    assert_eq!(combat_damage(&mut simulation, defender), 2);
    assert_eq!(combat_damage(&mut simulation, attacker), 3);
    assert_eq!(after_attack_count(&simulation), 1);
    simulation.assert_invariants().unwrap();
}

#[test]
fn combat_preparation_waits_for_suspended_chained_deaths_before_cancelling() {
    let (mut simulation, attacker, defender) = combat_reaction_fixture();
    let second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Second death", 0, 0, 1).with_deathrattle(vec![Effect::Choose {
            id: ChoiceId(600),
            player: PlayerSelector::Controller,
            options: vec![hearthstone_simulator_core::EffectChoiceOption {
                id: ChoiceId(601),
                effects: vec![Effect::Destroy {
                    targets: Selector::Entity(defender),
                }],
            }],
        }]),
        Zone::Play,
    )
    .unwrap();
    let first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("First death", 0, 0, 1).with_deathrattle(vec![Effect::Destroy {
            targets: Selector::Entity(second),
        }]),
        Zone::Play,
    )
    .unwrap();
    install_attack_reaction(
        &mut simulation,
        attacker,
        vec![Effect::Destroy {
            targets: Selector::Entity(first),
        }],
    );
    declare_combat(&mut simulation, attacker, defender);
    assert_eq!(
        simulation.snapshot().game.status,
        SimulationStatus::AwaitingChoice
    );
    assert_eq!(combat_damage(&mut simulation, defender), 0);
    let checkpoint = simulation.checkpoint().unwrap();
    let mut fork = simulation.fork().unwrap();
    let mut restored = Simulation::from_checkpoint(
        SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap(),
    )
    .unwrap();
    for candidate in [&mut simulation, &mut fork, &mut restored] {
        candidate.choose(ChoiceId(601)).unwrap();
        assert_eq!(combat_damage(candidate, attacker), 0);
        assert_eq!(
            windfury_attack_state(candidate, attacker).attacks_this_turn,
            0
        );
        assert_eq!(after_attack_count(candidate), 0);
        assert_eq!(candidate.snapshot().deaths.len(), 3);
        candidate.assert_invariants().unwrap();
    }
    assert_eq!(simulation.checkpoint().unwrap(), fork.checkpoint().unwrap());
    assert_eq!(
        simulation.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
}
