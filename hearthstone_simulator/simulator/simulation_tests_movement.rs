use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AttackState, CurrentStats, EnchantmentDuration, HeroClassPolicy, HeroHealthPolicy,
    HeroReplacement, KeepEnchantments, KeywordModifier, PhaseBoundaryPlan, ZoneMoveOutcome,
    ZoneMoveRequest, ZoneMovementKind,
};

fn move_target_to_hand() -> Effect {
    Effect::Move {
        targets: Selector::DeclaredTarget,
        player: PlayerSelector::Controller,
        zone: Zone::Hand,
        kind: ZoneMovementKind::Normal,
    }
}

#[googletest::test]
fn play_zone_enchantments_do_not_consume_board_capacity() {
    let cards = (0..7)
        .map(|index| Card::minion(format!("Minion {index}"), 0, 1, 1))
        .collect::<Vec<_>>();
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", cards),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    for _ in 0..7 {
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
    let target = simulation.snapshot().players[0].board[0];
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachKeywordModifier {
            targets: Selector::Entity(target),
            modifier: KeywordModifier {
                keyword: Keyword::Taunt,
                granted: true,
                silence_removable: true,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();

    assert_that!(simulation.snapshot().players[0].board.len(), eq(7));
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<KeywordModifier>().is_some())
        .unwrap();
    assert_that!(enchantment.get::<Zone>(), eq(Some(&Zone::Play)),);
}

#[googletest::test]
fn invariants_reject_enchantments_without_durations_or_play_zone() {
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
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<EntityKind>() == Some(&EntityKind::Enchantment))
        .unwrap()
        .id();
    simulation
        .app
        .world_mut()
        .entity_mut(enchantment)
        .remove::<EnchantmentDuration>();

    assert_that!(
        simulation.assert_invariants(),
        err(matches_pattern!(SimulationError::Invariant(
            contains_substring("enchantment duration")
        ))),
    );

    simulation
        .app
        .world_mut()
        .entity_mut(enchantment)
        .insert((EnchantmentDuration::Permanent, Zone::Hand));
    assert_that!(
        assert_enchantment_invariants(simulation.app.world()),
        err(contains_substring("not in Play")),
    );
}

#[googletest::test]
fn backward_movement_resets_runtime_tags_and_detaches_enchantments() {
    let reset = Card::spell("Reset", 0).with_effects(vec![
        Effect::AttachStatModifier {
            targets: Selector::DeclaredTarget,
            modifier: StatModifier {
                attack: 3,
                health: 2,
                silence_removable: true,
            },
            duration: EnchantmentDuration::Permanent,
        },
        Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        },
        Effect::Destroy {
            targets: Selector::DeclaredTarget,
        },
        move_target_to_hand(),
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Traveler", 0, 2, 3).with_keyword(Keyword::Taunt),
                reset,
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let traveler = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: traveler,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let traveler_entity = game_entity(simulation.app.world(), traveler).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<Keywords>(traveler_entity)
        .unwrap()
        .0
        .insert(Keyword::Stealth);
    let reset = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: reset,
            target: Some(traveler),
            board_index: None,
            choice: None,
        })
        .unwrap();

    let traveler_entity = game_entity(simulation.app.world(), traveler).unwrap();
    assert_that!(
        simulation.app.world().get::<Zone>(traveler_entity),
        eq(Some(&Zone::Hand))
    );
    assert_that!(
        simulation.app.world().get::<Damage>(traveler_entity),
        eq(Some(&Damage(0)))
    );
    assert_that!(
        simulation
            .app
            .world()
            .get::<PendingDestroy>(traveler_entity),
        none()
    );
    assert_that!(
        simulation.app.world().get::<CurrentStats>(traveler_entity),
        eq(Some(&CurrentStats {
            attack: 2,
            maximum_health: 3,
        }))
    );
    let keywords = &simulation
        .app
        .world()
        .get::<Keywords>(traveler_entity)
        .unwrap()
        .0;
    assert_that!(keywords.contains(&Keyword::Taunt), is_true());
    assert_that!(keywords.contains(&Keyword::Stealth), is_false());
    let snapshot = simulation.snapshot();
    let detached = snapshot
        .objects
        .iter()
        .find(|object| object.kind == EntityKind::Enchantment)
        .unwrap();
    assert_that!(detached.zone, eq(Zone::RemovedFromGame));
    let detached_entity = game_entity(simulation.app.world(), detached.id).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<EnchantmentDuration>(detached_entity),
        eq(Some(&EnchantmentDuration::Permanent)),
    );
}

#[googletest::test]
fn backward_movement_restores_cost_after_detaching_modifiers() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Reset cost", 4, 1, 1)]),
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
                value: -2,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();

    crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: card,
            destination_controller: PlayerId::One,
            destination: Zone::Deck,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();

    let card = game_entity(simulation.app.world(), card).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(card)
            .unwrap()
            .cost,
        eq(4)
    );
}

#[googletest::test]
fn death_reset_restores_innate_keywords() {
    let destroy = Card::spell("Destroy innate", 0).with_effects(vec![Effect::Destroy {
        targets: Selector::DeclaredTarget,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Innate Taunt", 0, 1, 1).with_keywords([Keyword::Taunt]),
                destroy,
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let minion = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: minion,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let entity = game_entity(simulation.app.world(), minion).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<Keywords>(entity)
        .unwrap()
        .0
        .insert(Keyword::Stealth);
    let destroy = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: destroy,
            target: Some(minion),
            board_index: None,
            choice: None,
        })
        .unwrap();

    let keywords = &simulation.app.world().get::<Keywords>(entity).unwrap().0;
    assert_that!(
        simulation.app.world().get::<Zone>(entity),
        eq(Some(&Zone::Graveyard))
    );
    assert_that!(keywords.contains(&Keyword::Taunt), is_true());
    assert_that!(keywords.contains(&Keyword::Stealth), is_false());
}

#[googletest::test]
fn keep_enchantments_preserves_attached_modifiers_during_backward_movement() {
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Persistent", 0, 1, 1),
                Card::spell("Bounce", 0).with_effects(vec![move_target_to_hand()]),
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let persistent = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: persistent,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let persistent_entity = game_entity(simulation.app.world(), persistent).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(persistent_entity)
        .insert(KeepEnchantments);
    attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        persistent,
        StatModifier {
            attack: 2,
            health: 2,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let bounce = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: bounce,
            target: Some(persistent),
            board_index: None,
            choice: None,
        })
        .unwrap();

    let entity = game_entity(simulation.app.world(), persistent).unwrap();
    assert_that!(
        simulation.app.world().get::<Zone>(entity),
        eq(Some(&Zone::Hand))
    );
    assert_that!(
        simulation.app.world().get::<CurrentStats>(entity),
        eq(Some(&CurrentStats {
            attack: 3,
            maximum_health: 3,
        }))
    );
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .filter(|object| object.kind == EntityKind::Enchantment)
            .all(|object| object.zone == Zone::Play),
        is_true()
    );
}

#[googletest::test]
fn forward_movement_preserves_enchantments() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::minion("Topdeck", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let topdeck = simulation.snapshot().players[0].deck[0];
    attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        topdeck,
        StatModifier {
            attack: 4,
            health: 0,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    draw_card(simulation.app.world_mut(), PlayerId::One).unwrap();

    let entity = game_entity(simulation.app.world(), topdeck).unwrap();
    assert_that!(
        simulation.app.world().get::<Zone>(entity),
        eq(Some(&Zone::Hand))
    );
    assert_that!(
        simulation.app.world().get::<CurrentStats>(entity),
        eq(Some(&CurrentStats {
            attack: 5,
            maximum_health: 1,
        }))
    );
}

#[googletest::test]
fn force_play_into_a_full_board_is_prevented_without_moving_the_entity() {
    let mut simulation = Simulation::new([
        PlayerConfig::with_deck("Jaina", vec![Card::minion("Waiting", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    simulation
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .board_limit = 0;
    let waiting = simulation.snapshot().players[0].deck[0];
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            origin: EffectOrigin::Other,
        },
        &Effect::Move {
            targets: Selector::Entity(waiting),
            player: PlayerSelector::Controller,
            zone: Zone::Play,
            kind: ZoneMovementKind::ForcePlay,
        },
    )
    .unwrap();

    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == waiting)
            .unwrap()
            .zone,
        eq(Zone::Deck)
    );
}

#[googletest::test]
fn same_zone_movement_bypasses_capacity_and_reapplies_exhaustion() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Restless", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let restless = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: restless,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    simulation
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .board_limit = 0;
    let entity = game_entity(simulation.app.world(), restless).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<AttackState>(entity)
        .unwrap()
        .exhausted = false;

    let outcome =
        crate::zone::move_entity(simulation.app.world_mut(), restless, Zone::Play, None).unwrap();

    assert_that!(matches!(outcome, ZoneMoveOutcome::Moved { .. }), is_true());
    assert_that!(
        simulation
            .app
            .world()
            .get::<AttackState>(entity)
            .unwrap()
            .exhausted,
        is_true()
    );
}

#[googletest::test]
fn same_zone_movement_without_a_position_preserves_board_order() {
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Left", 0, 1, 1),
                Card::minion("Middle", 0, 1, 1),
                Card::minion("Right", 0, 1, 1),
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
            .unwrap();
    }
    let before = simulation.snapshot();
    let middle = before
        .objects
        .iter()
        .find(|object| object.name == "Middle")
        .unwrap()
        .id;

    crate::zone::move_entity(simulation.app.world_mut(), middle, Zone::Play, None).unwrap();

    assert_that!(
        simulation.snapshot().players[0].board,
        eq(&before.players[0].board)
    );
}

#[googletest::test]
fn ruleset_capacity_covers_deck_battlefield_roles_and_secret_restrictions() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    simulation
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .deck_limit = 1;
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::spell("Deck one", 0),
        Zone::Deck,
    )
    .unwrap();
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::spell("Deck two", 0),
            Zone::Deck,
        ),
        err(anything())
    );
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::hero("Duplicate hero", 30),
            Zone::Play,
        ),
        err(anything())
    );
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::hero_power("Duplicate power", 2),
            Zone::Play,
        ),
        err(anything())
    );
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::weapon("First weapon", 0, 1),
        Zone::Play,
    )
    .unwrap();
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::weapon("Second weapon", 0, 1),
            Zone::Play,
        ),
        err(anything())
    );

    let mut secret = Card::spell("Unique secret", 0);
    secret.kind = EntityKind::Secret;
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        secret.clone(),
        Zone::Secret,
    )
    .unwrap();
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            secret,
            Zone::Secret,
        ),
        err(anything())
    );

    let mut first_quest = Card::spell("First quest", 0);
    first_quest.kind = EntityKind::Quest;
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        first_quest,
        Zone::Secret,
    )
    .unwrap();
    let mut second_quest = Card::spell("Second quest", 0);
    second_quest.kind = EntityKind::Quest;
    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            second_quest,
            Zone::Secret,
        ),
        err(anything())
    );
}

#[googletest::test]
fn full_zone_generation_does_not_consume_a_logical_identity() {
    let hand = (0..10)
        .map(|index| Card::spell(format!("Filler {index}"), 0))
        .collect::<Vec<_>>();
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", hand),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let next = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;

    assert_that!(
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::spell("No Room", 0),
            Zone::Hand,
        ),
        err(anything())
    );
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::NextGameEntityId>()
            .0,
        eq(next)
    );
}

#[googletest::test]
fn death_movement_resets_hero_armor() {
    let mut simulation = simulation();
    let hero = hero(&mut simulation, PlayerId::One);
    let entity = game_entity(simulation.app.world(), hero).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(entity)
        .insert(Armor(7));

    let outcome = crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: hero,
            destination_controller: PlayerId::One,
            destination: Zone::Graveyard,
            position: None,
            kind: ZoneMovementKind::Death,
        },
    )
    .unwrap();

    assert_that!(matches!(outcome, ZoneMoveOutcome::Moved { .. }), is_true());
    assert_that!(
        simulation.app.world().get::<Armor>(entity),
        eq(Some(&Armor(0)))
    );
}

#[googletest::test]
fn copying_missing_entities_or_into_full_zones_is_a_deterministic_no_op() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Copy source", 0, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        origin: EffectOrigin::Other,
    };
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(GameEntityId(u64::MAX)),
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
        },
    )
    .unwrap();

    let source = hand_card(&mut simulation, PlayerId::One);
    let before = simulation.snapshot().players[0].hand.clone();
    simulation
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .hand_limit = before.len();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
        },
    )
    .unwrap();

    assert_that!(simulation.snapshot().players[0].hand, eq(&before));
}

#[googletest::test]
fn hand_copy_does_not_make_a_temporary_discount_part_of_base_cost() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Discounted source", 5, 1, 1)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let source = hand_card(&mut simulation, PlayerId::One);
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
            targets: Selector::Entity(source),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -2,
                silence_removable: false,
            },
            duration: EnchantmentDuration::EndOfTurn(PlayerId::One),
        },
    )
    .unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
        },
    )
    .unwrap();

    let copy = simulation
        .app
        .world()
        .resource::<ZoneIndex>()
        .entities(PlayerId::One, Zone::Hand)
        .iter()
        .copied()
        .find(|card| *card != source)
        .unwrap();
    let copy = game_entity(simulation.app.world(), copy).unwrap();
    let runtime = simulation.app.world().get::<CardRuntime>(copy).unwrap();
    assert_that!(runtime.base_cost, eq(5));
    assert_that!(runtime.cost, eq(5));
}

#[googletest::test]
fn instant_and_ordinary_deaths_resolve_globally_by_play_order() {
    let old =
        Card::minion("Old ordinary death", 0, 1, 1).with_deathrattle(vec![Effect::ReplaceHero {
            player: PlayerSelector::Controller,
            replacement: Box::new(HeroReplacement {
                hero: Card::hero("Ordered replacement", 10),
                hero_power: Card::hero_power("Ordered power", 2),
                armor_gain: 0,
                health: HeroHealthPolicy::Set {
                    maximum_health: 10,
                    current_health: 10,
                },
                class: HeroClassPolicy::Keep,
                weapon: None,
            }),
        }]);
    let new =
        Card::minion("New instant death", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::FriendlyCharacters,
            amount: ValueExpression::Constant(3),
        }]);
    let full_hand = (0..10)
        .map(|index| Card::spell(format!("Full hand {index}"), 0))
        .collect();
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", full_hand),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let old = spawn_card(simulation.app.world_mut(), PlayerId::One, old, Zone::Play).unwrap();
    let old_entity = game_entity(simulation.app.world(), old).unwrap();
    let old_order = crate::entity::allocate_play_order(simulation.app.world_mut());
    simulation
        .app
        .world_mut()
        .entity_mut(old_entity)
        .insert((old_order, Damage(1)));
    let new = spawn_card(simulation.app.world_mut(), PlayerId::One, new, Zone::Play).unwrap();
    let new_entity = game_entity(simulation.app.world(), new).unwrap();
    let new_order = crate::entity::allocate_play_order(simulation.app.world_mut());
    simulation
        .app
        .world_mut()
        .entity_mut(new_entity)
        .insert(new_order);

    let outcome = crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: new,
            destination_controller: PlayerId::One,
            destination: Zone::Hand,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();
    assert_that!(
        matches!(outcome, ZoneMoveOutcome::FullZoneRemoval { .. }),
        is_true()
    );

    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    push_resolution_ops(
        world,
        [ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary)],
    );
    super::action::drive_resolution(world).unwrap();
    finish_sequence(world);

    let queued = simulation
        .trace()
        .iter()
        .find_map(|entry| match entry {
            TraceEntry::DeathPhaseQueued { deaths } => Some(deaths.clone()),
            _ => None,
        })
        .unwrap();
    assert_that!(queued, eq(&vec![old, new]));
    assert_that!(simulation.snapshot().players[0].health, eq(7));
    assert_that!(
        simulation
            .snapshot()
            .deaths
            .iter()
            .map(|record| record.entity)
            .collect::<Vec<_>>(),
        eq(&vec![new, old])
    );
}

#[googletest::test]
fn simultaneous_bounce_uses_play_order_and_full_zone_removal_records_death() {
    let doomed = Card::minion("Doomed", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
        targets: Selector::EnemyCharacters,
        amount: ValueExpression::Constant(1),
    }]);
    let mover = Card::spell("Vanish Fixture", 0).with_effects(vec![
        Effect::Summon {
            player: PlayerSelector::Controller,
            card: Card::minion("Oldest", 0, 1, 1),
            board_index: None,
        },
        Effect::Summon {
            player: PlayerSelector::Controller,
            card: doomed,
            board_index: None,
        },
        Effect::Move {
            targets: Selector::FriendlyMinions,
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
            kind: ZoneMovementKind::Normal,
        },
    ]);
    let mut hand = (0..9)
        .map(|index| Card::spell(format!("Filler {index}"), 0))
        .collect::<Vec<_>>();
    hand.push(mover);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", hand),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let mover = *simulation.snapshot().players[0].hand.last().unwrap();
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: mover,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let snapshot = simulation.snapshot();
    let oldest = snapshot
        .objects
        .iter()
        .find(|object| object.name == "Oldest")
        .unwrap();
    let doomed = snapshot
        .objects
        .iter()
        .find(|object| object.name == "Doomed")
        .unwrap();
    assert_that!(oldest.zone, eq(Zone::Hand));
    assert_that!(doomed.zone, eq(Zone::Graveyard));
    assert_that!(
        snapshot
            .deaths
            .iter()
            .any(|record| record.entity == doomed.id),
        is_true()
    );
    assert_that!(snapshot.players[1].health, eq(29));
}
