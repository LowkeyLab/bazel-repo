use googletest::prelude::*;

use super::{test_support::*, *};
use crate::{
    AttachedTo, ConditionTiming, EnchantmentDuration, SourceEligibilityPolicy, TimedCondition,
    TriggerCondition, TriggerDefinition, WoundedTargetPolicy,
};

fn turn_end_trigger(event_player: PlayerSelector, effects: Vec<Effect>) -> TriggerDefinition {
    TriggerDefinition {
        event: EventKind::TurnEnded,
        eligible_zones: vec![Zone::Play],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventControllerIs(event_player),
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: effects,
    }
}

fn play_card(
    simulation: &mut Simulation,
    player: PlayerId,
    card: GameEntityId,
    target: Option<GameEntityId>,
) {
    simulation
        .apply(GameAction::PlayCard {
            player,
            card,
            target,
            board_index: None,
            choice: None,
        })
        .unwrap();
}

fn attached_enchantment(simulation: &Simulation, host: GameEntityId) -> GameEntityId {
    let host = game_entity(simulation.app.world(), host).unwrap();
    simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<AttachedTo>().map(|attached| attached.0) == Some(host))
        .and_then(|entity| entity.get::<GameEntityId>())
        .copied()
        .unwrap()
}

#[googletest::test]
fn enchantment_triggers_share_play_order_with_ordinary_sources() {
    let ordinary = Card::minion("Ordinary source", 0, 1, 2).with_triggers(vec![turn_end_trigger(
        PlayerSelector::Controller,
        Vec::new(),
    )]);
    let grant =
        Card::spell("Grant trigger", 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::DeclaredTarget,
            triggers: vec![turn_end_trigger(PlayerSelector::Controller, Vec::new())],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![ordinary, Card::minion("Host", 0, 1, 2), grant],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let ordinary_source = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, ordinary_source, None);
    let host = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, host, None);
    let grant = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, grant, Some(host));
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<EnchantmentDuration>().is_some())
        .and_then(|entity| entity.get::<GameEntityId>())
        .copied()
        .unwrap();

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();

    let resolved = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::TriggerResolved { source, .. } => Some(*source),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(resolved, eq(&vec![ordinary_source, enchantment]));
}

#[googletest::test]
fn enchantment_controller_determines_trigger_group_not_host_controller() {
    let player_two_source =
        Card::minion("Player Two source", 0, 1, 2).with_triggers(vec![turn_end_trigger(
            PlayerSelector::Controller,
            Vec::new(),
        )]);
    let mut simulation = Simulation::with_seed_and_dominant_player(
        [
            PlayerConfig::new("Jaina", Vec::new()),
            PlayerConfig::new(
                "Rexxar",
                vec![player_two_source, Card::minion("Player Two host", 0, 1, 2)],
            ),
        ],
        0,
        PlayerId::One,
    );
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    let player_two_source = hand_card(&mut simulation, PlayerId::Two);
    play_card(&mut simulation, PlayerId::Two, player_two_source, None);
    let host = hand_card(&mut simulation, PlayerId::Two);
    play_card(&mut simulation, PlayerId::Two, host, None);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachTriggerEnchantment {
            targets: Selector::Entity(host),
            triggers: vec![turn_end_trigger(PlayerSelector::Opponent, Vec::new())],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        },
    )
    .unwrap();
    let player_one_enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<EnchantmentDuration>().is_some())
        .and_then(|entity| entity.get::<GameEntityId>())
        .copied()
        .unwrap();

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();

    let cross_controller_resolved = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::TriggerResolved { source, .. } => Some(*source),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(
        cross_controller_resolved,
        eq(&vec![player_one_enchantment, player_two_source]),
    );
}

#[googletest::test]
fn transforming_the_host_aborts_an_attached_trigger_captured_later_in_the_queue() {
    let suppressor = Card::minion("Suppressor", 0, 1, 2).with_triggers(vec![TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: Vec::new(),
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: vec![Effect::Transform {
            targets: Selector::DeclaredTarget,
            card: Card::minion("Transformed host", 0, 2, 2),
        }],
    }]);
    let attached_trigger = TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventTargetsAttachedEntity,
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 1,
            temporary: true,
        }],
    };
    let grant =
        Card::spell("Grant observer", 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::DeclaredTarget,
            triggers: vec![attached_trigger],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        }]);
    let bolt = Card::spell("Bolt", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(1),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![suppressor, Card::minion("Host", 0, 1, 4), grant, bolt],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let suppressor = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, suppressor, None);
    let host = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, host, None);
    let grant = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, grant, Some(host));
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<EnchantmentDuration>().is_some())
        .and_then(|entity| entity.get::<GameEntityId>())
        .copied()
        .unwrap();
    let bolt = hand_card(&mut simulation, PlayerId::One);

    play_card(&mut simulation, PlayerId::One, bolt, Some(host));

    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::TriggerSnapshot { candidates, .. }
                if candidates.iter().any(|candidate| candidate.source == enchantment)
        )),
        is_true(),
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::TriggerAborted { source, .. } if *source == enchantment
        )),
        is_true(),
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::TriggerResolved { source, .. } if *source == enchantment
        )),
        is_false(),
    );
}

#[googletest::test]
fn silence_removes_only_trigger_enchantments_marked_removable() {
    let grant = |name, silence_removable| {
        Card::spell(name, 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::DeclaredTarget,
            triggers: vec![turn_end_trigger(PlayerSelector::Controller, Vec::new())],
            duration: EnchantmentDuration::Permanent,
            silence_removable,
        }])
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Removable host", 0, 1, 2),
                Card::minion("Retained host", 0, 1, 2),
                grant("Removable trigger", true),
                grant("Retained trigger", false),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let removable_host = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, removable_host, None);
    let retained_host = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, retained_host, None);
    let removable_grant = hand_card(&mut simulation, PlayerId::One);
    play_card(
        &mut simulation,
        PlayerId::One,
        removable_grant,
        Some(removable_host),
    );
    let removable = attached_enchantment(&simulation, removable_host);
    let retained_grant = hand_card(&mut simulation, PlayerId::One);
    play_card(
        &mut simulation,
        PlayerId::One,
        retained_grant,
        Some(retained_host),
    );
    let retained = attached_enchantment(&simulation, retained_host);
    for host in [removable_host, retained_host] {
        execute_effect(
            simulation.app.world_mut(),
            &EffectContext {
                source: None,
                controller: PlayerId::One,
                declared_target: None,
                origin: EffectOrigin::Other,
            },
            &Effect::Silence {
                targets: Selector::Entity(host),
            },
        )
        .unwrap();
    }

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();

    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == removable)
            .unwrap()
            .zone,
        eq(Zone::RemovedFromGame),
    );
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == retained)
            .unwrap()
            .zone,
        eq(Zone::Play),
    );
    let turn_ended_event = simulation
        .trace()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            TraceEntry::EventCreated {
                id,
                kind: EventKind::TurnEnded,
                ..
            } => Some(*id),
            _ => None,
        })
        .unwrap();
    let candidates = simulation
        .trace()
        .iter()
        .find_map(|entry| match entry {
            TraceEntry::TriggerSnapshot { event, candidates } if *event == turn_ended_event => {
                Some(candidates)
            }
            _ => None,
        })
        .unwrap();
    assert_that!(
        candidates
            .iter()
            .any(|candidate| candidate.source == retained),
        is_true(),
    );
    assert_that!(
        candidates
            .iter()
            .any(|candidate| candidate.source == removable),
        is_false(),
    );
}

#[googletest::test]
fn attached_trigger_uses_host_and_event_controller_context() {
    let trigger = TriggerDefinition {
        event: EventKind::TurnEnded,
        eligible_zones: vec![Zone::Play],
        conditions: vec![TimedCondition {
            timing: ConditionTiming::QueueTime,
            condition: TriggerCondition::EventControllerIs(PlayerSelector::Controller),
        }],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: vec![Effect::DealDamage {
            targets: Selector::AttachedEntity,
            amount: ValueExpression::Constant(1),
        }],
    };
    let grant =
        Card::spell("Grant trigger", 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::DeclaredTarget,
            triggers: vec![trigger],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Host", 0, 1, 4), grant]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let host = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: host,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let grant = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: grant,
            target: Some(host),
            board_index: None,
            choice: None,
        })
        .unwrap();

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == host)
            .unwrap()
            .damage,
        eq(1)
    );

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == host)
            .unwrap()
            .damage,
        eq(1)
    );
}

#[googletest::test]
fn attached_trigger_can_require_the_event_to_target_its_host() {
    let trigger = TriggerDefinition {
        event: EventKind::Damage,
        eligible_zones: vec![Zone::Play],
        conditions: vec![
            TimedCondition {
                timing: ConditionTiming::QueueTime,
                condition: TriggerCondition::EventTargetsAttachedEntity,
            },
            TimedCondition {
                timing: ConditionTiming::QueueTime,
                condition: TriggerCondition::MinimumEntityCount {
                    selector: Selector::AttachedEntity,
                    count: 1,
                },
            },
        ],
        source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: WoundedTargetPolicy::ExcludeMortallyWounded,
        effect_program: vec![Effect::GainResource {
            player: PlayerSelector::Controller,
            amount: 1,
            temporary: true,
        }],
    };
    let grant =
        Card::spell("Grant observer", 0).with_effects(vec![Effect::AttachTriggerEnchantment {
            targets: Selector::DeclaredTarget,
            triggers: vec![trigger],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        }]);
    let bolt = || {
        Card::spell("Bolt", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        }])
    };
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Host", 0, 1, 4),
                Card::minion("Other", 0, 1, 4),
                grant,
                bolt(),
                bolt(),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let host = hand_card(&mut simulation, PlayerId::One);
    let other = {
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: host,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        let other = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: other,
                target: None,
                board_index: None,
                choice: None,
            })
            .unwrap();
        other
    };
    for target in [host, other, host] {
        let card = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: Some(target),
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
}

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
