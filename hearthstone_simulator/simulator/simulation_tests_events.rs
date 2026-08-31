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
fn outcome_waits_for_deathrattles_that_defeat_the_other_hero() {
    let mutual_destruction =
        Card::minion("Mutual Destruction", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::EnemyCharacters,
            amount: ValueExpression::Constant(30),
        }]);
    let lethal = Card::spell("Lethal Friendly Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::FriendlyCharacters,
        amount: ValueExpression::Constant(30),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![mutual_destruction, lethal]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
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
        simulation.snapshot().game.outcome,
        eq(Some(GameOutcome::Draw))
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::HeroDefeated { .. }))
            .count(),
        eq(2)
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
fn death_triggers_group_by_dominant_player_before_priority() {
    fn observer(name: &str, priority: i16) -> Card {
        Card::minion(name, 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority,
            wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: Vec::new(),
        }])
    }

    let blast = Card::spell("Dominant Grouping Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::with_seed_and_dominant_player(
        [
            PlayerConfig::new(
                "Jaina",
                vec![
                    Card::minion("Victim", 0, 1, 1),
                    observer("Dominant", 100),
                    blast,
                ],
            ),
            PlayerConfig::new("Rexxar", vec![observer("Secondary", -100)]),
        ],
        0,
        PlayerId::One,
    );
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    let secondary = hand_card(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::Two,
            card: secondary,
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
    let victim = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: victim,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let dominant = hand_card(&mut simulation, PlayerId::One);
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

    assert_that!(simulation.snapshot().dominant_player, eq(PlayerId::One));
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter_map(|entry| match entry {
                TraceEntry::TriggerResolved { source, .. } => Some(*source),
                _ => None,
            })
            .collect::<Vec<_>>(),
        eq(&vec![dominant, secondary])
    );
}

#[googletest::test]
fn death_prechecks_exclude_trigger_sources_created_during_the_batch() {
    let late_observer =
        Card::minion("Late Observer", 0, 1, 2).with_triggers(vec![crate::TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
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
    let trace = simulation.trace();
    let death_snapshots = trace
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match entry {
            TraceEntry::TriggerSnapshot { event, .. }
                if trace.iter().any(|candidate| {
                    matches!(
                        candidate,
                        TraceEntry::EventCreated {
                            id,
                            kind: EventKind::Death,
                            ..
                        } if id == event
                    )
                }) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let first_trigger = trace
        .iter()
        .position(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
        .unwrap();
    assert_that!(death_snapshots.len(), eq(2));
    assert_that!(death_snapshots[0] < first_trigger, is_true());
    assert_that!(death_snapshots[1] > first_trigger, is_true());
}

#[googletest::test]
fn earlier_deathrattle_can_enable_an_existing_trigger_for_a_later_death() {
    let reinforcement = Card::minion("Reinforcement", 0, 1, 2);
    let summoner = Card::minion("Early Summoner", 0, 1, 1).with_deathrattle(vec![Effect::Summon {
        player: PlayerSelector::Controller,
        card: reinforcement,
        board_index: None,
    }]);
    let observer = Card::minion("Conditional Observer", 0, 1, 2).with_triggers(vec![
        crate::TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Play],
            conditions: vec![crate::TimedCondition {
                timing: crate::ConditionTiming::QueueTime,
                condition: crate::TriggerCondition::MinimumEntityCount {
                    selector: Selector::FriendlyMinions,
                    count: 2,
                },
            }],
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            wounded_target_policy: crate::WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: vec![Effect::GainResource {
                player: PlayerSelector::Controller,
                amount: 1,
                temporary: true,
            }],
        },
    ]);
    let blast = Card::spell("Staged Death Blast", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                summoner,
                Card::minion("Later Death", 0, 1, 1),
                observer,
                blast,
            ],
        ),
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

    assert_that!(
        player(simulation.app.world(), PlayerId::One)
            .unwrap()
            .1
            .temporary_resources,
        eq(1)
    );
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerResolved { .. }))
            .count(),
        eq(2)
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
        eq(&vec![0, 0])
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
            conditions: vec![
                crate::TimedCondition {
                    timing: crate::ConditionTiming::QueueTime,
                    condition: crate::TriggerCondition::EventValueAtLeast(1),
                },
                crate::TimedCondition {
                    timing: crate::ConditionTiming::QueueTime,
                    condition: crate::TriggerCondition::EventTargetsSelf,
                },
            ],
            source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
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
    assert_that!(
        simulation
            .trace()
            .iter()
            .filter(|entry| matches!(entry, TraceEntry::TriggerAborted { .. }))
            .count(),
        eq(0)
    );
    let captured_sources = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::TriggerSnapshot { candidates, .. } => Some(candidates.as_slice()),
            _ => None,
        })
        .flatten()
        .map(|candidate| candidate.source)
        .collect::<std::collections::BTreeSet<_>>();
    assert_that!(captured_sources.is_empty(), is_false());
    assert_that!(
        simulation.trace().iter().all(|entry| match entry {
            TraceEntry::TriggerResolved { source, .. } => captured_sources.contains(source),
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
