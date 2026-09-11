use googletest::prelude::*;

use super::effect_executor::copy_entity;
use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AuraRefreshPlan, CopyRequest, CopyStatePolicy, DamageRequest, DrawContinuationPolicy,
    DrawOutcome, DrawRequest, DrawResultSlot, DrawResultSlotId, EnchantmentDuration,
    HealthAuraCache, KeepEnchantments, KeywordModifier, OtherAuraCache, Player, SequenceStep,
    SilenceRemovable,
};

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
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
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
    world.resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
    assert_that!(
        world
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Graveyard)
            .len(),
        eq(1)
    );
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
            },
            EnchantmentDuration::Permanent,
        ),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(
        silence_entity(world, GameEntityId(999)),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(
        transform_entity(
            world,
            GameEntityId(999),
            Card::minion("Missing", 0, 1, 1),
            crate::TransformKind::Spell,
        ),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(999))))
    );
    assert_that!(copy_card_data(world, GameEntityId(999)), none());
    assert_that!(hero_id(world, PlayerId::One), eq(Some(first_hero)));
}

#[googletest::test]
fn current_operation_dispatch_handles_empty_and_missing_event_work() {
    let mut simulation = simulation();
    let world = simulation.app.world_mut();
    execute_current_resolution_op(world);
    assert_that!(world.resource::<OperationFailure>().0, none());

    for operation in [
        ResolutionOp::FinishEvent(EventId(999)),
        ResolutionOp::ResolveEvent(EventId(999)),
    ] {
        world.resource_mut::<CurrentResolutionOp>().0 = Some(StackedResolutionOp {
            id: ResolutionId(999),
            operation,
        });
        execute_current_resolution_op(world);
        assert_that!(
            matches!(
                world.resource_mut::<OperationFailure>().0.take(),
                Some(SimulationError::Resolution(ResolutionError::MissingEvent(
                    EventId(999)
                )))
            ),
            is_true()
        );
    }
}

#[googletest::test]
fn choice_suspension_retains_lower_stack_work_and_resumes_selected_branch_first() {
    let mut simulation = simulation();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
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
fn runtime_invariants_reject_malformed_player_hero_and_power_roles() {
    let mut missing_player = simulation();
    let player_entity = missing_player
        .app
        .world()
        .iter_entities()
        .find(|entity| {
            entity
                .get::<Player>()
                .is_some_and(|player| player.id == PlayerId::One)
        })
        .unwrap()
        .id();
    missing_player
        .app
        .world_mut()
        .entity_mut(player_entity)
        .remove::<Player>();
    assert_that!(
        missing_player.assert_invariants().unwrap_err().to_string(),
        contains_substring("0 Player entities")
    );

    let mut invalid_player = simulation();
    let player_entity = invalid_player
        .app
        .world()
        .iter_entities()
        .find(|entity| {
            entity
                .get::<Player>()
                .is_some_and(|player| player.id == PlayerId::One)
        })
        .unwrap()
        .id();
    invalid_player
        .app
        .world_mut()
        .entity_mut(player_entity)
        .insert(EntityKind::Minion);
    assert_that!(
        invalid_player.assert_invariants().unwrap_err().to_string(),
        contains_substring("invalid Player components")
    );

    let mut missing_hero = simulation();
    let hero = hero_id(missing_hero.app.world(), PlayerId::One).unwrap();
    let hero_entity = game_entity(missing_hero.app.world(), hero).unwrap();
    missing_hero
        .app
        .world_mut()
        .entity_mut(hero_entity)
        .insert(EntityKind::Spell);
    assert_that!(
        missing_hero.assert_invariants().unwrap_err().to_string(),
        contains_substring("0 active Heroes")
    );

    let mut incomplete_hero = simulation();
    let hero = hero_id(incomplete_hero.app.world(), PlayerId::One).unwrap();
    let hero_entity = game_entity(incomplete_hero.app.world(), hero).unwrap();
    incomplete_hero
        .app
        .world_mut()
        .entity_mut(hero_entity)
        .remove::<Armor>();
    assert_that!(
        incomplete_hero.assert_invariants().unwrap_err().to_string(),
        contains_substring("Hero lacks required components")
    );

    let mut missing_power = simulation();
    let power = missing_power.snapshot().players[0].hero_power.unwrap();
    let power_entity = game_entity(missing_power.app.world(), power).unwrap();
    missing_power
        .app
        .world_mut()
        .entity_mut(power_entity)
        .insert(EntityKind::Spell);
    assert_that!(
        missing_power.assert_invariants().unwrap_err().to_string(),
        contains_substring("0 active Hero Powers")
    );

    let mut incomplete_power = simulation();
    let power = incomplete_power.snapshot().players[0].hero_power.unwrap();
    let power_entity = game_entity(incomplete_power.app.world(), power).unwrap();
    incomplete_power
        .app
        .world_mut()
        .entity_mut(power_entity)
        .remove::<CardRuntime>();
    assert_that!(
        incomplete_power
            .assert_invariants()
            .unwrap_err()
            .to_string(),
        contains_substring("Hero Power lacks required components")
    );
}

#[googletest::test]
fn checkpoint_roundtrip_preserves_optional_components_and_relationships() {
    let mut original = simulation();
    let target = hand_card(&mut original, PlayerId::One);
    let world = original.app.world_mut();
    attach_stat_modifier(
        world,
        PlayerId::One,
        target,
        StatModifier {
            attack: 2,
            health: 3,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let permanent_enchantment = world
        .iter_entities()
        .find_map(|entity| {
            entity
                .contains::<StatModifier>()
                .then(|| entity.get::<GameEntityId>().copied())
                .flatten()
        })
        .unwrap();
    attach_stat_modifier(
        world,
        PlayerId::One,
        target,
        StatModifier {
            attack: 0,
            health: 0,
            silence_removable: true,
        },
        EnchantmentDuration::EndOfTurn(PlayerId::One),
    )
    .unwrap();
    let temporary_enchantment = world
        .iter_entities()
        .find_map(|entity| {
            (entity.contains::<StatModifier>()
                && entity.get::<GameEntityId>().copied() != Some(permanent_enchantment))
            .then(|| entity.get::<GameEntityId>().copied())
            .flatten()
        })
        .unwrap();
    let target_entity = game_entity(world, target).unwrap();
    world.entity_mut(target_entity).insert((
        Armor(4),
        PendingDestroy,
        Abilities(vec!["Battlecry".to_string()]),
        Enchantments(vec![permanent_enchantment]),
        AttackAuraCache(vec![AuraApplication {
            provider: target,
            definition_index: 0,
            modifier: AuraModifier::Attack(1),
        }]),
        HealthAuraCache(vec![AuraApplication {
            provider: target,
            definition_index: 1,
            modifier: AuraModifier::MaximumHealth(2),
        }]),
        OtherAuraCache(vec![AuraApplication {
            provider: target,
            definition_index: 2,
            modifier: AuraModifier::Immune,
        }]),
        KeepEnchantments,
        Silenced,
        DeathRecord {
            entity: target,
            controller: PlayerId::One,
            kind: EntityKind::Minion,
            play_order: 3,
            remembered_zone_position: 0,
            simultaneous_ordinal: 0,
            turn_of_death: 1,
        },
    ));
    let enchantment_entity = game_entity(world, temporary_enchantment).unwrap();
    world.entity_mut(enchantment_entity).insert((
        KeywordModifier {
            keyword: Keyword::Taunt,
            granted: true,
            silence_removable: true,
        },
        SilenceRemovable,
    ));

    let checkpoint = original.checkpoint().unwrap();
    assert_that!(
        checkpoint
            .entities
            .iter()
            .find(|entity| entity.id == permanent_enchantment)
            .unwrap()
            .enchantment_duration,
        eq(Some(EnchantmentDuration::Permanent))
    );
    assert_that!(
        checkpoint
            .entities
            .iter()
            .find(|entity| entity.id == temporary_enchantment)
            .unwrap()
            .enchantment_duration,
        eq(Some(EnchantmentDuration::EndOfTurn(PlayerId::One)))
    );
    let mut restored = simulation();
    restored.restore(checkpoint.clone()).unwrap();

    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
    let restored_target = game_entity(restored.app.world(), target).unwrap();
    assert_that!(
        restored.app.world().get::<Armor>(restored_target),
        eq(Some(&Armor(4)))
    );
    assert_that!(
        restored.app.world().get::<Abilities>(restored_target),
        eq(Some(&Abilities(vec!["Battlecry".to_string()])))
    );
    assert_that!(
        restored
            .app
            .world()
            .entity(restored_target)
            .contains::<KeepEnchantments>(),
        is_true()
    );
    assert_that!(
        restored.app.world().get::<HealthAuraCache>(restored_target),
        some(anything())
    );
    assert_that!(
        restored.app.world().get::<OtherAuraCache>(restored_target),
        some(anything())
    );
    let restored_enchantment = game_entity(restored.app.world(), temporary_enchantment).unwrap();
    assert_that!(
        restored
            .app
            .world()
            .get::<crate::AttachedTo>(restored_enchantment)
            .map(|attached| attached.0),
        eq(Some(restored_target))
    );
    assert_that!(
        restored
            .app
            .world()
            .entity(restored_enchantment)
            .contains::<SilenceRemovable>(),
        is_true()
    );
    assert_that!(
        restored
            .app
            .world()
            .get::<EnchantmentDuration>(restored_enchantment),
        eq(Some(&EnchantmentDuration::EndOfTurn(PlayerId::One)))
    );
}

#[googletest::test]
fn schema_seven_json_roundtrip_preserves_trigger_enchantment_payloads() {
    let trigger = crate::TriggerDefinition {
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
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Permanent host", 0, 1, 1),
                Card::minion("Timed host", 0, 1, 1),
                Card::minion("Removed host", 0, 1, 1),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let hosts = simulation.snapshot().players[0].hand.clone();
    let [permanent_host, timed_host, removed_host] = hosts.as_slice() else {
        panic!("fixture should have exactly three host cards");
    };
    let (permanent_host, timed_host, removed_host) = (*permanent_host, *timed_host, *removed_host);

    let mut attach = |host, controller, duration| {
        execute_effect(
            simulation.app.world_mut(),
            &EffectContext {
                source: None,
                controller,
                declared_target: None,
                drawn_card: None,
                origin: EffectOrigin::Other,
            },
            &Effect::AttachTriggerEnchantment {
                targets: Selector::Entity(host),
                triggers: vec![trigger.clone()],
                duration,
                silence_removable: true,
            },
        )
        .unwrap();
        let host_entity = game_entity(simulation.app.world(), host).unwrap();
        simulation
            .app
            .world()
            .iter_entities()
            .find(|entity| {
                entity.get::<crate::AttachedTo>().map(|attached| attached.0) == Some(host_entity)
                    && entity.contains::<RuntimeTriggers>()
            })
            .and_then(|entity| entity.get::<GameEntityId>())
            .copied()
            .unwrap()
    };
    let permanent = attach(
        permanent_host,
        PlayerId::One,
        EnchantmentDuration::Permanent,
    );
    let timed = attach(
        timed_host,
        PlayerId::Two,
        EnchantmentDuration::EndOfTurn(PlayerId::Two),
    );
    let removed = attach(
        removed_host,
        PlayerId::One,
        EnchantmentDuration::EndOfTurnSeries(PlayerId::One),
    );
    silence_entity(simulation.app.world_mut(), removed_host).unwrap();

    let checkpoint = simulation.checkpoint().unwrap();
    assert_that!(checkpoint.schema_version, eq(7));
    for (id, controller, zone, duration, attached_to) in [
        (
            permanent,
            PlayerId::One,
            Zone::Play,
            EnchantmentDuration::Permanent,
            Some(permanent_host),
        ),
        (
            timed,
            PlayerId::Two,
            Zone::Play,
            EnchantmentDuration::EndOfTurn(PlayerId::Two),
            Some(timed_host),
        ),
        (
            removed,
            PlayerId::One,
            Zone::RemovedFromGame,
            EnchantmentDuration::EndOfTurnSeries(PlayerId::One),
            None,
        ),
    ] {
        let entity = checkpoint
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .unwrap();
        assert_that!(entity.runtime_triggers, eq(&Some(vec![trigger.clone()])));
        assert_that!(entity.enchantment_duration, eq(Some(duration)));
        assert_that!(entity.controller, eq(Some(controller)));
        assert_that!(entity.zone, eq(Some(zone)));
        assert_that!(entity.play_order.is_some(), is_true());
        assert_that!(entity.attached_to, eq(attached_to));
    }

    let json = checkpoint.to_json().unwrap();
    let decoded = SimulationCheckpoint::from_json(&json).unwrap();
    assert_that!(decoded, eq(&checkpoint));
    let restored = Simulation::from_checkpoint(decoded).unwrap();
    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
}

#[googletest::test]
fn checkpoints_reject_unsupported_trigger_enchantment_policies_only_for_enchantments() {
    let ordinary_death_trigger = crate::TriggerDefinition {
        event: EventKind::Death,
        eligible_zones: vec![Zone::Graveyard],
        conditions: Vec::new(),
        source_eligibility: crate::SourceEligibilityPolicy::RememberedSource,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
        effect_program: Vec::new(),
    };
    let valid_trigger = crate::TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: Vec::new(),
        source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: Vec::new(),
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Ordinary Death trigger", 0, 1, 1)
                    .with_triggers(vec![ordinary_death_trigger]),
                Card::minion("Enchantment host", 0, 1, 1),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let _ordinary = hand_card(&mut simulation, PlayerId::One);
    let host = hand_card(&mut simulation, PlayerId::One);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachTriggerEnchantment {
            targets: Selector::Entity(host),
            triggers: vec![valid_trigger.clone()],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        },
    )
    .unwrap();
    let base = simulation.checkpoint().unwrap();
    Simulation::from_checkpoint(base.clone()).unwrap();

    let mut empty = base.clone();
    empty
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .runtime_triggers = Some(Vec::new());

    let mut death = valid_trigger.clone();
    death.event = EventKind::Death;
    let mut death_checkpoint = base.clone();
    death_checkpoint
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .runtime_triggers = Some(vec![death]);

    let mut wrong_zone = valid_trigger.clone();
    wrong_zone.eligible_zones = vec![Zone::Hand];
    let mut wrong_zone_checkpoint = base.clone();
    wrong_zone_checkpoint
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .runtime_triggers = Some(vec![wrong_zone]);

    let mut remembered = valid_trigger;
    remembered.source_eligibility = crate::SourceEligibilityPolicy::RememberedSource;
    let mut remembered_checkpoint = base;
    remembered_checkpoint
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .runtime_triggers = Some(vec![remembered]);

    for checkpoint in [
        empty,
        death_checkpoint,
        wrong_zone_checkpoint,
        remembered_checkpoint,
    ] {
        assert_that!(
            Simulation::from_checkpoint(checkpoint).map(|_| ()),
            err(matches_pattern!(
                SimulationError::InvalidTriggerEnchantment(_)
            )),
        );
    }
}

#[googletest::test]
fn checkpoints_reject_enchantments_without_durations_and_schema_version_six() {
    let mut simulation = simulation();
    let target = hand_card(&mut simulation, PlayerId::One);
    attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        target,
        StatModifier {
            attack: 1,
            health: 1,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();

    let mut missing_duration = simulation.checkpoint().unwrap();
    missing_duration
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .enchantment_duration = None;
    assert_that!(
        Simulation::from_checkpoint(missing_duration).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("enchantment duration")
        ))),
    );

    let mut wrong_zone = simulation.checkpoint().unwrap();
    wrong_zone
        .entities
        .iter_mut()
        .find(|entity| entity.kind == Some(EntityKind::Enchantment))
        .unwrap()
        .zone = Some(Zone::Hand);
    assert_that!(
        Simulation::from_checkpoint(wrong_zone).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("not in Play")
        ))),
    );

    let mut old_schema = simulation.checkpoint().unwrap();
    old_schema.schema_version = 6;
    assert_that!(
        Simulation::from_checkpoint(old_schema).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("unsupported checkpoint schema version 6")
        ))),
    );
}

#[googletest::test]
fn checkpoint_builder_rejects_executing_ops_and_invalid_world_relationships() {
    let mut simulation = simulation();
    simulation
        .app
        .world_mut()
        .resource_mut::<CurrentResolutionOp>()
        .0 = Some(StackedResolutionOp {
        id: ResolutionId(1),
        operation: ResolutionOp::CheckOutcome,
    });
    assert_that!(
        matches!(simulation.checkpoint(), Err(SimulationError::Checkpoint(_))),
        is_true()
    );
    simulation
        .app
        .world_mut()
        .resource_mut::<CurrentResolutionOp>()
        .0 = None;

    let without_id = simulation.app.world_mut().spawn(GameObject).id();
    assert_that!(
        matches!(simulation.checkpoint(), Err(SimulationError::Checkpoint(_))),
        is_true()
    );
    simulation.app.world_mut().despawn(without_id);

    let raw_target = simulation.app.world_mut().spawn_empty().id();
    simulation
        .app
        .world_mut()
        .spawn((GameEntityId(999), crate::AttachedTo(raw_target)));
    assert_that!(
        matches!(simulation.checkpoint(), Err(SimulationError::Checkpoint(_))),
        is_true()
    );
    assert_that!(
        SimulationCheckpoint::from_json("not json"),
        err(matches_pattern!(SimulationError::Checkpoint(_)))
    );
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
fn checkpoints_reject_stale_draw_slot_counters() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    let slot = DrawResultSlotId(4);
    checkpoint.resolution.next_draw_result_slot_id = slot.0;
    checkpoint
        .resolution
        .draw_result_slots
        .insert(slot, DrawResultSlot::default());
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    let json = checkpoint.to_json().unwrap();
    let decoded = SimulationCheckpoint::from_json(&json).unwrap();

    assert_that!(
        Simulation::from_checkpoint(decoded).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("next draw result slot ID 4 does not exceed highest retained ID 4")
        ))),
    );
}

#[googletest::test]
fn checkpoint_roundtrip_resumes_a_suspended_pending_draw_continuation() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    let source = checkpoint.entities[0].id;
    let slot = DrawResultSlotId(4);
    let context = EffectContext {
        source: Some(source),
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    let choice = ChoiceId(50);
    let option = ChoiceId(51);
    checkpoint.resolution.next_draw_result_slot_id = 5;
    checkpoint
        .resolution
        .draw_result_slots
        .insert(slot, DrawResultSlot::default());
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(7),
        operation: ResolutionOp::ContinueDraw {
            result: slot,
            context,
            effects: vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
            policy: DrawContinuationPolicy::RequireCard,
        },
    });
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(8),
        operation: ResolutionOp::ProcessDraw(DrawRequest {
            player: PlayerId::One,
            source: Some(source),
            result: slot,
        }),
    });
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(9),
        operation: ResolutionOp::RequestChoice(ChoiceRequest {
            id: choice,
            player: PlayerId::One,
            options: vec![ChoiceOption {
                id: option,
                operations: vec![ResolutionOp::CheckOutcome],
            }],
        }),
    });
    checkpoint.resolution.next_resolution_id = 10;
    checkpoint.resolution.remaining_budget = checkpoint.ruleset.resolution_budget;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    let json = checkpoint.to_json().unwrap();
    let decoded = SimulationCheckpoint::from_json(&json).unwrap();
    let mut original = Simulation::from_checkpoint(decoded).unwrap();
    drive_resolution(original.app.world_mut()).unwrap();
    assert_that!(original.pending_choice().unwrap().request.id, eq(choice));
    let suspended = original.checkpoint().unwrap();
    let json = suspended.to_json().unwrap();
    let mut restored =
        Simulation::from_checkpoint(SimulationCheckpoint::from_json(&json).unwrap()).unwrap();

    original.choose(option).unwrap();
    restored.choose(option).unwrap();

    assert_that!(restored.snapshot(), eq(&original.snapshot()));
    assert_that!(restored.trace(), eq(original.trace()));
    assert_that!(
        restored.checkpoint().unwrap(),
        eq(&original.checkpoint().unwrap())
    );
}

#[googletest::test]
fn checkpoints_reject_missing_or_non_monotonic_draw_slot_references() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let source = base.entities[0].id;
    let slot = DrawResultSlotId(4);
    let context = EffectContext {
        source: Some(source),
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    let operations = [
        ResolutionOp::ProcessDraw(DrawRequest {
            player: PlayerId::One,
            source: Some(source),
            result: slot,
        }),
        ResolutionOp::FinishDraw(slot),
        ResolutionOp::ContinueDraw {
            result: slot,
            context,
            effects: Vec::new(),
            policy: DrawContinuationPolicy::RequireCard,
        },
    ];

    for operation in operations {
        let mut checkpoint = base.clone();
        checkpoint.resolution.next_draw_result_slot_id = 5;
        checkpoint.resolution.stack.push(StackedResolutionOp {
            id: ResolutionId(0),
            operation,
        });
        checkpoint.resolution.next_resolution_id = 1;
        checkpoint.resolution.sequence_active = true;
        checkpoint.game.status = SimulationStatus::Resolving;
        let json = checkpoint.to_json().unwrap();
        let decoded = SimulationCheckpoint::from_json(&json).unwrap();

        assert_that!(
            Simulation::from_checkpoint(decoded).map(|_| ()),
            err(matches_pattern!(SimulationError::Checkpoint(
                contains_substring("missing draw result slot DrawResultSlotId(4)")
            ))),
        );
    }

    let mut checkpoint = base;
    checkpoint.resolution.next_draw_result_slot_id = 4;
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(0),
        operation: ResolutionOp::ProcessDraw(DrawRequest {
            player: PlayerId::One,
            source: Some(source),
            result: slot,
        }),
    });
    checkpoint.resolution.next_resolution_id = 1;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    assert_that!(
        Simulation::from_checkpoint(checkpoint).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("draw result slot ID 4 is not below next ID 4")
        ))),
    );
}

#[googletest::test]
fn checkpoints_reject_missing_event_and_event_slot_operation_references() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let source = base.entities[0].id;
    let context = EffectContext {
        source: Some(source),
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    let missing_event = EventId(3);
    let missing_slot = EventSlotId(4);
    let operations = [
        ResolutionOp::ResolveEvent(missing_event),
        ResolutionOp::FinishEvent(missing_event),
        ResolutionOp::RunEffect {
            context,
            effect: Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            },
            event: Some(missing_event),
        },
        ResolutionOp::ResolveEventSlot(missing_slot),
        ResolutionOp::ProcessDamage {
            request: DamageRequest {
                source: Some(source),
                target: source,
                proposed: 1,
            },
            actual_event: missing_slot,
            ordinal: 0,
        },
    ];

    for operation in operations {
        let mut checkpoint = base.clone();
        checkpoint.resolution.next_event_id = 5;
        checkpoint.resolution.next_event_slot_id = 5;
        checkpoint.resolution.stack.push(StackedResolutionOp {
            id: ResolutionId(0),
            operation,
        });
        checkpoint.resolution.next_resolution_id = 1;
        checkpoint.resolution.sequence_active = true;
        checkpoint.game.status = SimulationStatus::Resolving;

        assert_that!(
            Simulation::from_checkpoint(checkpoint).map(|_| ()),
            err(matches_pattern!(SimulationError::Checkpoint(anything()))),
        );
    }

    let mut checkpoint = base;
    checkpoint.resolution.next_event_id = 5;
    checkpoint.resolution.event_slots.insert(
        EventSlotId(0),
        PreparedEventSlot {
            event: Some(missing_event),
        },
    );
    checkpoint.resolution.next_event_slot_id = 1;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    assert_that!(
        Simulation::from_checkpoint(checkpoint).map(|_| ()),
        err(matches_pattern!(SimulationError::Checkpoint(
            contains_substring("missing prepared event EventId(3)")
        ))),
    );
}

#[googletest::test]
fn checkpoints_reject_dangling_resolution_entity_references_recursively() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let valid = base.entities[0].id;
    let missing = GameEntityId(u64::MAX);
    let context = |source, declared_target, drawn_card| EffectContext {
        source,
        controller: PlayerId::One,
        declared_target,
        drawn_card,
        origin: EffectOrigin::Other,
    };
    let operations = vec![
        ResolutionOp::RunSequenceStep(SequenceStep::PlayCard {
            player: PlayerId::One,
            card: missing,
            target: None,
            board_index: None,
        }),
        ResolutionOp::RefreshAuras(AuraRefreshPlan::PlayedProvider(missing)),
        ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::Damage,
            source: Some(missing),
            targets: vec![valid],
            controller: PlayerId::One,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        }),
        ResolutionOp::RunEffect {
            context: context(Some(missing), None, None),
            effect: Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            },
            event: None,
        },
        ResolutionOp::RunEffect {
            context: context(Some(valid), Some(missing), None),
            effect: Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            },
            event: None,
        },
        ResolutionOp::RunEffect {
            context: context(Some(valid), None, Some(missing)),
            effect: Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            },
            event: None,
        },
        ResolutionOp::ProcessDamageBatch(vec![DamageRequest {
            source: Some(valid),
            target: missing,
            proposed: 1,
        }]),
        ResolutionOp::ProcessDraw(DrawRequest {
            player: PlayerId::One,
            source: Some(missing),
            result: DrawResultSlotId(0),
        }),
        ResolutionOp::TransformEntity {
            target: missing,
            source: Some(valid),
            card: Card::minion("Future Transform", 1, 1, 1),
            kind: crate::TransformKind::Spell,
        },
        ResolutionOp::CopyEntity(CopyRequest {
            source: missing,
            controller: PlayerId::One,
            destination: Zone::Hand,
            board_index: None,
            policy: CopyStatePolicy::CurrentForm,
        }),
        ResolutionOp::RequestChoice(ChoiceRequest {
            id: ChoiceId(60),
            player: PlayerId::One,
            options: vec![ChoiceOption {
                id: ChoiceId(61),
                operations: vec![ResolutionOp::RunEffect {
                    context: context(Some(valid), None, None),
                    effect: Effect::Sequence(vec![Effect::Destroy {
                        targets: Selector::Random(Box::new(Selector::Entity(missing))),
                    }]),
                    event: None,
                }],
            }],
        }),
    ];

    for operation in operations {
        let mut checkpoint = base.clone();
        checkpoint.resolution.next_draw_result_slot_id = 1;
        checkpoint
            .resolution
            .draw_result_slots
            .insert(DrawResultSlotId(0), DrawResultSlot::default());
        checkpoint.resolution.stack.push(StackedResolutionOp {
            id: ResolutionId(0),
            operation,
        });
        checkpoint.resolution.next_resolution_id = 1;
        checkpoint.resolution.sequence_active = true;
        checkpoint.game.status = SimulationStatus::Resolving;

        assert_that!(
            Simulation::from_checkpoint(checkpoint).map(|_| ()),
            err(matches_pattern!(SimulationError::Checkpoint(
                contains_substring("missing logical entity")
            ))),
        );
    }

    let mut event = base.clone();
    event.resolution.events.insert(
        EventId(0),
        PreparedEvent {
            context: EventContext {
                kind: EventKind::Damage,
                source: Some(valid),
                targets: vec![missing],
                controller: PlayerId::One,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            },
            prechecked_triggers: None,
            candidates: None,
        },
    );
    event.resolution.next_event_id = 1;
    event.resolution.sequence_active = true;
    event.game.status = SimulationStatus::Resolving;

    let mut outcome = base.clone();
    outcome.resolution.next_draw_result_slot_id = 1;
    outcome.resolution.draw_result_slots.insert(
        DrawResultSlotId(0),
        DrawResultSlot {
            outcome: Some(DrawOutcome::Drawn(missing)),
        },
    );
    outcome.resolution.sequence_active = true;
    outcome.game.status = SimulationStatus::Resolving;

    let mut transform = base.clone();
    transform
        .resolution
        .pending_played_self_transforms
        .insert(missing);
    transform.resolution.sequence_active = true;
    transform.game.status = SimulationStatus::Resolving;

    let mut card_program = base;
    card_program
        .entities
        .iter_mut()
        .find_map(|entity| entity.card_runtime.as_mut())
        .unwrap()
        .program = vec![Effect::Sequence(vec![Effect::Destroy {
        targets: Selector::Entity(missing),
    }])];

    for checkpoint in [event, outcome, transform, card_program] {
        assert_that!(
            Simulation::from_checkpoint(checkpoint).map(|_| ()),
            err(matches_pattern!(SimulationError::Checkpoint(
                contains_substring("missing logical entity")
            ))),
        );
    }
}

#[googletest::test]
fn checkpoints_reject_invalid_versions_rng_entities_zones_and_enchantments() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let mut malformed = Vec::new();

    let mut schema = base.clone();
    schema.schema_version += 1;
    malformed.push(schema);

    let mut revision = base.clone();
    revision.ruleset.rulebook_revision += 1;
    malformed.push(revision);

    let mut rng = base.clone();
    rng.rng.algorithm_version = crate::RNG_ALGORITHM_VERSION + 1;
    malformed.push(rng);

    let mut duplicate = base.clone();
    duplicate.entities.push(duplicate.entities[0].clone());
    malformed.push(duplicate);

    let mut counter = base.clone();
    counter.next_game_entity_id = counter.entities.last().unwrap().id.0;
    malformed.push(counter);

    let mut controller = base.clone();
    let zoned = controller
        .entities
        .iter_mut()
        .find(|entity| entity.zone.is_some())
        .unwrap();
    zoned.controller = None;
    malformed.push(controller);

    let mut position = base.clone();
    let zoned = position
        .entities
        .iter_mut()
        .find(|entity| entity.zone.is_some())
        .unwrap();
    zoned.zone_position = None;
    malformed.push(position);

    let mut enchantment = base;
    enchantment.entities[0].enchantments = Some(Enchantments(vec![GameEntityId(u64::MAX)]));
    malformed.push(enchantment);

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
fn checkpoints_reject_inconsistent_effective_costs() {
    let simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Invalid cost", 3, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let mut checkpoint = simulation.checkpoint().unwrap();
    checkpoint
        .entities
        .iter_mut()
        .find(|entity| entity.zone == Some(Zone::Hand))
        .unwrap()
        .card_runtime
        .as_mut()
        .unwrap()
        .cost = 0;

    assert_that!(
        matches!(
            Simulation::from_checkpoint(checkpoint),
            Err(SimulationError::Checkpoint(_))
        ),
        is_true()
    );
}

#[googletest::test]
fn checkpoints_reject_cost_modifiers_without_play_order() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Missing order", 3, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachCostModifier {
            targets: Selector::Entity(card),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -1,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();
    let mut checkpoint = simulation.checkpoint().unwrap();
    checkpoint
        .entities
        .iter_mut()
        .find(|entity| entity.cost_modifier.is_some())
        .unwrap()
        .play_order = None;

    assert_that!(
        matches!(
            Simulation::from_checkpoint(checkpoint),
            Err(SimulationError::Checkpoint(reason)) if reason.contains("lacks play order")
        ),
        is_true()
    );
}

#[googletest::test]
fn checkpoints_reject_stale_play_order_counters() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Ordered cost", 3, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachCostModifier {
            targets: Selector::Entity(card),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -1,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();
    let checkpoint = simulation.checkpoint().unwrap();
    let modifier = checkpoint
        .entities
        .iter()
        .find(|entity| entity.cost_modifier.is_some())
        .unwrap();
    let modifier_id = modifier.id;
    let modifier_order = modifier.play_order.unwrap();
    let target_id = modifier.attached_to.unwrap();

    let mut detached_modifier = checkpoint.clone();
    detached_modifier
        .entities
        .iter_mut()
        .find(|entity| entity.id == modifier_id)
        .unwrap()
        .attached_to = None;
    let target = detached_modifier
        .entities
        .iter_mut()
        .find(|entity| entity.id == target_id)
        .unwrap();
    let runtime = target.card_runtime.as_mut().unwrap();
    runtime.cost = runtime.base_cost;
    detached_modifier.next_play_order = modifier_order;

    assert_that!(
        matches!(
            Simulation::from_checkpoint(detached_modifier),
            Err(SimulationError::Checkpoint(_))
        ),
        is_true()
    );
}

#[googletest::test]
fn checkpoints_reject_stale_non_cost_play_order_counters() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Played later", 0, 1, 1)]),
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
    let mut checkpoint = simulation.checkpoint().unwrap();
    checkpoint.next_play_order = checkpoint
        .entities
        .iter()
        .find(|entity| entity.id == card)
        .unwrap()
        .play_order
        .unwrap();

    assert_that!(
        matches!(
            Simulation::from_checkpoint(checkpoint),
            Err(SimulationError::Checkpoint(_))
        ),
        is_true()
    );
}

#[googletest::test]
fn checkpoints_reject_missing_or_duplicate_active_heroes_and_powers() {
    let simulation = simulation();
    let base = simulation.checkpoint().unwrap();
    let mut malformed = Vec::new();

    let mut missing_player = base.clone();
    let index = missing_player
        .entities
        .iter()
        .position(|entity| {
            entity
                .player
                .as_ref()
                .is_some_and(|player| player.id == PlayerId::One)
        })
        .unwrap();
    missing_player.entities.remove(index);
    malformed.push(missing_player);

    let mut invalid_player = base.clone();
    let player = invalid_player
        .entities
        .iter_mut()
        .find(|entity| {
            entity
                .player
                .as_ref()
                .is_some_and(|player| player.id == PlayerId::One)
        })
        .unwrap();
    player.kind = Some(EntityKind::Minion);
    malformed.push(invalid_player);

    for kind in [EntityKind::Hero, EntityKind::HeroPower] {
        let mut missing = base.clone();
        let index = missing
            .entities
            .iter()
            .position(|entity| {
                entity.kind == Some(kind)
                    && entity.controller == Some(PlayerId::One)
                    && entity.zone == Some(Zone::Play)
            })
            .unwrap();
        missing.entities.remove(index);
        malformed.push(missing);

        let mut duplicate = base.clone();
        let mut entity = duplicate
            .entities
            .iter()
            .find(|entity| {
                entity.kind == Some(kind)
                    && entity.controller == Some(PlayerId::One)
                    && entity.zone == Some(Zone::Play)
            })
            .unwrap()
            .clone();
        entity.id = GameEntityId(duplicate.next_game_entity_id);
        duplicate.next_game_entity_id += 1;
        duplicate.entities.push(entity);
        malformed.push(duplicate);
    }

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
fn checkpoints_reject_dangling_aura_provider_references() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    checkpoint.entities[0].attack_aura_cache = Some(AttackAuraCache(vec![AuraApplication {
        provider: GameEntityId(u64::MAX),
        definition_index: 0,
        modifier: AuraModifier::Attack(1),
    }]));

    assert_that!(
        matches!(
            Simulation::from_checkpoint(checkpoint),
            Err(SimulationError::Checkpoint(reason)) if reason.contains("missing aura provider")
        ),
        is_true()
    );
}

#[googletest::test]
fn pending_choice_programs_are_validated_during_restoration() {
    let mut simulation = simulation();
    let missing = NativeEffectId::new("missing:pending_choice");
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [ResolutionOp::RequestChoice(ChoiceRequest {
            id: ChoiceId(30),
            player: PlayerId::One,
            options: vec![ChoiceOption {
                id: ChoiceId(31),
                operations: vec![ResolutionOp::RunEffect {
                    context: EffectContext {
                        source: None,
                        controller: PlayerId::One,
                        declared_target: None,
                        drawn_card: None,
                        origin: EffectOrigin::Other,
                    },
                    effect: Effect::Native(missing.clone()),
                    event: None,
                }],
            }],
        })],
    );
    drive_resolution(world).unwrap();

    assert_that!(
        matches!(
            Simulation::from_checkpoint(simulation.checkpoint().unwrap()),
            Err(SimulationError::NativeEffectNotRegistered(id)) if id == missing
        ),
        is_true()
    );
}

#[googletest::test]
fn played_self_finish_barrier_programs_are_validated_during_restoration() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    let source = checkpoint.entities[0].id;
    let missing = NativeEffectId::new("missing:played_self_finish");
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(0),
        operation: ResolutionOp::FinishPlayedSelfTransform {
            subject: source,
            original_after_play: vec![crate::TriggerSeed {
                source,
                definition_index: 0,
                definition: crate::TriggerDefinition {
                    event: EventKind::AfterPlay,
                    eligible_zones: vec![Zone::Play],
                    conditions: Vec::new(),
                    source_eligibility: crate::SourceEligibilityPolicy::RememberedSource,
                    priority: 0,
                    wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
                    effect_program: vec![Effect::Native(missing.clone())],
                },
                controller: PlayerId::One,
                zone: Zone::Play,
                play_order: 0,
            }],
        },
    });
    checkpoint.resolution.next_resolution_id = 1;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    assert_that!(
        Simulation::from_checkpoint(checkpoint).map(|_| ()),
        err(eq(&SimulationError::NativeEffectNotRegistered(missing)))
    );
}

#[googletest::test]
fn retained_event_and_operation_programs_are_validated_during_restoration() {
    let simulation = simulation();
    let mut checkpoint = simulation.checkpoint().unwrap();
    let source = checkpoint.entities[0].id;
    let definition = crate::TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: Vec::new(),
        source_eligibility: crate::SourceEligibilityPolicy::RememberedSource,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
        effect_program: vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 1,
            temporary: true,
        }],
    };
    let seed = crate::TriggerSeed {
        source,
        definition_index: 0,
        definition: definition.clone(),
        controller: PlayerId::One,
        zone: Zone::Play,
        play_order: 0,
    };
    let candidate = crate::TriggerCandidate {
        source,
        event: EventId(1),
        definition_index: 0,
        definition,
        controller: PlayerId::One,
        order: crate::TriggerOrderKey {
            player_bucket: 0,
            zone_bucket: 0,
            priority: 0,
            play_order: 0,
            source,
            tie_breaker: 0,
        },
    };
    checkpoint.resolution.events.insert(
        EventId(1),
        PreparedEvent {
            context: EventContext {
                kind: EventKind::Damage,
                source: Some(source),
                targets: vec![source],
                controller: PlayerId::One,
                proposed_value: Some(1),
                actual_value: Some(1),
                simultaneous_ordinal: 0,
            },
            prechecked_triggers: Some(vec![seed]),
            candidates: Some(vec![candidate.clone()]),
        },
    );
    checkpoint.resolution.stack = vec![
        StackedResolutionOp {
            id: ResolutionId(1),
            operation: ResolutionOp::RunEffect {
                context: EffectContext {
                    source: Some(source),
                    controller: PlayerId::One,
                    declared_target: Some(source),
                    drawn_card: None,
                    origin: EffectOrigin::Other,
                },
                effect: Effect::GainResource {
                    player: PlayerSelector::Controller,
                    amount: 1,
                    temporary: true,
                },
                event: Some(EventId(1)),
            },
        },
        StackedResolutionOp {
            id: ResolutionId(2),
            operation: ResolutionOp::AttemptTrigger(candidate),
        },
        StackedResolutionOp {
            id: ResolutionId(3),
            operation: ResolutionOp::RequestChoice(ChoiceRequest {
                id: ChoiceId(40),
                player: PlayerId::One,
                options: vec![ChoiceOption {
                    id: ChoiceId(41),
                    operations: vec![ResolutionOp::CheckOutcome],
                }],
            }),
        },
    ];
    checkpoint.resolution.next_resolution_id = 4;
    checkpoint.resolution.next_event_id = 2;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;

    let restored = Simulation::from_checkpoint(checkpoint.clone()).unwrap();

    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
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

#[googletest::test]
fn checkpoints_reject_missing_transform_and_copy_references() {
    let original = simulation();
    let base = original.checkpoint().unwrap();
    let valid = base.entities[0].id;
    let missing = GameEntityId(u64::MAX);
    let definition = crate::TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: Vec::new(),
        source_eligibility: crate::SourceEligibilityPolicy::RememberedSource,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
        effect_program: Vec::new(),
    };
    let mut malformed = Vec::new();

    for operation in [
        ResolutionOp::TransformEntity {
            target: missing,
            source: Some(valid),
            card: Card::minion("Retained transform", 0, 1, 1),
            kind: crate::TransformKind::Spell,
        },
        ResolutionOp::CopyEntity(CopyRequest {
            source: missing,
            controller: PlayerId::One,
            destination: Zone::Hand,
            board_index: None,
            policy: CopyStatePolicy::CurrentForm,
        }),
    ] {
        let mut checkpoint = base.clone();
        checkpoint.resolution.stack.push(StackedResolutionOp {
            id: ResolutionId(0),
            operation,
        });
        checkpoint.resolution.next_resolution_id = 1;
        checkpoint.resolution.sequence_active = true;
        checkpoint.game.status = SimulationStatus::Resolving;
        malformed.push(checkpoint);
    }

    let mut seed = base.clone();
    seed.resolution.events.insert(
        EventId(0),
        PreparedEvent {
            context: EventContext {
                kind: EventKind::Damage,
                source: Some(valid),
                targets: vec![valid],
                controller: PlayerId::One,
                proposed_value: Some(1),
                actual_value: Some(1),
                simultaneous_ordinal: 0,
            },
            prechecked_triggers: Some(vec![crate::TriggerSeed {
                source: missing,
                definition_index: 0,
                definition: definition.clone(),
                controller: PlayerId::One,
                zone: Zone::Play,
                play_order: 0,
            }]),
            candidates: None,
        },
    );
    seed.resolution.next_event_id = 1;
    seed.resolution.sequence_active = true;
    seed.game.status = SimulationStatus::Resolving;
    malformed.push(seed);

    let mut candidate = base;
    candidate.resolution.events.insert(
        EventId(0),
        PreparedEvent {
            context: EventContext {
                kind: EventKind::Damage,
                source: Some(valid),
                targets: vec![valid],
                controller: PlayerId::One,
                proposed_value: Some(1),
                actual_value: Some(1),
                simultaneous_ordinal: 0,
            },
            prechecked_triggers: None,
            candidates: Some(vec![crate::TriggerCandidate {
                source: valid,
                event: EventId(0),
                definition_index: 0,
                definition,
                controller: PlayerId::One,
                order: crate::TriggerOrderKey {
                    player_bucket: 0,
                    zone_bucket: 0,
                    priority: 0,
                    play_order: 0,
                    source: missing,
                    tie_breaker: 0,
                },
            }]),
        },
    );
    candidate.resolution.next_event_id = 1;
    candidate.resolution.sequence_active = true;
    candidate.game.status = SimulationStatus::Resolving;
    malformed.push(candidate);

    let mut copied = simulation();
    let source = spawn_card(
        copied.app.world_mut(),
        PlayerId::One,
        Card::minion("Copy source", 0, 2, 2),
        Zone::Play,
    )
    .unwrap();
    attach_stat_modifier(
        copied.app.world_mut(),
        PlayerId::One,
        source,
        StatModifier {
            attack: 1,
            health: 1,
            silence_removable: false,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    copy_entity(
        copied.app.world_mut(),
        CopyRequest {
            source,
            controller: PlayerId::Two,
            destination: Zone::Play,
            board_index: None,
            policy: CopyStatePolicy::InPlayState,
        },
    )
    .unwrap();
    let copy = copied
        .trace()
        .iter()
        .find_map(|entry| match entry {
            TraceEntry::EntityCopied { copy, .. } => Some(copy),
            _ => None,
        })
        .copied()
        .unwrap();
    let copied_checkpoint = copied.checkpoint().unwrap();
    let attachment_id = copied_checkpoint
        .entities
        .iter()
        .find(|entity| entity.attached_to == Some(copy))
        .unwrap()
        .id;

    let mut missing_relationship = copied_checkpoint.clone();
    missing_relationship
        .entities
        .iter_mut()
        .find(|entity| entity.id == attachment_id)
        .unwrap()
        .attached_to = None;
    malformed.push(missing_relationship);

    let mut missing_duration = copied_checkpoint.clone();
    missing_duration
        .entities
        .iter_mut()
        .find(|entity| entity.id == attachment_id)
        .unwrap()
        .enchantment_duration = None;
    malformed.push(missing_duration);

    let mut missing_play_order = copied_checkpoint.clone();
    missing_play_order
        .entities
        .iter_mut()
        .find(|entity| entity.id == attachment_id)
        .unwrap()
        .play_order = None;
    malformed.push(missing_play_order);

    let mut missing_identity = copied_checkpoint.clone();
    missing_identity
        .entities
        .iter_mut()
        .find(|entity| entity.id == attachment_id)
        .unwrap()
        .definition_id = None;
    malformed.push(missing_identity);

    let mut missing_payload = copied_checkpoint;
    let attachment = missing_payload
        .entities
        .iter_mut()
        .find(|entity| entity.id == attachment_id)
        .unwrap();
    attachment.stat_modifier = None;
    attachment.keyword_modifier = None;
    attachment.cost_modifier = None;
    attachment.runtime_triggers = None;
    attachment.runtime_continuous_effects = None;
    malformed.push(missing_payload);

    for checkpoint in malformed {
        assert_that!(
            Simulation::from_checkpoint(checkpoint).map(|_| ()),
            err(matches_pattern!(SimulationError::Checkpoint(anything())))
        );
    }
}

#[googletest::test]
fn checkpoint_fork_executes_retained_transform_equivalently() {
    let mut original = simulation();
    let target = hand_card(&mut original, PlayerId::One);
    begin_sequence(original.app.world_mut()).unwrap();
    original.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        original.app.world_mut(),
        [ResolutionOp::TransformEntity {
            target,
            source: None,
            card: Card::minion("Checkpoint replacement", 2, 4, 5),
            kind: crate::TransformKind::Spell,
        }],
    );
    let json = original.checkpoint().unwrap().to_json().unwrap();
    let mut restored =
        Simulation::from_checkpoint(SimulationCheckpoint::from_json(&json).unwrap()).unwrap();
    let mut fork = restored.fork().unwrap();

    drive_resolution(restored.app.world_mut()).unwrap();
    drive_resolution(fork.app.world_mut()).unwrap();
    finish_sequence(restored.app.world_mut());
    finish_sequence(fork.app.world_mut());
    restored.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
    fork.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;

    assert_that!(restored.snapshot(), eq(&fork.snapshot()));
    assert_that!(restored.trace(), eq(fork.trace()));
    assert_that!(
        assert_resolution_invariants(restored.app.world()),
        ok(anything())
    );
}

#[googletest::test]
fn checkpoint_fork_executes_retained_play_copy_with_cloned_enchantments_equivalently() {
    let mut original = simulation();
    let source = spawn_card(
        original.app.world_mut(),
        PlayerId::One,
        Card::minion("Checkpoint copy source", 1, 3, 4),
        Zone::Play,
    )
    .unwrap();
    attach_stat_modifier(
        original.app.world_mut(),
        PlayerId::One,
        source,
        StatModifier {
            attack: 2,
            health: 1,
            silence_removable: true,
        },
        EnchantmentDuration::EndOfTurn(PlayerId::One),
    )
    .unwrap();
    begin_sequence(original.app.world_mut()).unwrap();
    original.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        original.app.world_mut(),
        [ResolutionOp::CopyEntity(CopyRequest {
            source,
            controller: PlayerId::Two,
            destination: Zone::Play,
            board_index: Some(0),
            policy: CopyStatePolicy::InPlayState,
        })],
    );
    let json = original.checkpoint().unwrap().to_json().unwrap();
    let mut restored =
        Simulation::from_checkpoint(SimulationCheckpoint::from_json(&json).unwrap()).unwrap();
    let mut fork = restored.fork().unwrap();

    drive_resolution(restored.app.world_mut()).unwrap();
    drive_resolution(fork.app.world_mut()).unwrap();
    finish_sequence(restored.app.world_mut());
    finish_sequence(fork.app.world_mut());
    restored.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;
    fork.app.world_mut().resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;

    assert_that!(restored.snapshot(), eq(&fork.snapshot()));
    assert_that!(restored.trace(), eq(fork.trace()));
    assert_that!(
        restored.checkpoint().unwrap(),
        eq(&fork.checkpoint().unwrap())
    );
    assert_that!(
        assert_resolution_invariants(restored.app.world()),
        ok(anything())
    );
}
