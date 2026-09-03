use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{EnchantmentDuration, ExtraTurnTiming, KeywordModifier, ScheduledTurnKind};

fn object(simulation: &mut Simulation, id: GameEntityId) -> GameObjectSnapshot {
    simulation
        .snapshot()
        .objects
        .into_iter()
        .find(|object| object.id == id)
        .unwrap()
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

#[googletest::test]
fn permanent_enchantment_has_explicit_duration_and_survives_turn_cleanup() {
    let buff =
        Card::spell("Permanent Strength", 0).with_effects(vec![Effect::AttachStatModifier {
            targets: Selector::DeclaredTarget,
            modifier: StatModifier {
                attack: 2,
                health: 0,
                silence_removable: true,
            },
            duration: EnchantmentDuration::Permanent,
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Target", 0, 1, 2), buff]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let target = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, target, None);
    let buff = hand_card(&mut simulation, PlayerId::One);
    play_card(&mut simulation, PlayerId::One, buff, Some(target));

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();

    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<StatModifier>().is_some())
        .unwrap();
    assert_that!(enchantment.get::<Zone>(), eq(Some(&Zone::Play)));
    assert_that!(
        enchantment.get::<EnchantmentDuration>(),
        eq(Some(&EnchantmentDuration::Permanent)),
    );
    assert_that!(object(&mut simulation, target).attack, eq(Some(3)));
}

#[googletest::test]
fn after_current_extra_turns_extend_the_active_players_turn_series() {
    let time_warp =
        Card::spell("Time Warp Fixture", 0).with_effects(vec![Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 2,
            timing: ExtraTurnTiming::AfterCurrentTurn,
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![time_warp]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let time_warp = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: time_warp,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    for expected in [PlayerId::One, PlayerId::One, PlayerId::Two] {
        let active = simulation.snapshot().game.active_player;
        simulation
            .apply(GameAction::EndTurn { player: active })
            .unwrap();
        assert_that!(simulation.snapshot().game.active_player, eq(expected));
    }
    let changed = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::TurnChanged {
                active_player,
                kind,
                ..
            } => Some((*active_player, *kind)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(
        changed,
        eq(&vec![
            (PlayerId::One, ScheduledTurnKind::Extra),
            (PlayerId::One, ScheduledTurnKind::Extra),
            (PlayerId::Two, ScheduledTurnKind::Natural),
        ])
    );
}

#[googletest::test]
fn next_series_extras_are_grouped_with_each_players_natural_turn() {
    let temporus = Card::spell("Temporus Fixture", 0).with_effects(vec![
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Opponent,
            count: 1,
            timing: ExtraTurnTiming::DuringNextTurnSeries,
        },
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 1,
            timing: ExtraTurnTiming::DuringNextTurnSeries,
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![temporus]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let temporus = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: temporus,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let mut sequence = vec![PlayerId::One];
    for _ in 0..4 {
        let active = simulation.snapshot().game.active_player;
        simulation
            .apply(GameAction::EndTurn { player: active })
            .unwrap();
        sequence.push(simulation.snapshot().game.active_player);
    }
    assert_that!(
        sequence,
        eq(&vec![
            PlayerId::One,
            PlayerId::Two,
            PlayerId::Two,
            PlayerId::One,
            PlayerId::One,
        ])
    );
}

#[googletest::test]
fn end_of_turn_enchantments_expire_before_the_next_turn_starts() {
    let buff = Card::spell("Brief Strength", 0).with_effects(vec![Effect::AttachStatModifier {
        targets: Selector::DeclaredTarget,
        modifier: StatModifier {
            attack: 3,
            health: 0,
            silence_removable: true,
        },
        duration: EnchantmentDuration::EndOfTurn(PlayerId::One),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Target", 0, 1, 2), buff]),
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
    let buff = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: buff,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();
    assert_that!(object(&mut simulation, target).attack, eq(Some(4)));

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    assert_that!(object(&mut simulation, target).attack, eq(Some(1)));
    assert_that!(
        simulation
            .trace()
            .iter()
            .any(|entry| matches!(entry, TraceEntry::TemporaryEffectExpired { .. })),
        is_true()
    );
}

#[googletest::test]
fn temporary_cost_modifier_changes_legality_until_the_turn_ends() {
    let discount =
        Card::spell("Brief Discount", 0).with_effects(vec![Effect::AttachCostModifier {
            targets: Selector::DeclaredTarget,
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -2,
                silence_removable: true,
            },
            duration: EnchantmentDuration::EndOfTurn(PlayerId::One),
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Expensive", 3, 3, 3), discount]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let expensive = simulation
        .snapshot()
        .objects
        .iter()
        .find(|object| object.name == "Expensive")
        .unwrap()
        .id;
    let discount = simulation
        .snapshot()
        .objects
        .iter()
        .find(|object| object.name == "Brief Discount")
        .unwrap()
        .id;

    assert_that!(
        simulation.legal_actions().iter().any(
            |action| matches!(action, GameAction::PlayCard { card, .. } if *card == expensive)
        ),
        is_false()
    );
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: discount,
            target: Some(expensive),
            board_index: None,
            choice: None,
        })
        .unwrap();
    assert_that!(
        simulation.legal_actions().iter().any(
            |action| matches!(action, GameAction::PlayCard { card, .. } if *card == expensive)
        ),
        is_true()
    );

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    let expensive = game_entity(simulation.app.world(), expensive).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(expensive)
            .unwrap()
            .cost,
        eq(3)
    );
}

#[googletest::test]
fn ordered_cost_modifiers_keep_negative_values_until_payment() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Ordered Target", 5, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let target = hand_card(&mut simulation, PlayerId::One);
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    for (operation, value) in [
        (CostOperation::Set, 3),
        (CostOperation::Multiply, 2),
        (CostOperation::Add, -7),
    ] {
        execute_effect(
            simulation.app.world_mut(),
            &context,
            &Effect::AttachCostModifier {
                targets: Selector::Entity(target),
                modifier: CostModifier {
                    operation,
                    value,
                    silence_removable: false,
                },
                duration: EnchantmentDuration::Permanent,
            },
        )
        .unwrap();
    }
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(target_entity)
            .unwrap()
            .cost,
        eq(-1)
    );

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: target,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    assert_that!(simulation.snapshot().players[0].resources_spent, eq(0));
}

#[googletest::test]
fn checkpoint_restores_temporary_cost_modifier_payloads() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Checkpoint Target", 4, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let card = hand_card(&mut simulation, PlayerId::One);
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
            targets: Selector::Entity(card),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -1,
                silence_removable: false,
            },
            duration: EnchantmentDuration::EndOfTurn(PlayerId::One),
        },
    )
    .unwrap();

    let mut restored = Simulation::from_checkpoint(simulation.checkpoint().unwrap()).unwrap();
    assert_that!(
        restored
            .app
            .world()
            .iter_entities()
            .filter(|entity| entity.get::<CostModifier>().is_some())
            .count(),
        eq(1)
    );
    execute_effect(
        restored.app.world_mut(),
        &context,
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

    let card = game_entity(restored.app.world(), card).unwrap();
    assert_that!(
        restored.app.world().get::<CardRuntime>(card).unwrap().cost,
        eq(2)
    );
}

#[googletest::test]
fn opponent_turn_series_duration_survives_contiguous_extra_turns() {
    let setup = Card::spell("Series Fixture", 0).with_effects(vec![
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Opponent,
            count: 1,
            timing: ExtraTurnTiming::DuringNextTurnSeries,
        },
        Effect::AttachStatModifier {
            targets: Selector::DeclaredTarget,
            modifier: StatModifier {
                attack: 2,
                health: 0,
                silence_removable: true,
            },
            duration: EnchantmentDuration::EndOfTurnSeries(PlayerId::Two),
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Target", 0, 1, 2), setup]),
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
    let setup = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: setup,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::One,
        })
        .unwrap();
    assert_that!(object(&mut simulation, target).attack, eq(Some(3)));
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();
    assert_that!(simulation.snapshot().game.active_player, eq(PlayerId::Two));
    assert_that!(object(&mut simulation, target).attack, eq(Some(3)));
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();
    assert_that!(simulation.snapshot().game.active_player, eq(PlayerId::One));
    assert_that!(object(&mut simulation, target).attack, eq(Some(1)));
}

fn play_named_card(simulation: &mut Simulation, name: &str) {
    let card = simulation
        .snapshot()
        .objects
        .iter()
        .find(|object| object.name == name && object.zone == Zone::Hand)
        .unwrap()
        .id;
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

fn temporus_time_warp_sequence(reverse_play_order: bool) -> Vec<PlayerId> {
    let temporus = Card::spell("Brann Temporus Fixture", 0).with_effects(vec![
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Opponent,
            count: 2,
            timing: ExtraTurnTiming::DuringNextTurnSeries,
        },
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 1,
            timing: ExtraTurnTiming::AfterCurrentTurn,
        },
    ]);
    let time_warp = Card::spell("Time Warp Composition Fixture", 0).with_effects(vec![
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 1,
            timing: ExtraTurnTiming::AfterCurrentTurn,
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![temporus, time_warp]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let order = if reverse_play_order {
        ["Time Warp Composition Fixture", "Brann Temporus Fixture"]
    } else {
        ["Brann Temporus Fixture", "Time Warp Composition Fixture"]
    };
    for name in order {
        play_named_card(&mut simulation, name);
    }

    let mut sequence = vec![PlayerId::One];
    for _ in 0..6 {
        let active = simulation.snapshot().game.active_player;
        simulation
            .apply(GameAction::EndTurn { player: active })
            .unwrap();
        sequence.push(simulation.snapshot().game.active_player);
    }
    sequence
}

#[googletest::test]
fn temporus_and_time_warp_follow_turn_series_precedence_in_either_play_order() {
    let expected = vec![
        PlayerId::One,
        PlayerId::Two,
        PlayerId::Two,
        PlayerId::Two,
        PlayerId::One,
        PlayerId::One,
        PlayerId::One,
    ];
    assert_that!(temporus_time_warp_sequence(false), eq(&expected));
    assert_that!(temporus_time_warp_sequence(true), eq(&expected));
}

#[googletest::test]
fn opposing_temporus_effects_produce_the_rulebook_turn_series() {
    let mut schedule = TurnSchedule::default();
    schedule.schedule(
        PlayerId::One,
        PlayerId::Two,
        1,
        ExtraTurnTiming::DuringNextTurnSeries,
    );
    schedule.schedule(
        PlayerId::One,
        PlayerId::One,
        1,
        ExtraTurnTiming::DuringNextTurnSeries,
    );

    let first = schedule.next_turn(PlayerId::One);
    schedule.schedule(
        PlayerId::Two,
        PlayerId::One,
        1,
        ExtraTurnTiming::DuringNextTurnSeries,
    );
    schedule.schedule(
        PlayerId::Two,
        PlayerId::Two,
        1,
        ExtraTurnTiming::DuringNextTurnSeries,
    );
    let mut sequence = vec![first.player];
    let mut ending = first.player;
    for _ in 0..5 {
        let next = schedule.next_turn(ending);
        sequence.push(next.player);
        ending = next.player;
    }
    assert_that!(
        sequence,
        eq(&vec![
            PlayerId::Two,
            PlayerId::Two,
            PlayerId::One,
            PlayerId::One,
            PlayerId::One,
            PlayerId::Two,
        ])
    );
}

#[googletest::test]
fn after_current_grants_are_anchored_to_the_active_player_not_the_controller() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::Two,
            declared_target: None,
            origin: EffectOrigin::Other,
        },
        &Effect::ScheduleExtraTurns {
            player: PlayerSelector::Controller,
            count: 1,
            timing: ExtraTurnTiming::AfterCurrentTurn,
        },
    )
    .unwrap();

    assert_that!(
        simulation
            .app
            .world()
            .resource::<TurnSchedule>()
            .after_current_anchor,
        eq(Some(PlayerId::One))
    );
}

#[googletest::test]
fn non_stat_keyword_enchantment_expires_after_the_full_turn_series() {
    let setup = Card::spell("Series Immune Fixture", 0).with_effects(vec![
        Effect::ScheduleExtraTurns {
            player: PlayerSelector::Opponent,
            count: 1,
            timing: ExtraTurnTiming::DuringNextTurnSeries,
        },
        Effect::AttachKeywordModifier {
            targets: Selector::DeclaredTarget,
            modifier: KeywordModifier {
                keyword: Keyword::Immune,
                granted: true,
                silence_removable: true,
            },
            duration: EnchantmentDuration::EndOfTurnSeries(PlayerId::Two),
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Protected", 0, 1, 2), setup]),
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
    let setup = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: setup,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<Keywords>(target_entity)
            .unwrap()
            .0
            .contains(&Keyword::Immune),
        is_true()
    );

    for expected_active in [PlayerId::Two, PlayerId::Two] {
        let active = simulation.snapshot().game.active_player;
        simulation
            .apply(GameAction::EndTurn { player: active })
            .unwrap();
        assert_that!(
            simulation.snapshot().game.active_player,
            eq(expected_active)
        );
        assert_that!(
            simulation
                .app
                .world()
                .get::<Keywords>(target_entity)
                .unwrap()
                .0
                .contains(&Keyword::Immune),
            is_true()
        );
    }
    simulation
        .apply(GameAction::EndTurn {
            player: PlayerId::Two,
        })
        .unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<Keywords>(target_entity)
            .unwrap()
            .0
            .contains(&Keyword::Immune),
        is_false()
    );
}

#[googletest::test]
fn checkpoint_roundtrip_preserves_pending_turn_schedule() {
    let warp = Card::spell("Warp", 0).with_effects(vec![Effect::ScheduleExtraTurns {
        player: PlayerSelector::Controller,
        count: 1,
        timing: ExtraTurnTiming::AfterCurrentTurn,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![warp]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let warp = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: warp,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let checkpoint = simulation.checkpoint().unwrap();
    let restored = Simulation::from_checkpoint(checkpoint.clone()).unwrap();
    assert_that!(
        restored.app.world().resource::<TurnSchedule>(),
        eq(&checkpoint.turn_schedule)
    );
}
