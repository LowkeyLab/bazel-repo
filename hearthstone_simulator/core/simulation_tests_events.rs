use googletest::prelude::*;

use super::{test_support::*, *};

#[googletest::test]
fn lethal_hero_state_is_irreversible_before_simultaneous_deathrattle_healing() {
    let healer = Card::minion("Last Gasp Healer", 0, 1, 1).with_deathrattle(vec![Effect::Heal {
        targets: Selector::FriendlyCharacters,
        amount: ValueExpression::Constant(30),
    }]);
    let lethal = Card::spell("Lethal Friendly Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::FriendlyCharacters,
        amount: ValueExpression::Constant(30),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![healer, lethal]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let healer = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: healer,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let lethal = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: lethal,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let snapshot = simulation.snapshot();
    assert_that!(snapshot.players[0].health, eq(30));
    assert_that!(
        snapshot.game.outcome,
        eq(Some(GameOutcome::Winner(PlayerId::Two)))
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::HeroDefeated {
                controller: PlayerId::One,
                ..
            }
        )),
        is_true()
    );
}

#[googletest::test]
fn simultaneous_deaths_use_global_play_order_and_cache_the_turn() {
    let blast = Card::spell("Cross Controller Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Newer One", 0, 1, 1), blast]),
        PlayerConfig::new("Rexxar", vec![Card::minion("Older Two", 0, 1, 1)]),
    ]);
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    let older = hand_card(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::Two,
            card: older,
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
    let newer = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: newer,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let blast = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: blast,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let deaths = simulation.snapshot().deaths;
    assert_that!(
        deaths
            .iter()
            .map(|record| record.entity)
            .collect::<Vec<_>>(),
        eq(&vec![older, newer])
    );
    assert_that!(
        deaths
            .iter()
            .map(|record| record.turn_of_death)
            .collect::<Vec<_>>(),
        eq(&vec![3, 3])
    );
}

#[googletest::test]
fn death_event_trigger_queues_are_frozen_before_the_batch_resolves() {
    let late_observer =
        Card::minion("Late Observer", 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        }]);
    let summoner =
        Card::minion("Observer Summoner", 0, 1, 1).with_deathrattle(vec![Effect::Summon {
            player: PlayerSelector::Controller,
            card: late_observer,
            board_index: None,
        }]);
    let blast = Card::spell("Frozen Batch Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![summoner, Card::minion("Second Death", 0, 1, 1), blast],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let summoner = hand_card(&mut simulation, PlayerId::One);
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
        simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::TriggerResolved { source, .. } => Some(*source),
                _ => None,
            })
            .collect::<Vec<_>>(),
        eq(&vec![summoner])
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::FrameBegin { kind, .. } if kind == "EventBatch"
        )),
        is_true()
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::FrameBegin { kind, .. } if kind == "EventQueue"
        )),
        is_true()
    );
}

#[googletest::test]
fn aborted_death_event_entries_do_not_block_queue_completion() {
    let mut world = World::new();
    world.init_resource::<GameEntityIndex>();
    let queue = world
        .spawn((ResolutionQueue(QueueKind::Events), QueueState::Collecting))
        .id();
    let entry = add_event_entry(
        &mut world,
        queue,
        QueuedEvent {
            event: crate::ResolutionId(1),
            event_entity: Entity::PLACEHOLDER,
            order: EventOrderKey {
                player_bucket: 0,
                ordinal: 0,
                tie_breaker: 0,
            },
        },
    )
    .expect("collecting queue should accept the event");
    world.entity_mut(entry).insert(QueuedTrigger {
        source: GameEntityId(u64::MAX),
        event: crate::ResolutionId(1),
        event_entity: Entity::PLACEHOLDER,
        definition_index: 0,
        order: crate::TriggerOrderKey {
            player_bucket: 0,
            zone_bucket: 0,
            priority: 0,
            play_order: 0,
            source: GameEntityId(u64::MAX),
            tie_breaker: 0,
        },
    });
    freeze_queue(&mut world, queue).expect("event queue should freeze");

    resolve_prepared_events(&mut world, queue)
        .expect("an aborted entry should not fail the Death Event queue");

    assert_that!(
        world.get::<QueueState>(queue),
        eq(Some(&QueueState::Complete))
    );
    assert_that!(
        world.get::<crate::QueueEntryStatus>(entry),
        eq(Some(&crate::QueueEntryStatus::Aborted))
    );
}

#[googletest::test]
fn deathrattles_and_board_observers_mingle_by_play_order() {
    let deathrattle = Card::minion("Older Deathrattle", 0, 1, 1).with_deathrattle(Vec::new());
    let observer =
        Card::minion("Newer Observer", 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: Vec::new(),
        }]);
    let blast = Card::spell("Mingle Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![deathrattle, observer, blast]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let older = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: older,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let newer = hand_card(&mut simulation, PlayerId::One);
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

    assert_that!(
        simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::TriggerResolved { source, .. } => Some(*source),
                _ => None,
            })
            .collect::<Vec<_>>(),
        eq(&vec![older, newer])
    );
}

#[googletest::test]
fn deathrattles_resolve_in_chained_death_phases() {
    let bomber =
        Card::minion("Deathrattle Bomber", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::AllMinions,
            amount: ValueExpression::Constant(1),
        }]);
    let blast = Card::spell("Chain Starter", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![bomber, Card::minion("Chain Target", 0, 1, 2), blast],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let bomber_id = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: bomber_id,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let target_id = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: target_id,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let blast_id = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: blast_id,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let snapshot = simulation.snapshot();
    assert_that!(
        snapshot
            .deaths
            .iter()
            .map(|record| record.entity)
            .collect::<Vec<_>>(),
        eq(&vec![bomber_id, target_id])
    );
    assert_that!(
        snapshot
            .deaths
            .iter()
            .map(|record| record.remembered_zone_position)
            .collect::<Vec<_>>(),
        eq(&vec![1, 1])
    );
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == bomber_id)
            .unwrap()
            .zone,
        eq(Zone::Graveyard)
    );
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == target_id)
            .unwrap()
            .zone,
        eq(Zone::Graveyard)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::DeathPhaseQueued { .. }))
            .count(),
        eq(2)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(
                entry,
                TraceEntry::EventCreated {
                    kind: EventKind::Death,
                    ..
                }
            ))
            .count(),
        eq(2)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
            .count(),
        eq(1)
    );
    simulation.assert_invariants().unwrap();
}

#[googletest::test]
fn damage_events_freeze_and_resolve_trigger_effects_depth_first() {
    let reactive =
        Card::minion("Reactive", 0, 1, 4).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: vec![crate::TimedCondition {
                timing: crate::ConditionTiming::QueueTime,
                condition: crate::TriggerCondition::EventValueAtLeast(1),
            }],
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            allow_repeated_event: false,
            allow_direct_self_nesting: false,
            wounded_target_policy: crate::WoundedTargetPolicy::ExcludeMortallyWounded,
            effect_program: vec![Effect::DealDamage {
                targets: Selector::EnemyMinions,
                amount: ValueExpression::Constant(1),
            }],
        }]);
    let blast = Card::spell("Trigger Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![reactive, blast]),
        PlayerConfig::new("Rexxar", vec![Card::minion("Target", 0, 0, 5)]),
    ]);
    let reactive_id = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: reactive_id,
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
    let target = hand_card(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::Two,
            card: target,
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
    let blast = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: blast,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let snapshot = simulation.snapshot();
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == reactive_id)
            .unwrap()
            .damage,
        eq(1)
    );
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == target)
            .unwrap()
            .damage,
        eq(3)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
            .count(),
        eq(2)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerAborted { .. }))
            .count(),
        eq(2)
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::QueueFrozen { entries, .. } if !entries.is_empty()
        )),
        is_true()
    );
    let frozen_ids = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::QueueFrozen { entries, .. } => Some(entries.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_that!(
        simulation.trace().iter().all(|entry| match entry {
            TraceEntry::TriggerResolved { id, .. } => frozen_ids.contains(id),
            _ => true,
        }),
        is_true()
    );
    simulation.assert_invariants().unwrap();
}

#[googletest::test]
fn resolution_time_conditions_abort_frozen_trigger_entries() {
    let reactive = Card::minion("Resolution-Time Observer", 0, 1, 4).with_triggers(vec![
        crate::TriggerDefinition {
            event: EventKind::Damage,
            eligible_zones: vec![Zone::Play],
            conditions: vec![crate::TimedCondition {
                timing: crate::ConditionTiming::ResolutionTime,
                condition: crate::TriggerCondition::ControllerIs(PlayerId::Two),
            }],
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
        },
    ]);
    let bolt = Card::spell("Resolution-Time Bolt", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![reactive, bolt]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
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
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(0)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerAborted { .. }))
            .count(),
        eq(1)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .any(|entry| matches!(entry, TraceEntry::TriggerResolved { .. })),
        is_false()
    );
}
