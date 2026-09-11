use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AttachedTo, AttackAuraCache, AttackState, ConditionTiming, ContinuousEffectDefinition,
    ContinuousModifier, Controller, DefinitionId, DrawContinuationPolicy, DrawOutcome,
    EnchantmentDuration, HealthAuraCache, HeroMetadata, HeroPowerState, KeepEnchantments,
    OtherAuraCache, PlayOrder, PlayerAudience, SilenceRemovable, SourceEligibilityPolicy,
    TargetAudience, TargetFilter, TargetKind, TargetRequirement, TimedCondition, TransformKind,
    TriggerCondition, TriggerDefinition, WoundedTargetPolicy, ZoneMovementKind, ZonePosition,
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
        drawn_card: None,
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
fn trigger_enchantment_attachment_reports_a_missing_explicit_target() {
    let mut simulation = simulation();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };

    assert_that!(
        execute_effect(
            simulation.app.world_mut(),
            &context,
            &Effect::AttachTriggerEnchantment {
                targets: Selector::Entity(GameEntityId(999)),
                triggers: vec![turn_end_trigger(Vec::new())],
                duration: EnchantmentDuration::Permanent,
                silence_removable: true,
            },
        ),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
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
        drawn_card: None,
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
        drawn_card: None,
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
                kind: TransformKind::Spell,
            },
            Effect::Copy {
                targets: Selector::DeclaredTarget,
                player: PlayerSelector::Controller,
                zone: Zone::Hand,
                board_index: None,
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
        drawn_card: None,
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
fn draw_expansion_rejects_counts_beyond_remaining_budget_without_allocating_slots() {
    for count in [2, u32::MAX] {
        let mut simulation = simulation();
        let world = simulation.app.world_mut();
        world.resource_mut::<Ruleset>().resolution_budget = 3;
        begin_sequence(world).unwrap();
        let before = world.resource::<ResolutionWork>().clone();

        assert_that!(
            execute_effect(
                world,
                &EffectContext {
                    source: None,
                    controller: PlayerId::One,
                    declared_target: None,
                    drawn_card: None,
                    origin: EffectOrigin::Other,
                },
                &Effect::Draw {
                    player: PlayerSelector::Controller,
                    count,
                },
            ),
            err(eq(&SimulationError::Resolution(
                ResolutionError::BudgetExhausted { operation: None }
            )))
        );
        assert_that!(world.resource::<ResolutionWork>(), eq(&before));
    }
}

#[googletest::test]
fn draw_continuation_reads_cost_after_draw_triggers() {
    let cost_trigger = TriggerDefinition {
        event: EventKind::CardDrawn,
        eligible_zones: vec![Zone::Hand],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventTargetsSelf,
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: vec![Effect::AttachCostModifier {
            targets: Selector::Source,
            modifier: CostModifier {
                operation: CostOperation::Set,
                value: 4,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        }],
    };
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck(
            "Jaina",
            vec![Card::spell("Reactive draw", 1).with_triggers(vec![cost_trigger])],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let drawn = simulation.snapshot().players[0].deck[0];
    let target = hero(&mut simulation, PlayerId::Two);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    execute_effect(
        world,
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: Some(target),
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::DrawThen {
            player: PlayerSelector::Controller,
            effects: vec![
                Effect::DealDamage {
                    targets: Selector::DeclaredTarget,
                    amount: ValueExpression::DrawnCardCost,
                },
                Effect::Move {
                    targets: Selector::DrawnCard,
                    player: PlayerSelector::Controller,
                    zone: Zone::Graveyard,
                    kind: ZoneMovementKind::Normal,
                },
            ],
            policy: DrawContinuationPolicy::RequireCard,
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    let target = game_entity(world, target).unwrap();
    assert_that!(world.get::<Damage>(target), eq(Some(&Damage(4))));
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Graveyard),
        eq(&[drawn])
    );
}

#[googletest::test]
fn draw_continuation_policies_cover_burn_and_fatigue() {
    for (has_card, policy, expected_resources) in [
        (true, DrawContinuationPolicy::RequireCard, 0),
        (true, DrawContinuationPolicy::RunWithoutCard, 1),
        (false, DrawContinuationPolicy::RequireCard, 0),
        (false, DrawContinuationPolicy::RunWithoutCard, 1),
    ] {
        let deck = has_card
            .then(|| Card::spell("Burned", 3))
            .into_iter()
            .collect();
        let mut simulation = Simulation::new([
            PlayerConfig::with_deck("Jaina", deck),
            PlayerConfig::new("Rexxar", vec![Card::spell("Stale binding", 5)]),
        ]);
        if has_card {
            simulation
                .app
                .world_mut()
                .resource_mut::<Ruleset>()
                .hand_limit = 0;
        }
        let stale = simulation.snapshot().players[1].hand[0];
        let target = hero(&mut simulation, PlayerId::Two);
        let world = simulation.app.world_mut();
        begin_sequence(world).unwrap();
        execute_effect(
            world,
            &EffectContext {
                source: None,
                controller: PlayerId::One,
                declared_target: Some(target),
                drawn_card: Some(stale),
                origin: EffectOrigin::Other,
            },
            &Effect::DrawThen {
                player: PlayerSelector::Controller,
                effects: vec![
                    Effect::DealDamage {
                        targets: Selector::DeclaredTarget,
                        amount: ValueExpression::DrawnCardCost,
                    },
                    Effect::GainResource {
                        player: PlayerSelector::Controller,
                        amount: 1,
                        temporary: true,
                    },
                ],
                policy,
            },
        )
        .unwrap();
        drive_resolution(world).unwrap();
        finish_sequence(world);

        assert_that!(
            player(world, PlayerId::One).unwrap().1.temporary_resources,
            eq(expected_resources)
        );
        let target = game_entity(world, target).unwrap();
        assert_that!(world.get::<Damage>(target), eq(Some(&Damage(0))));
    }
}

#[googletest::test]
fn fatigue_requests_resolve_damage_before_the_next_draw() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let hero = hero(&mut simulation, PlayerId::One);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    execute_effect(
        world,
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::Draw {
            player: PlayerSelector::Controller,
            count: 2,
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    let trace = simulation.trace();
    let fatigue = trace
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match entry {
            TraceEntry::DrawResolved {
                outcome: DrawOutcome::Fatigue { amount },
                ..
            } => Some((index, *amount)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(
        fatigue
            .iter()
            .map(|(_, amount)| *amount)
            .collect::<Vec<_>>(),
        eq(&vec![1, 2])
    );
    for (request, next_request) in fatigue.iter().zip(
        fatigue
            .iter()
            .skip(1)
            .map(|(index, _)| *index)
            .chain([trace.len()]),
    ) {
        let (request_index, amount) = *request;
        let request_trace = &trace[request_index + 1..next_request];
        assert_that!(
            request_trace.iter().any(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::ProposedDamage, targets, proposed: Some(proposed), .. } if targets == &[hero] && proposed == &amount)),
            is_true()
        );
        assert_that!(
            request_trace.iter().any(|entry| matches!(entry, TraceEntry::Damage { target, proposed, actual, .. } if *target == hero && proposed == &amount && actual == &amount)),
            is_true()
        );
        assert_that!(
            request_trace.iter().any(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::Damage, targets, actual: Some(actual), .. } if targets == &[hero] && actual == &amount)),
            is_true()
        );
    }
}

#[googletest::test]
fn explicitly_ordered_cross_player_draws_retain_controller_order() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::spell("One", 0)]),
        PlayerConfig::with_deck("Rexxar", vec![Card::spell("Two", 0)]),
    ]);
    let first = simulation.snapshot().players[0].deck[0];
    let second = simulation.snapshot().players[1].deck[0];
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    execute_effect(
        world,
        &EffectContext {
            source: None,
            controller: PlayerId::Two,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::Sequence(vec![
            Effect::Draw {
                player: PlayerSelector::Player(PlayerId::One),
                count: 1,
            },
            Effect::Draw {
                player: PlayerSelector::Player(PlayerId::Two),
                count: 1,
            },
        ]),
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    let players = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::DrawResolved { player, .. } => Some(*player),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(players, eq(&vec![PlayerId::One, PlayerId::Two]));
    assert_that!(simulation.snapshot().players[0].hand, eq(&vec![first]));
    assert_that!(simulation.snapshot().players[1].hand, eq(&vec![second]));
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
        drawn_card: None,
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
        drawn_card: None,
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
        drawn_card: None,
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
fn draw_then_programs_are_rejected_before_card_play_mutates_state() {
    let missing = NativeEffectId::new("synthetic:missing_draw_continuation");
    let card = Card::spell("Invalid Draw Continuation", 1).with_effects(vec![Effect::DrawThen {
        player: PlayerSelector::Controller,
        effects: vec![Effect::Native(missing.clone())],
        policy: DrawContinuationPolicy::RequireCard,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![card]),
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
    assert_that!(simulation.resolution_work().sequence_active, is_false());
    assert_that!(simulation.resolution_work().stack, is_empty());
    assert_that!(
        simulation.trace().last(),
        some(matches_pattern!(TraceEntry::ActionRejected { .. }))
    );
}

#[googletest::test]
fn invalid_played_self_placements_are_rejected_before_card_play_mutates_state() {
    let replacement = || Card::minion("Played self replacement", 0, 2, 2);
    let played_self = |targets| Effect::Transform {
        targets,
        card: replacement(),
        kind: TransformKind::PlayedSelf,
    };
    let trigger = self_event_trigger(EventKind::CardPlayed, vec![played_self(Selector::Source)]);
    let cases = [
        Card::spell("Spell placement", 1).with_effects(vec![played_self(Selector::Source)]),
        Card::minion("Non-source placement", 1, 1, 1)
            .with_effects(vec![played_self(Selector::DeclaredTarget)]),
        Card::minion("Draw continuation placement", 1, 1, 1).with_effects(vec![Effect::DrawThen {
            player: PlayerSelector::Controller,
            effects: vec![played_self(Selector::Source)],
            policy: DrawContinuationPolicy::RunWithoutCard,
        }]),
        Card::minion("Trigger placement", 1, 1, 1).with_triggers(vec![trigger]),
    ];

    for card in cases {
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![card]),
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
            err(matches_pattern!(SimulationError::InvalidTransformation(
                anything()
            )))
        );
        assert_that!(simulation.snapshot(), eq(&before));
        assert_that!(simulation.resolution_work().sequence_active, is_false());
        assert_that!(simulation.resolution_work().stack, is_empty());
    }
}

#[googletest::test]
fn draw_then_continuations_cannot_modify_an_enclosing_event() {
    let trigger = TriggerDefinition {
        event: EventKind::ProposedDamage,
        eligible_zones: vec![Zone::Play],
        conditions: Vec::new(),
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::IncludePendingDestroy,
        effect_program: vec![Effect::DrawThen {
            player: PlayerSelector::Controller,
            effects: vec![Effect::ModifyEventValue {
                operation: EventValueOperation::Add,
                value: ValueExpression::Constant(1),
            }],
            policy: DrawContinuationPolicy::RunWithoutCard,
        }],
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![Card::minion("Invalid delayed modifier", 1, 1, 1).with_triggers(vec![trigger])],
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
        err(eq(&SimulationError::NoModifiableEventValue))
    );
    assert_that!(simulation.snapshot(), eq(&before));
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
        TransformKind::Spell,
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
        drawn_card: None,
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
        drawn_card: None,
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
        TransformKind::Spell,
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

#[googletest::test]
fn invalid_transformation_is_atomic() {
    let mut simulation = simulation();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Invalid target", 1, 1, 2),
        Zone::Play,
    )
    .unwrap();
    let attachment = attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        target,
        StatModifier {
            attack: 2,
            health: 2,
            silence_removable: false,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(target_entity)
        .remove::<DefinitionId>();
    let before = simulation.checkpoint().unwrap();

    let result = transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("Replacement", 2, 2, 3),
        TransformKind::Spell,
    );

    assert_that!(result, err(anything()));
    assert_that!(simulation.checkpoint().unwrap(), eq(&before));

    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(target_entity)
        .insert(DefinitionId("synthetic:invalid_target".to_string()));
    let attachment_entity = game_entity(simulation.app.world(), attachment).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(attachment_entity)
        .remove::<PlayOrder>();
    let before_invalid_attachment = simulation.checkpoint().unwrap();
    let attachment_result = transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("Replacement", 2, 2, 3),
        TransformKind::Spell,
    );
    assert_that!(attachment_result, err(anything()));
    assert_that!(
        simulation.checkpoint().unwrap(),
        eq(&before_invalid_attachment)
    );

    let before_missing = simulation.checkpoint().unwrap();
    let missing_result = transform_entity(
        simulation.app.world_mut(),
        GameEntityId(u64::MAX),
        Card::minion("Replacement", 2, 2, 3),
        TransformKind::Spell,
    );
    assert_that!(missing_result, err(anything()));
    assert_that!(simulation.checkpoint().unwrap(), eq(&before_missing));
}

#[googletest::test]
fn transformation_rejects_unsupported_replacement_kinds_atomically() {
    let hero = Card::hero("Replacement hero", 30);
    let mut player = Card::minion("Replacement player", 0, 0, 1);
    player.kind = EntityKind::Player;
    let mut enchantment = Card::minion("Replacement enchantment", 0, 0, 1);
    enchantment.kind = EntityKind::Enchantment;

    for replacement in [hero, player, enchantment] {
        let mut simulation = simulation();
        let target = spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::minion("Transform target", 1, 1, 2),
            Zone::Play,
        )
        .unwrap();
        attach_stat_modifier(
            simulation.app.world_mut(),
            PlayerId::One,
            target,
            StatModifier {
                attack: 2,
                health: 2,
                silence_removable: false,
            },
            EnchantmentDuration::Permanent,
        )
        .unwrap();
        let before = simulation.checkpoint().unwrap();

        let result = transform_entity(
            simulation.app.world_mut(),
            target,
            replacement,
            TransformKind::Spell,
        );

        assert_that!(
            result,
            err(matches_pattern!(SimulationError::InvalidTransformation(
                anything()
            )))
        );
        assert_that!(simulation.checkpoint().unwrap(), eq(&before));
    }
}

#[googletest::test]
fn transformation_rejects_non_minion_targets_atomically() {
    let mut simulation = simulation();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::hero("Unsupported target", 30),
        Zone::Hand,
    )
    .unwrap();
    let before = simulation.checkpoint().unwrap();

    let result = transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("Replacement", 2, 2, 3),
        TransformKind::Spell,
    );

    assert_that!(
        result,
        err(matches_pattern!(SimulationError::InvalidTransformation(
            anything()
        )))
    );
    assert_that!(simulation.checkpoint().unwrap(), eq(&before));
}

#[googletest::test]
fn transformation_detaches_enchantments_in_play_order_then_entity_id() {
    let mut simulation = simulation();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Attachment host", 1, 1, 2),
        Zone::Play,
    )
    .unwrap();
    let first = attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        target,
        StatModifier {
            attack: 1,
            health: 0,
            silence_removable: false,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let second = attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        target,
        StatModifier {
            attack: 2,
            health: 0,
            silence_removable: false,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let first_entity = game_entity(simulation.app.world(), first).unwrap();
    let second_entity = game_entity(simulation.app.world(), second).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(first_entity)
        .insert(PlayOrder(20));
    simulation
        .app
        .world_mut()
        .entity_mut(second_entity)
        .insert(PlayOrder(10));

    transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("Detached host", 1, 3, 3),
        TransformKind::Spell,
    )
    .unwrap();

    let removed = simulation
        .app
        .world()
        .resource::<ZoneIndex>()
        .entities(PlayerId::One, Zone::RemovedFromGame);
    let first_position = removed.iter().position(|id| *id == first).unwrap();
    let second_position = removed.iter().position(|id| *id == second).unwrap();
    assert_that!(second_position, lt(first_position));
}

#[googletest::test]
fn transformation_uses_entity_id_to_break_equal_attachment_play_orders() {
    let mut simulation = simulation();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Equal-order host", 1, 1, 2),
        Zone::Play,
    )
    .unwrap();
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    let higher_id = GameEntityId(1_001);
    let lower_id = GameEntityId(1_000);
    for id in [higher_id, lower_id] {
        simulation.app.world_mut().spawn((
            GameObject,
            id,
            EntityKind::Enchantment,
            Controller(PlayerId::One),
            PlayOrder(10),
            EnchantmentDuration::Permanent,
            AttachedTo(target_entity),
        ));
        crate::zone::insert_into_zone(
            simulation.app.world_mut(),
            id,
            PlayerId::One,
            Zone::Play,
            None,
        )
        .unwrap();
    }

    transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("Equal-order replacement", 1, 2, 2),
        TransformKind::Spell,
    )
    .unwrap();

    let removed = simulation
        .app
        .world()
        .resource::<ZoneIndex>()
        .entities(PlayerId::One, Zone::RemovedFromGame);
    let lower_position = removed.iter().position(|id| *id == lower_id).unwrap();
    let higher_position = removed.iter().position(|id| *id == higher_id).unwrap();
    assert_that!(lower_position, lt(higher_position));
}

#[googletest::test]
fn transformation_replaces_form_state_and_preserves_stable_state() {
    let mut simulation = simulation();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Old form", 5, 4, 6).with_targeting(TargetRequirement::Optional(
            TargetFilter {
                audience: TargetAudience::Friendly,
                kind: TargetKind::Minion,
            },
        )),
        Zone::Play,
    )
    .unwrap();
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(target_entity)
        .insert((
            PlayOrder(77),
            Armor(9),
            HeroMetadata::default(),
            HeroPowerState::default(),
            Abilities(vec!["old ability".to_string()]),
            Enchantments(vec![GameEntityId(404)]),
            PendingDestroy,
            Silenced,
            KeepEnchantments,
            HealthAuraCache(vec![AuraApplication {
                provider: GameEntityId(101),
                definition_index: 0,
                modifier: AuraModifier::MaximumHealth(2),
            }]),
            AttackAuraCache(vec![AuraApplication {
                provider: GameEntityId(102),
                definition_index: 0,
                modifier: AuraModifier::Attack(3),
            }]),
            OtherAuraCache(vec![AuraApplication {
                provider: GameEntityId(103),
                definition_index: 0,
                modifier: AuraModifier::Immune,
            }]),
        ));
    let controller = *simulation
        .app
        .world()
        .get::<Controller>(target_entity)
        .unwrap();
    let zone = *simulation.app.world().get::<Zone>(target_entity).unwrap();
    let position = *simulation
        .app
        .world()
        .get::<ZonePosition>(target_entity)
        .unwrap();

    transform_entity(
        simulation.app.world_mut(),
        target,
        Card::minion("New form", 2, 2, 3).with_targeting(TargetRequirement::Required(
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Character,
            },
        )),
        TransformKind::Spell,
    )
    .unwrap();

    let transformed = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation.app.world().get::<GameEntityId>(transformed),
        eq(Some(&target))
    );
    assert_that!(
        simulation.app.world().get::<Controller>(transformed),
        eq(Some(&controller))
    );
    assert_that!(
        simulation.app.world().get::<Zone>(transformed),
        eq(Some(&zone))
    );
    assert_that!(
        simulation.app.world().get::<ZonePosition>(transformed),
        eq(Some(&position))
    );
    assert_that!(
        simulation.app.world().get::<PlayOrder>(transformed),
        eq(Some(&PlayOrder(77)))
    );
    assert_that!(simulation.app.world().get::<Armor>(transformed), none());
    assert_that!(
        simulation.app.world().get::<HeroMetadata>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<HeroPowerState>(transformed),
        none()
    );
    assert_that!(simulation.app.world().get::<Abilities>(transformed), none());
    assert_that!(
        simulation.app.world().get::<Enchantments>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<PendingDestroy>(transformed),
        none()
    );
    assert_that!(simulation.app.world().get::<Silenced>(transformed), none());
    assert_that!(
        simulation.app.world().get::<KeepEnchantments>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<HealthAuraCache>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<AttackAuraCache>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<OtherAuraCache>(transformed),
        none()
    );
    assert_that!(
        simulation.app.world().get::<AttackState>(transformed),
        eq(Some(&AttackState::default()))
    );
    assert_eq!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(transformed)
            .unwrap()
            .targeting,
        TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        })
    );
}
