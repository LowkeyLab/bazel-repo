use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    EnchantmentDuration, HealthAuraCache, KeepEnchantments, KeywordModifier, OtherAuraCache,
    Player, SilenceRemovable,
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
        transform_entity(world, GameEntityId(999), Card::minion("Missing", 0, 1, 1)),
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
    let enchantment = world
        .iter_entities()
        .find_map(|entity| {
            entity
                .contains::<StatModifier>()
                .then(|| entity.get::<GameEntityId>().copied())
                .flatten()
        })
        .unwrap();
    let target_entity = game_entity(world, target).unwrap();
    world.entity_mut(target_entity).insert((
        Armor(4),
        PendingDestroy,
        Abilities(vec!["Battlecry".to_string()]),
        Enchantments(vec![enchantment]),
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
    let enchantment_entity = game_entity(world, enchantment).unwrap();
    world.entity_mut(enchantment_entity).insert((
        KeywordModifier {
            keyword: Keyword::Taunt,
            granted: true,
            silence_removable: true,
        },
        EnchantmentDuration::EndOfTurn(PlayerId::One),
        SilenceRemovable,
    ));

    let checkpoint = original.checkpoint().unwrap();
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
    let restored_enchantment = game_entity(restored.app.world(), enchantment).unwrap();
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
