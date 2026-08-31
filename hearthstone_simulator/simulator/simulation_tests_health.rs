use googletest::prelude::*;

use super::{test_support::*, *};

#[googletest::test]
fn area_damage_removes_deaths_together_at_the_phase_boundary() {
    let blast = Card::spell("Synthetic Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(2),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("First", 0, 1, 2),
                Card::minion("Second", 0, 1, 2),
                blast,
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..3 {
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .expect("synthetic card should resolve");
    }

    let snapshot = simulation.snapshot();
    let living_minions = snapshot.players[0]
        .board
        .iter()
        .filter(|id| {
            snapshot
                .objects
                .iter()
                .any(|object| object.id == **id && object.kind == EntityKind::Minion)
        })
        .count();
    let deaths = simulation
        .trace()
        .iter()
        .filter(|entry| matches!(entry, TraceEntry::EntityDied { .. }))
        .count();

    assert_that!(living_minions, eq(0));
    assert_that!(deaths, eq(2));
    simulation
        .assert_invariants()
        .expect("invariants should hold");
}

#[googletest::test]
fn proposed_damage_modifiers_apply_in_program_order_and_can_prevent_damage() {
    let modifier =
        Card::minion("Ordered Modifier", 0, 1, 12).with_triggers(vec![self_event_trigger(
            EventKind::ProposedDamage,
            vec![
                Effect::ModifyEventValue {
                    operation: EventValueOperation::Replace,
                    value: ValueExpression::Constant(2),
                },
                Effect::ModifyEventValue {
                    operation: EventValueOperation::Add,
                    value: ValueExpression::Constant(1),
                },
                Effect::ModifyEventValue {
                    operation: EventValueOperation::Multiply,
                    value: ValueExpression::Constant(3),
                },
            ],
        )]);
    let preventer = Card::minion("Preventer", 0, 1, 12).with_triggers(vec![self_event_trigger(
        EventKind::ProposedDamage,
        vec![Effect::ModifyEventValue {
            operation: EventValueOperation::Replace,
            value: ValueExpression::Constant(0),
        }],
    )]);
    let blast = Card::spell("Modifiable Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(5),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![modifier, preventer, blast]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let modifier = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: modifier,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let preventer = hand_card(&mut simulation, PlayerId::One);
    for _ in 0..2 {
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
    }

    let snapshot = simulation.snapshot();
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == modifier)
            .unwrap()
            .damage,
        eq(9)
    );
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == preventer)
            .unwrap()
            .damage,
        eq(0)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::EventValueChanged {
                    operation, current, ..
                } => Some((*operation, *current)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        eq(&vec![
            (EventValueOperation::Replace, 2),
            (EventValueOperation::Add, 3),
            (EventValueOperation::Multiply, 9),
            (EventValueOperation::Replace, 0),
        ])
    );
}

#[googletest::test]
fn damage_protection_precedes_predamage_triggers_and_zero_damage_is_not_an_event() {
    let protected = Card::minion("Protected Observer", 0, 1, 4).with_triggers(vec![
        self_event_trigger(
            EventKind::ProposedDamage,
            vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        ),
        self_event_trigger(
            EventKind::Damage,
            vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 10,
                temporary: true,
            }],
        ),
    ]);
    let blast = Card::spell("Protection Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let zero = Card::spell("Zero Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(0),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![protected.clone(), protected.clone(), protected, blast, zero],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let mut minions = Vec::new();
    for _ in 0..3 {
        let card = hand_card(&mut simulation, PlayerId::One);
        minions.push(card);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
    }
    for (target, keyword) in [
        (minions[0], Keyword::Immune),
        (minions[1], Keyword::DivineShield),
    ] {
        let entity = game_entity(simulation.app.world(), target).unwrap();
        simulation
            .app
            .world_mut()
            .get_mut::<Keywords>(entity)
            .unwrap()
            .0
            .insert(keyword);
    }
    for _ in 0..2 {
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
    }

    let snapshot = simulation.snapshot();
    assert_that!(
        minions
            .iter()
            .map(|id| snapshot
                .objects
                .iter()
                .find(|object| object.id == *id)
                .unwrap()
                .damage)
            .collect::<Vec<_>>(),
        eq(&vec![0, 0, 1])
    );
    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(11)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::ProposedDamage,
                    ..
                }
            ))
            .count(),
        eq(1)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::Damage,
                    ..
                }
            ))
            .count(),
        eq(1)
    );
}

#[googletest::test]
fn no_op_healing_does_not_create_healing_reactions() {
    let observer =
        Card::minion("Full Health Observer", 0, 1, 4).with_triggers(vec![self_event_trigger(
            EventKind::Healing,
            vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        )]);
    let healing = Card::spell("No-op Healing", 0).with_effects(vec![Effect::Heal {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(2),
    }]);
    let zero = Card::spell("Zero Healing", 0).with_effects(vec![Effect::Heal {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(0),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![observer, healing, zero]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..3 {
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
    }

    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(0)
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::EventCreated {
                kind: EventKind::Healing,
                ..
            }
        )),
        is_false()
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::Healing {
                    proposed: 0 | 2,
                    actual: 0,
                    ..
                }
            ))
            .count(),
        eq(2)
    );
}

#[googletest::test]
fn each_predamage_event_resolves_after_prior_damage_is_applied() {
    let first = Card::minion("First Damage Target", 0, 1, 5);
    let healer = Card::minion("Predamage Healer", 0, 1, 5).with_triggers(vec![self_event_trigger(
        EventKind::ProposedDamage,
        vec![Effect::Heal {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }],
    )]);
    let blast = Card::spell("Ordered Damage", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(2),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![first, healer, blast]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let mut minions = Vec::new();
    for _ in 0..3 {
        let card = hand_card(&mut simulation, PlayerId::One);
        if minions.len() < 2 {
            minions.push(card);
        }
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
    }

    let snapshot = simulation.snapshot();
    assert_that!(
        minions
            .iter()
            .map(|id| snapshot
                .objects
                .iter()
                .find(|object| object.id == *id)
                .unwrap()
                .damage)
            .collect::<Vec<_>>(),
        eq(&vec![1, 2])
    );
    let first_damage = simulation
        .trace()
        .iter()
        .position(|entry| {
            matches!(
                entry,
                TraceEntry::Damage {
                    target,
                    actual: 2,
                    ..
                } if *target == minions[0]
            )
        })
        .unwrap();
    let intervening_heal = simulation
        .trace()
        .iter()
        .position(|entry| {
            matches!(
                entry,
                TraceEntry::Healing {
                    target,
                    actual: 1,
                    ..
                } if *target == minions[0]
            )
        })
        .unwrap();
    let second_damage = simulation
        .trace()
        .iter()
        .position(|entry| {
            matches!(
                entry,
                TraceEntry::Damage {
                    target,
                    actual: 2,
                    ..
                } if *target == minions[1]
            )
        })
        .unwrap();
    assert_that!(first_damage < intervening_heal, is_true());
    assert_that!(intervening_heal < second_damage, is_true());
}

#[googletest::test]
fn armor_loss_counts_as_actual_damage_for_traces_and_triggers() {
    let observer = Card::minion("Armor Damage Observer", 0, 1, 4).with_triggers(vec![
        crate::TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: vec![crate::TimedCondition {
                timing: crate::ConditionTiming::QueueTime,
                condition: crate::TriggerCondition::EventValueAtLeast(1),
            }],
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        },
    ]);
    let bolt = Card::spell("Armor Bolt", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(2),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![observer, bolt]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..1 {
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
    }
    let target = hero(&mut simulation, PlayerId::Two);
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(target_entity)
        .insert(Armor(3));
    let bolt = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: bolt,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    let target = simulation
        .snapshot()
        .objects
        .into_iter()
        .find(|object| object.id == target)
        .unwrap();
    assert_that!(simulation.snapshot().players[1].armor, eq(1));
    assert_that!(target.damage, eq(0));
    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(1)
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::Damage {
                target: damaged,
                proposed: 2,
                actual: 2,
                ..
            } if *damaged == target.id
        )),
        is_true()
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::EventCreated {
                kind: EventKind::Damage,
                actual: Some(2),
                ..
            }
        )),
        is_true()
    );
}

#[googletest::test]
fn simultaneous_damage_is_applied_before_reactions_and_freezes_later_events() {
    let late_observer =
        Card::minion("Late Observer", 0, 1, 2).with_triggers(vec![self_event_trigger(
            EventKind::Damage,
            vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        )]);
    let observer = Card::minion("Batch Observer", 0, 1, 3).with_triggers(vec![self_event_trigger(
        EventKind::Damage,
        vec![Effect::Sequence(vec![
            Effect::Heal {
                targets: Selector::AllMinions,
                amount: ValueExpression::Constant(1),
            },
            Effect::Summon {
                player: PlayerSelector::Controller,
                card: late_observer,
                board_index: None,
            },
        ])],
    )]);
    let target = Card::minion("Batch Target", 0, 1, 3);
    let blast = Card::spell("Simultaneous Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![observer, target, blast]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let observer = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: observer,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let target = hand_card(&mut simulation, PlayerId::One);
    for _ in 0..2 {
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
    }

    let snapshot = simulation.snapshot();
    for id in [observer, target] {
        assert_that!(
            snapshot
                .objects
                .iter()
                .find(|object| object.id == id)
                .unwrap()
                .damage,
            eq(0)
        );
    }
    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(0)
    );
    let last_damage = simulation
        .trace()
        .iter()
        .rposition(|entry| matches!(entry, TraceEntry::Damage { .. }))
        .unwrap();
    let first_reaction = simulation
        .trace()
        .iter()
        .position(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
        .unwrap();
    assert_that!(last_damage < first_reaction, is_true());
}

#[googletest::test]
fn simultaneous_healing_is_applied_before_healing_reactions() {
    let observer = Card::minion("Healing Observer", 0, 1, 4).with_triggers(vec![
        self_event_trigger(
            EventKind::ProposedHealing,
            vec![Effect::ModifyEventValue {
                operation: EventValueOperation::Multiply,
                value: ValueExpression::Constant(2),
            }],
        ),
        self_event_trigger(EventKind::Healing, Vec::new()),
    ]);
    let target = Card::minion("Healing Target", 0, 1, 4);
    let blast = Card::spell("Setup Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(2),
    }]);
    let healing = Card::spell("Simultaneous Healing", 0).with_effects(vec![Effect::Heal {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![observer, target, blast, healing]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..4 {
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
    }

    let trace = simulation.trace();
    let healing_entries = trace
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| matches!(entry, TraceEntry::Healing { .. }).then_some(index))
        .collect::<Vec<_>>();
    let healing_reaction = trace
        .iter()
        .rposition(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
        .unwrap();
    assert_that!(healing_entries.len(), eq(2));
    assert_that!(
        healing_entries
            .iter()
            .all(|index| *index < healing_reaction),
        is_true()
    );
    assert_that!(
        trace
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::ProposedHealing,
                    ..
                }
            ))
            .count(),
        eq(2)
    );
    let snapshot = simulation.snapshot();
    let minion_damage = snapshot
        .objects
        .iter()
        .filter(|object| object.kind == EntityKind::Minion)
        .map(|object| object.damage)
        .collect::<Vec<_>>();
    assert_that!(minion_damage, eq(&vec![0, 1]));
}

#[googletest::test]
fn event_value_modifiers_are_rejected_outside_proposed_event_triggers() {
    let invalid = Card::spell("Invalid Modifier", 0).with_effects(vec![Effect::ModifyEventValue {
        operation: EventValueOperation::Replace,
        value: ValueExpression::Constant(0),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![invalid]),
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
fn damage_handles_missing_targets_immunity_shields_armor_and_negative_values() {
    let mut simulation = simulation();
    let target = hero(&mut simulation, PlayerId::Two);
    let entity = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        apply_damage(simulation.app.world_mut(), None, GameEntityId(99), 1),
        err(eq(&SimulationError::EntityNotFound(GameEntityId(99))))
    );

    simulation
        .app
        .world_mut()
        .get_mut::<Keywords>(entity)
        .unwrap()
        .0
        .insert(Keyword::Immune);
    apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
    assert_that!(
        simulation.app.world().get::<Damage>(entity),
        eq(Some(&Damage(0)))
    );
    simulation
        .app
        .world_mut()
        .get_mut::<Keywords>(entity)
        .unwrap()
        .0
        .remove(&Keyword::Immune);
    simulation
        .app
        .world_mut()
        .get_mut::<Keywords>(entity)
        .unwrap()
        .0
        .insert(Keyword::DivineShield);
    apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<Keywords>(entity)
            .unwrap()
            .0
            .contains(&Keyword::DivineShield),
        is_false()
    );
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(Armor(3));
    apply_damage(simulation.app.world_mut(), None, target, 5).unwrap();
    assert_that!(
        simulation.app.world().get::<Armor>(entity),
        eq(Some(&Armor(0)))
    );
    assert_that!(
        simulation.app.world().get::<Damage>(entity),
        eq(Some(&Damage(2)))
    );
    apply_damage(simulation.app.world_mut(), None, target, -5).unwrap();
    assert_that!(
        simulation.app.world().get::<Damage>(entity),
        eq(Some(&Damage(2)))
    );

    apply_damage_batch(
        simulation.app.world_mut(),
        Vec::new(),
        SimultaneousEventOrder::Given,
    )
    .unwrap();
    apply_healing_batch(
        simulation.app.world_mut(),
        Vec::new(),
        SimultaneousEventOrder::Given,
    )
    .unwrap();
    expand_damage_batch(simulation.app.world_mut(), Vec::new());
    expand_healing_batch(simulation.app.world_mut(), Vec::new());
    let player_entity = simulation.snapshot().players[0].entity;
    apply_healing_batch(
        simulation.app.world_mut(),
        vec![HealingRequest {
            source: None,
            target: player_entity,
            proposed: 1,
        }],
        SimultaneousEventOrder::Given,
    )
    .unwrap();
    apply_healing_batch(
        simulation.app.world_mut(),
        vec![HealingRequest {
            source: None,
            target,
            proposed: 1,
        }],
        SimultaneousEventOrder::Given,
    )
    .unwrap();
    assert_that!(
        simulation.app.world().get::<Damage>(entity),
        eq(Some(&Damage(1)))
    );
    apply_damage(simulation.app.world_mut(), None, target, i32::MAX).unwrap();
    assert_that!(
        simulation.app.world().get::<Damage>(entity),
        eq(Some(&Damage(i32::MAX)))
    );
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: Some(target),
        origin: EffectOrigin::Other,
    };
    assert_that!(
        modify_active_event_value(
            simulation.app.world_mut(),
            &context,
            EventValueOperation::Replace,
            ValueExpression::Constant(0),
        ),
        err(eq(&SimulationError::NoModifiableEventValue))
    );
}
