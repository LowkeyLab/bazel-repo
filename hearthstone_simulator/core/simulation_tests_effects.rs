use googletest::prelude::*;

use super::{test_support::*, *};

#[derive(Resource)]
struct NativeHandlerObservation(EffectContext);

fn synthetic_native_handler(In(context): In<EffectContext>, mut commands: Commands) -> Vec<Effect> {
    commands.insert_resource(NativeHandlerObservation(context.clone()));
    vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(2),
    }]
}

fn synthetic_native_modifier_handler(In(_): In<EffectContext>) -> Vec<Effect> {
    vec![Effect::ModifyEventValue {
        operation: EventValueOperation::Replace,
        value: ValueExpression::Constant(0),
    }]
}

#[googletest::test]
fn effect_dispatch_covers_selectors_values_and_stateful_primitives() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::spell("Friendly Draw", 0)]),
        PlayerConfig::with_deck("Rexxar", vec![Card::spell("Enemy Draw", 0)]),
    ]);
    let world = simulation.app.world_mut();
    let friendly = spawn_card(
        world,
        PlayerId::One,
        Card::minion("Friendly", 0, 2, 3).with_keywords([Keyword::Taunt]),
        Zone::Play,
    )
    .unwrap();
    let enemy = spawn_card(
        world,
        PlayerId::Two,
        Card::minion("Enemy", 0, 1, 4),
        Zone::Play,
    )
    .unwrap();
    let context = EffectContext {
        source: Some(friendly),
        controller: PlayerId::One,
        declared_target: Some(enemy),
    };

    assert_that!(
        select_entities(world, &context, &Selector::Source),
        eq(&vec![friendly])
    );
    assert_that!(
        select_entities(world, &context, &Selector::DeclaredTarget),
        eq(&vec![enemy])
    );
    assert_that!(
        select_entities(world, &context, &Selector::Entity(enemy)),
        eq(&vec![enemy])
    );
    assert_that!(
        select_entities(
            world,
            &context,
            &Selector::InZone {
                player: PlayerSelector::Opponent,
                zone: Zone::Deck,
            }
        )
        .len(),
        eq(1)
    );
    assert_that!(
        select_entities(world, &context, &Selector::FriendlyMinions),
        eq(&vec![friendly])
    );
    assert_that!(
        select_entities(world, &context, &Selector::EnemyMinions),
        eq(&vec![enemy])
    );
    assert_that!(
        select_entities(world, &context, &Selector::AllMinions),
        eq(&vec![friendly, enemy])
    );
    assert_that!(
        select_entities(world, &context, &Selector::FriendlyCharacters).len(),
        eq(2)
    );
    assert_that!(
        select_entities(world, &context, &Selector::EnemyCharacters).len(),
        eq(2)
    );
    assert_that!(
        select_entities(world, &context, &Selector::AllCharacters).len(),
        eq(4)
    );
    assert_that!(
        select_entities(
            world,
            &context,
            &Selector::Random(Box::new(Selector::Entity(enemy)))
        ),
        eq(&vec![enemy])
    );
    assert_that!(
        evaluate_value(world, &context, ValueExpression::SourceAttack, 9),
        eq(2)
    );
    assert_that!(
        evaluate_value(world, &context, ValueExpression::TargetCount, 9),
        eq(9)
    );
    assert_that!(
        resolve_player(PlayerId::One, PlayerSelector::Controller),
        eq(PlayerId::One)
    );
    assert_that!(
        resolve_player(PlayerId::One, PlayerSelector::Opponent),
        eq(PlayerId::Two)
    );
    assert_that!(
        resolve_player(PlayerId::One, PlayerSelector::Player(PlayerId::Two)),
        eq(PlayerId::Two)
    );

    begin_resolution(world, ResolutionKind::Sequence);
    execute_effects(
        world,
        &context,
        &[Effect::Sequence(vec![
            Effect::DealDamage {
                targets: Selector::DeclaredTarget,
                amount: ValueExpression::SourceAttack,
            },
            Effect::Heal {
                targets: Selector::DeclaredTarget,
                amount: ValueExpression::TargetCount,
            },
            Effect::Destroy {
                targets: Selector::DeclaredTarget,
            },
            Effect::Draw {
                player: PlayerSelector::Opponent,
                count: 1,
            },
            Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 2,
                temporary: true,
            },
            Effect::GainResource {
                player: PlayerSelector::Player(PlayerId::Two),
                amount: 2,
                temporary: false,
            },
            Effect::Summon {
                player: PlayerSelector::Opponent,
                card: Card::minion("Summoned", 0, 1, 1),
                board_index: Some(1),
            },
            Effect::AttachStatModifier {
                targets: Selector::Source,
                modifier: StatModifier {
                    attack: 3,
                    health: 2,
                    silence_removable: true,
                },
            },
            Effect::Silence {
                targets: Selector::Source,
            },
            Effect::Transform {
                targets: Selector::DeclaredTarget,
                card: Card::minion("Sheep", 1, 1, 1).with_keywords([Keyword::Rush]),
            },
            Effect::Copy {
                targets: Selector::DeclaredTarget,
                player: PlayerSelector::Controller,
                zone: Zone::Hand,
            },
        ])],
    )
    .unwrap();
    complete_active(world).unwrap();
    cleanup_resolution(world);

    assert_that!(
        world.get::<Damage>(game_entity(world, enemy).unwrap()),
        eq(Some(&Damage(0)))
    );
    assert_that!(
        has_keyword(world, game_entity(world, friendly).unwrap(), Keyword::Taunt,),
        is_false()
    );
    let copied = world
        .resource::<ZoneIndex>()
        .entities(PlayerId::One, Zone::Hand)[0];
    assert_that!(
        has_keyword(world, game_entity(world, enemy).unwrap(), Keyword::Rush),
        is_true()
    );
    assert_that!(
        has_keyword(world, game_entity(world, copied).unwrap(), Keyword::Rush),
        is_true()
    );
    assert_that!(
        player(world, PlayerId::One).unwrap().1.temporary_resources,
        eq(2)
    );
    assert_that!(
        player(world, PlayerId::Two).unwrap().1.maximum_resources,
        eq(2)
    );

    execute_effect(
        world,
        &context,
        &Effect::Summon {
            player: PlayerSelector::Opponent,
            card: Card::minion("Appended", 0, 1, 1),
            board_index: None,
        },
    )
    .unwrap();
    let board_before_invalid_position = world
        .resource::<ZoneIndex>()
        .entities(PlayerId::Two, Zone::Play)
        .to_vec();
    let entities_before_invalid_position = world.resource::<GameEntityIndex>().0.len();
    assert_that!(
        matches!(
            execute_effect(
                world,
                &context,
                &Effect::Summon {
                    player: PlayerSelector::Opponent,
                    card: Card::minion("Bad Position", 0, 1, 1),
                    board_index: Some(999),
                },
            ),
            Err(SimulationError::Zone(ZoneError::InvalidPosition { .. }))
        ),
        is_true()
    );
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::Two, Zone::Play),
        eq(board_before_invalid_position.as_slice())
    );
    assert_that!(
        world.resource::<GameEntityIndex>().0.len(),
        eq(entities_before_invalid_position)
    );

    let minion_count = board_before_invalid_position
        .iter()
        .filter(|id| {
            game_entity(world, **id).and_then(|entity| world.get::<EntityKind>(entity))
                == Some(&EntityKind::Minion)
        })
        .count();
    world.resource_mut::<Ruleset>().board_limit = minion_count;
    execute_effect(
        world,
        &context,
        &Effect::Summon {
            player: PlayerSelector::Opponent,
            card: Card::minion("No Room", 0, 1, 1),
            board_index: None,
        },
    )
    .unwrap();
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::Two, Zone::Play),
        eq(board_before_invalid_position.as_slice())
    );
}

#[googletest::test]
fn native_handlers_flush_commands_and_return_nested_effect_plans() {
    let native_id = NativeEffectId::new("synthetic:native_damage");
    let spell = Card::spell("Native Bolt", 0).with_effects(vec![Effect::Native(native_id.clone())]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![spell]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    simulation
        .register_native_effect(native_id.clone(), synthetic_native_handler)
        .unwrap();
    assert_that!(
        simulation.register_native_effect(native_id.clone(), synthetic_native_handler),
        err(eq(&SimulationError::NativeEffectAlreadyRegistered(
            native_id.clone()
        )))
    );
    let card = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[1].health, eq(28));
    let mut fork = simulation.fork().unwrap();
    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
    assert_that!(simulation.trace(), eq(fork.trace()));
    assert_that!(
        simulation
            .app
            .world()
            .resource::<NativeHandlerObservation>()
            .0
            .declared_target,
        eq(Some(target))
    );

    let missing = NativeEffectId::new("synthetic:missing");
    let world = simulation.app.world_mut();
    begin_resolution(world, ResolutionKind::Sequence);
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
    };
    assert_that!(
        execute_effect(world, &context, &Effect::Native(missing.clone())),
        err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
    );
    complete_active(world).unwrap();
    cleanup_resolution(world);
}

#[googletest::test]
fn native_returned_event_modifiers_are_validated_before_execution() {
    let native_id = NativeEffectId::new("synthetic:native_modifier");
    let mut simulation = simulation();
    simulation
        .register_native_effect(native_id.clone(), synthetic_native_modifier_handler)
        .unwrap();
    let world = simulation.app.world_mut();
    begin_resolution(world, ResolutionKind::Sequence);
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
    };

    assert_that!(
        execute_effect(world, &context, &Effect::Native(native_id.clone())),
        err(eq(&SimulationError::NoModifiableEventValue))
    );
    assert_that!(
        world
            .resource::<CanonicalTrace>()
            .entries
            .iter()
            .any(|entry| matches!(entry, TraceEntry::EventValueChanged { .. })),
        is_false()
    );
    complete_active(world).unwrap();
    cleanup_resolution(world);

    let root = begin_resolution(world, ResolutionKind::Sequence);
    let event = spawn_resolution_child(world, root, ResolutionKind::Event);
    world.entity_mut(event).insert(EventContext {
        kind: EventKind::ProposedDamage,
        source: None,
        targets: Vec::new(),
        controller: PlayerId::One,
        proposed_value: Some(5),
        actual_value: None,
        simultaneous_ordinal: 0,
    });
    activate_resolution_child(world, event).unwrap();
    execute_effect(world, &context, &Effect::Native(native_id)).unwrap();
    assert_that!(
        world.get::<EventContext>(event).unwrap().proposed_value,
        eq(Some(0))
    );
    complete_active(world).unwrap();
    complete_active(world).unwrap();
    cleanup_resolution(world);
}

#[googletest::test]
fn event_value_modifiers_do_not_cross_nested_event_boundaries() {
    let mut simulation = simulation();
    let target = hero(&mut simulation, PlayerId::Two);
    let world = simulation.app.world_mut();
    let root = begin_resolution(world, ResolutionKind::Sequence);
    let outer = spawn_resolution_child(world, root, ResolutionKind::Event);
    world.entity_mut(outer).insert(EventContext {
        kind: EventKind::ProposedDamage,
        source: None,
        targets: vec![target],
        controller: PlayerId::One,
        proposed_value: Some(5),
        actual_value: None,
        simultaneous_ordinal: 0,
    });
    activate_resolution_child(world, outer).unwrap();
    let inner = spawn_resolution_child(world, outer, ResolutionKind::Event);
    world.entity_mut(inner).insert(EventContext {
        kind: EventKind::Damage,
        source: None,
        targets: vec![target],
        controller: PlayerId::One,
        proposed_value: Some(1),
        actual_value: Some(1),
        simultaneous_ordinal: 0,
    });
    activate_resolution_child(world, inner).unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: Some(target),
    };

    assert_that!(
        modify_active_event_value(
            world,
            &context,
            EventValueOperation::Replace,
            ValueExpression::Constant(0),
        ),
        err(eq(&SimulationError::NoModifiableEventValue))
    );
    assert_that!(
        world.get::<EventContext>(outer).unwrap().proposed_value,
        eq(Some(5))
    );
    complete_active(world).unwrap();
    complete_active(world).unwrap();
    complete_active(world).unwrap();
    cleanup_resolution(world);
}

#[googletest::test]
fn missing_native_effects_are_rejected_before_card_play_mutates_state() {
    let missing = NativeEffectId::new("synthetic:missing");
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::spell("Missing Native", 1)
                    .with_effects(vec![Effect::Native(missing.clone())]),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let before = simulation.snapshot();

    assert_that!(
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
    );

    assert_that!(simulation.snapshot(), eq(&before));
    let mut fork = simulation.fork().unwrap();
    assert_that!(simulation.snapshot(), eq(&fork.snapshot()));
}

#[googletest::test]
fn missing_native_deathrattles_are_rejected_before_card_play_mutates_state() {
    let missing = NativeEffectId::new("synthetic:missing_deathrattle");
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Missing Native Deathrattle", 1, 1, 1)
                    .with_deathrattle(vec![Effect::Native(missing.clone())]),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    let before = simulation.snapshot();

    assert_that!(
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
    );

    assert_that!(simulation.snapshot(), eq(&before));
    assert_that!(
        simulation.snapshot().game.status,
        eq(SimulationStatus::AwaitingAction)
    );
    simulation
        .assert_invariants()
        .expect("rejected Deathrattle should preserve invariants");
}

#[googletest::test]
fn silence_suppresses_future_triggers_but_preserves_frozen_entries() {
    let suppressor =
        Card::minion("Suppressor", 0, 1, 3).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: vec![Effect::Silence {
                targets: Selector::DeclaredTarget,
            }],
        }]);
    let reactive =
        Card::minion("Reactive", 0, 1, 4).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        }]);
    let bolt = || {
        Card::spell("Bolt", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        }])
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![suppressor, reactive, bolt(), bolt()]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let suppressor = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: suppressor,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let reactive = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: reactive,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    for _ in 0..2 {
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
    }

    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(1)
    );
    let reactive_entity = game_entity(simulation.app.world(), reactive).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<RuntimeTriggers>(reactive_entity)
            .is_some(),
        is_true()
    );
    assert_that!(
        simulation
            .app
            .world()
            .get::<TriggersSuppressed>(reactive_entity)
            .is_some(),
        is_true()
    );

    transform_entity(
        simulation.app.world_mut(),
        reactive,
        Card::minion("Transformed", 0, 2, 2).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Healing,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustExist,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: Vec::new(),
        }]),
    )
    .unwrap();
    let transformed = game_entity(simulation.app.world(), reactive).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<TriggersSuppressed>(transformed)
            .is_none(),
        is_true()
    );
    assert_that!(
        simulation
            .app
            .world()
            .get::<RuntimeTriggers>(transformed)
            .unwrap()
            .0[0]
            .event,
        eq(EventKind::Healing)
    );
}
