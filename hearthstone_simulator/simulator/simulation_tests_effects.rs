use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AttachedTo, ConditionTiming, ContinuousEffectDefinition, ContinuousModifier, Controller,
    DefinitionId, EnchantmentDuration, PlayerAudience, SilenceRemovable, SourceEligibilityPolicy,
    TimedCondition, TriggerCondition, TriggerDefinition, WoundedTargetPolicy, ZoneMovementKind,
};

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

fn turn_end_trigger(effect_program: Vec<Effect>) -> TriggerDefinition {
    TriggerDefinition {
        event: EventKind::TurnEnded,
        eligible_zones: vec![Zone::Play],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventControllerIs(PlayerSelector::Controller),
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program,
    }
}

#[googletest::test]
fn trigger_enchantment_records_attachment_context() {
    let mut simulation = simulation();
    let world = simulation.app.world_mut();
    let target = spawn_card(
        world,
        PlayerId::One,
        Card::minion("Target", 0, 1, 3),
        Zone::Play,
    )
    .unwrap();
    let trigger = turn_end_trigger(vec![Effect::DealDamage {
        targets: Selector::AttachedEntity,
        amount: ValueExpression::Constant(1),
    }]);
    let context = EffectContext {
        source: None,
        controller: PlayerId::Two,
        declared_target: None,
        origin: EffectOrigin::Other,
    };

    execute_effect(
        world,
        &context,
        &Effect::AttachTriggerEnchantment {
            targets: Selector::Entity(target),
            triggers: vec![trigger.clone()],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        },
    )
    .unwrap();

    let target_entity = game_entity(world, target).unwrap();
    let enchantments = world
        .iter_entities()
        .filter(|entity| {
            entity.get::<EntityKind>() == Some(&EntityKind::Enchantment)
                && entity.get::<RuntimeTriggers>().is_some()
        })
        .collect::<Vec<_>>();
    assert_that!(enchantments.len(), eq(1));
    let enchantment = enchantments[0];
    assert_that!(enchantment.get::<Zone>(), eq(Some(&Zone::Play)));
    assert_that!(
        enchantment.get::<AttachedTo>(),
        eq(Some(&AttachedTo(target_entity)))
    );
    assert_that!(
        enchantment.get::<RuntimeTriggers>(),
        eq(Some(&RuntimeTriggers(vec![trigger])))
    );
    assert_that!(
        enchantment.get::<Controller>(),
        eq(Some(&Controller(PlayerId::Two)))
    );
    assert_that!(
        enchantment.get::<EnchantmentDuration>(),
        eq(Some(&EnchantmentDuration::Permanent))
    );
    assert_that!(
        enchantment.get::<DefinitionId>(),
        eq(Some(&DefinitionId(
            "synthetic:trigger_enchantment".to_string()
        )))
    );
    assert_that!(enchantment.contains::<SilenceRemovable>(), is_true());
}

#[googletest::test]
fn action_validation_rejects_invalid_trigger_enchantments_without_creating_them() {
    let missing = NativeEffectId::new("synthetic:missing_trigger_effect");
    let valid = turn_end_trigger(Vec::new());
    let mut death = valid.clone();
    death.event = EventKind::Death;
    let mut wrong_zones = valid.clone();
    wrong_zones.eligible_zones = vec![Zone::Play, Zone::Hand];
    let mut remembered = valid.clone();
    remembered.source_eligibility = SourceEligibilityPolicy::RememberedSource;
    let mut invalid_native = valid.clone();
    invalid_native.effect_program = vec![Effect::Native(missing.clone())];
    let cases = [
        (
            "empty triggers",
            Vec::new(),
            SimulationError::InvalidTriggerEnchantment(
                "at least one trigger is required".to_string(),
            ),
        ),
        (
            "Death trigger",
            vec![death],
            SimulationError::InvalidTriggerEnchantment(
                "added Deathrattles are not supported".to_string(),
            ),
        ),
        (
            "wrong eligible zones",
            vec![wrong_zones],
            SimulationError::InvalidTriggerEnchantment(
                "eligible zones must be exactly [Play]".to_string(),
            ),
        ),
        (
            "remembered source",
            vec![remembered],
            SimulationError::InvalidTriggerEnchantment("source must remain in Play".to_string()),
        ),
        (
            "invalid nested native effect",
            vec![invalid_native],
            SimulationError::NativeEffectNotRegistered(missing),
        ),
    ];

    for (name, triggers, expected) in cases {
        let card = Card::spell(name, 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::FriendlyMinions,
            triggers,
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![card]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let card = hand_card(&mut simulation, PlayerId::One);

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(eq(&expected))
        );
        assert_that!(
            simulation
                .app
                .world()
                .iter_entities()
                .filter(|entity| {
                    entity.get::<EntityKind>() == Some(&EntityKind::Enchantment)
                        && entity.get::<RuntimeTriggers>().is_some()
                })
                .count(),
            eq(0)
        );
    }
}

#[googletest::test]
fn effect_dispatch_reports_stale_targets_and_native_systems() {
    let mut simulation = simulation();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    assert_that!(
        execute_effect(
            simulation.app.world_mut(),
            &context,
            &Effect::Move {
                targets: Selector::Entity(GameEntityId(u64::MAX)),
                player: PlayerSelector::Controller,
                zone: Zone::Hand,
                kind: ZoneMovementKind::Normal,
            },
        ),
        err(matches_pattern!(SimulationError::Zone(_)))
    );
    assert_that!(
        execute_effect(
            simulation.app.world_mut(),
            &context,
            &Effect::AttachContinuousEffect {
                targets: Selector::Entity(GameEntityId(u64::MAX)),
                effect: ContinuousEffectDefinition {
                    recipients: PlayerAudience::Controller,
                    modifier: ContinuousModifier::SpellDamage(1),
                },
                silence_removable: true,
                duration: EnchantmentDuration::Permanent,
            },
        ),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(u64::MAX))))
    );

    let native_id = NativeEffectId::new("synthetic:stale_system");
    simulation
        .register_native_effect(native_id.clone(), synthetic_native_handler)
        .unwrap();
    let system = simulation
        .app
        .world()
        .resource::<NativeEffectRegistry>()
        .0
        .get(&native_id)
        .copied()
        .unwrap();
    simulation
        .app
        .world_mut()
        .unregister_system(system)
        .unwrap();
    assert_that!(
        execute_effect(
            simulation.app.world_mut(),
            &context,
            &Effect::Native(native_id),
        ),
        err(matches_pattern!(SimulationError::NativeEffectFailed { .. }))
    );
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
        Card::minion("Friendly", 0, 2, 3),
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
        origin: EffectOrigin::Other,
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

    begin_sequence(world).unwrap();
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
                duration: EnchantmentDuration::Permanent,
            },
            Effect::Silence {
                targets: Selector::Source,
            },
            Effect::Transform {
                targets: Selector::DeclaredTarget,
                card: Card::minion("Sheep", 1, 1, 1),
            },
            Effect::Copy {
                targets: Selector::DeclaredTarget,
                player: PlayerSelector::Controller,
                zone: Zone::Hand,
            },
        ])],
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    assert_that!(
        world.get::<Damage>(game_entity(world, enemy).unwrap()),
        eq(Some(&Damage(0)))
    );
    assert_that!(
        world
            .get::<Keywords>(game_entity(world, friendly).unwrap())
            .unwrap()
            .0
            .contains(&Keyword::Taunt),
        is_false()
    );
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Hand)
            .len(),
        eq(1)
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
fn multi_draw_expands_into_ordered_single_draw_operations() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck(
            "Jaina",
            vec![Card::spell("First", 0), Card::spell("Second", 0)],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    execute_effect(
        world,
        &context,
        &Effect::Draw {
            player: PlayerSelector::Controller,
            count: 0,
        },
    )
    .unwrap();
    execute_effect(
        world,
        &context,
        &Effect::Draw {
            player: PlayerSelector::Controller,
            count: 2,
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Hand)
            .len(),
        eq(2)
    );
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Deck)
            .is_empty(),
        is_true()
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
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    assert_that!(
        execute_effect(world, &context, &Effect::Native(missing.clone())),
        err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
    );
}

#[googletest::test]
fn native_returned_event_modifiers_are_validated_before_execution() {
    let native_id = NativeEffectId::new("synthetic:native_modifier");
    let mut simulation = simulation();
    simulation
        .register_native_effect(native_id.clone(), synthetic_native_modifier_handler)
        .unwrap();
    let world = simulation.app.world_mut();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
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

    begin_sequence(world).unwrap();
    let event = prepare_event(
        world,
        EventContext {
            kind: EventKind::ProposedDamage,
            source: None,
            targets: Vec::new(),
            controller: PlayerId::One,
            proposed_value: Some(5),
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    );
    execute_effect_operation(world, &context, &Effect::Native(native_id), Some(event)).unwrap();
    drive_resolution(world).unwrap();
    assert_that!(
        world
            .resource::<ResolutionWork>()
            .events
            .get(&event)
            .unwrap()
            .context
            .proposed_value,
        eq(Some(0))
    );
    take_prepared_event(world, event).unwrap();
    finish_sequence(world);
}

#[googletest::test]
fn action_resolution_errors_abandon_work_and_restore_input_state() {
    let native_id = NativeEffectId::new("synthetic:invalid_action_modifier");
    let card = Card::spell("Invalid Native Modifier", 0)
        .with_effects(vec![Effect::Native(native_id.clone())]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![card]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    simulation
        .register_native_effect(native_id, synthetic_native_modifier_handler)
        .unwrap();
    let card = hand_card(&mut simulation, PlayerId::One);

    assert_that!(
        simulation.apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        }),
        err(eq(&SimulationError::NoModifiableEventValue))
    );
    assert_that!(simulation.resolution_work().sequence_active, is_false());
    assert_that!(simulation.resolution_work().stack.is_empty(), is_true());
    assert_that!(
        simulation.app.world().resource::<GameState>().status,
        eq(SimulationStatus::AwaitingAction)
    );
}

#[googletest::test]
fn event_value_modifiers_do_not_cross_nested_event_boundaries() {
    let mut simulation = simulation();
    let target = hero(&mut simulation, PlayerId::Two);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    let outer = prepare_event(
        world,
        EventContext {
            kind: EventKind::ProposedDamage,
            source: None,
            targets: vec![target],
            controller: PlayerId::One,
            proposed_value: Some(5),
            actual_value: None,
            simultaneous_ordinal: 0,
        },
    );
    let inner = prepare_event(
        world,
        EventContext {
            kind: EventKind::Damage,
            source: None,
            targets: vec![target],
            controller: PlayerId::One,
            proposed_value: Some(1),
            actual_value: Some(1),
            simultaneous_ordinal: 0,
        },
    );
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: Some(target),
        origin: EffectOrigin::Other,
    };

    assert_that!(
        modify_event_value(
            world,
            Some(inner),
            &context,
            EventValueOperation::Replace,
            ValueExpression::Constant(0),
        ),
        err(eq(&SimulationError::NoModifiableEventValue))
    );
    assert_that!(
        world
            .resource::<ResolutionWork>()
            .events
            .get(&outer)
            .unwrap()
            .context
            .proposed_value,
        eq(Some(5))
    );
    take_prepared_event(world, inner).unwrap();
    take_prepared_event(world, outer).unwrap();
    finish_sequence(world);
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
            .get::<Silenced>(reactive_entity)
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
            .get::<Silenced>(transformed)
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

#[googletest::test]
fn silence_removes_a_temporary_cost_modifier_completely() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Cost target", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let target = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: target,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::AttachCostModifier {
            targets: Selector::Entity(target),
            modifier: CostModifier {
                operation: CostOperation::Set,
                value: 5,
                silence_removable: true,
            },
            duration: EnchantmentDuration::EndOfTurn(PlayerId::One),
        },
    )
    .unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Silence {
            targets: Selector::Entity(target),
        },
    )
    .unwrap();

    let target = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(target)
            .unwrap()
            .cost,
        eq(0)
    );
    assert_that!(
        simulation
            .app
            .world()
            .iter_entities()
            .filter(|entity| {
                entity.get::<EnchantmentDuration>().is_some()
                    && entity.get::<AttachedTo>().is_some()
            })
            .count(),
        eq(0)
    );
}

#[googletest::test]
fn transformation_discards_cost_modifiers_from_the_old_form() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Old form", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let target = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: target,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::AttachCostModifier {
            targets: Selector::Entity(target),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -2,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();
    transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("New form", 4, 2, 2),
    )
    .unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::AttachCostModifier {
            targets: Selector::Entity(target),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -1,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();

    let target = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(target)
            .unwrap()
            .cost,
        eq(3)
    );
}
