use googletest::prelude::*;

use super::effect_executor::copy_entity;
use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AttachedTo, AttackAuraCache, AttackState, ContinuousEffectDefinition, ContinuousModifier,
    Controller, CopyRequest, CopyStatePolicy, CostOperation, CurrentStats, DefinitionId,
    DisplayName, DrawOutcome, EnchantmentDuration, HealthAuraCache, HeroClassPolicy,
    HeroHealthPolicy, HeroReplacement, KeepEnchantments, KeywordModifier, OtherAuraCache,
    PhaseBoundaryPlan, PlayOrder, PlayerAudience, RuntimeContinuousEffects, RuntimeTriggers,
    SilenceRemovable, SourceEligibilityPolicy, TargetAudience, TargetFilter, TargetKind,
    TargetRequirement, TransformKind, TriggerDefinition, WoundedTargetPolicy, ZoneMoveOutcome,
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
            drawn_card: None,
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
    let reset = Card::spell("Reset", 0)
        .with_effects(vec![
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
        ])
        .with_targeting(TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Friendly,
            kind: TargetKind::Minion,
        }));
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Traveler", 0, 2, 3)
                    .with_keyword(Keyword::Taunt)
                    .with_targeting(TargetRequirement::RequiredIfAvailable(TargetFilter {
                        audience: TargetAudience::Either,
                        kind: TargetKind::Character,
                    })),
                reset,
            ],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let traveler = hand_card(&mut simulation, PlayerId::One);
    let declared_target = hero(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: traveler,
            target: Some(declared_target),
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
    assert_eq!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(traveler_entity)
            .unwrap()
            .targeting,
        TargetRequirement::RequiredIfAvailable(TargetFilter {
            audience: TargetAudience::Either,
            kind: TargetKind::Character,
        })
    );
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
        drawn_card: None,
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
    let destroy = Card::spell("Destroy innate", 0)
        .with_effects(vec![Effect::Destroy {
            targets: Selector::DeclaredTarget,
        }])
        .with_targeting(TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Friendly,
            kind: TargetKind::Minion,
        }));
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
                Card::spell("Bounce", 0)
                    .with_effects(vec![move_target_to_hand()])
                    .with_targeting(TargetRequirement::Required(TargetFilter {
                        audience: TargetAudience::Friendly,
                        kind: TargetKind::Minion,
                    })),
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
fn kept_continuous_effect_is_inactive_while_its_host_is_off_board() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Persistent", 0, 1, 1)]),
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
    execute_effect(
        simulation.app.world_mut(),
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: None,
            drawn_card: None,
            origin: EffectOrigin::Other,
        },
        &Effect::AttachContinuousEffect {
            targets: Selector::Entity(persistent),
            effect: ContinuousEffectDefinition {
                recipients: PlayerAudience::Controller,
                modifier: ContinuousModifier::SpellDamage(2),
            },
            silence_removable: true,
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| {
            entity.get::<AttachedTo>().map(|attached| attached.0) == Some(persistent_entity)
                && entity.get::<RuntimeContinuousEffects>().is_some()
        })
        .unwrap()
        .id();
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::One),
        eq(2)
    );

    crate::zone::move_entity_with_request(
        simulation.app.world_mut(),
        ZoneMoveRequest {
            entity: persistent,
            destination_controller: PlayerId::One,
            destination: Zone::Hand,
            position: None,
            kind: ZoneMovementKind::Normal,
        },
    )
    .unwrap();

    assert_that!(
        simulation.app.world().get::<AttachedTo>(enchantment),
        eq(Some(&AttachedTo(persistent_entity)))
    );
    assert_that!(
        simulation.app.world().get::<Zone>(enchantment),
        eq(Some(&Zone::Play))
    );
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::One),
        eq(0)
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
    let world = simulation.app.world_mut();
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
            count: 1,
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);
    world.resource_mut::<GameState>().status = SimulationStatus::AwaitingAction;

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
fn a_burn_is_not_draw_discard_or_death() {
    let hand = (0..10)
        .map(|index| Card::spell(format!("Filler {index}"), 0))
        .collect::<Vec<_>>();
    let mut simulation = Simulation::new([
        PlayerConfig {
            name: "Jaina".to_owned(),
            deck: vec![Card::spell("Burned", 0)],
            hand,
        },
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let burned = simulation.snapshot().players[0].deck[0];
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
            count: 1,
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    assert_that!(
        simulation
            .app
            .world()
            .resource::<ZoneIndex>()
            .entities(PlayerId::One, Zone::Graveyard),
        contains(eq(&burned))
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(entry, TraceEntry::DrawResolved { player: PlayerId::One, outcome: DrawOutcome::Burned(card), .. } if *card == burned)),
        is_true()
    );
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(entry, TraceEntry::EventCreated { kind: EventKind::CardDrawn, targets, .. } if targets.contains(&burned))),
        is_false()
    );
    let death_cache = simulation.app.world().resource::<DeathEventCache>();
    assert_that!(
        death_cache
            .records
            .iter()
            .any(|record| record.entity == burned),
        is_false()
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
            drawn_card: None,
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
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    let next = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    begin_sequence(simulation.app.world_mut()).unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(GameEntityId(u64::MAX)),
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
            board_index: None,
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    finish_sequence(simulation.app.world_mut());
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::NextGameEntityId>()
            .0,
        eq(next)
    );

    let source = hand_card(&mut simulation, PlayerId::One);
    let before = simulation.snapshot().players[0].hand.clone();
    simulation
        .app
        .world_mut()
        .resource_mut::<Ruleset>()
        .hand_limit = before.len();
    let next = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    begin_sequence(simulation.app.world_mut()).unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Controller,
            zone: Zone::Hand,
            board_index: None,
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    finish_sequence(simulation.app.world_mut());

    assert_that!(simulation.snapshot().players[0].hand, eq(&before));
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
fn non_play_copy_uses_current_form_without_runtime_attachments() {
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![Card::minion("Original source", 1, 1, 1).with_targeting(
                TargetRequirement::Optional(TargetFilter {
                    audience: TargetAudience::Friendly,
                    kind: TargetKind::Minion,
                }),
            )],
        ),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let source = hand_card(&mut simulation, PlayerId::One);
    let existing = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Existing minion", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    begin_sequence(simulation.app.world_mut()).unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Opponent,
            zone: Zone::Play,
            board_index: Some(0),
        },
    )
    .unwrap();

    transform_entity(
        simulation.app.world_mut(),
        source,
        Card::minion("Current form", 5, 4, 6).with_targeting(TargetRequirement::Required(
            TargetFilter {
                audience: TargetAudience::Enemy,
                kind: TargetKind::Character,
            },
        )),
        TransformKind::Spell,
    )
    .unwrap();
    assert_eq!(
        copy_card_data(simulation.app.world(), source)
            .unwrap()
            .targeting,
        TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        })
    );
    copy_entity(
        simulation.app.world_mut(),
        CopyRequest {
            source,
            originating_source: None,
            controller: PlayerId::Two,
            destination: Zone::Hand,
            board_index: None,
            policy: CopyStatePolicy::CurrentForm,
        },
    )
    .unwrap();
    let hand_copy = *simulation.snapshot().players[1].hand.last().unwrap();
    let hand_copy_entity = game_entity(simulation.app.world(), hand_copy).unwrap();
    assert_eq!(
        simulation
            .app
            .world()
            .get::<CardRuntime>(hand_copy_entity)
            .unwrap()
            .targeting,
        TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        })
    );
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
        &Effect::AttachStatModifier {
            targets: Selector::Entity(source),
            modifier: StatModifier {
                attack: 3,
                health: 2,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();
    let source_entity = game_entity(simulation.app.world(), source).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(source_entity)
        .insert((
            Damage(3),
            Silenced,
            PendingDestroy,
            AttackAuraCache::default(),
            HealthAuraCache::default(),
            OtherAuraCache::default(),
            PlayOrder(77),
        ));

    drive_resolution(simulation.app.world_mut()).unwrap();
    finish_sequence(simulation.app.world_mut());

    let board = simulation.snapshot().players[1].board.clone();
    assert_that!(board.len(), eq(2));
    assert_that!(board[1], eq(existing));
    let copy = board[0];
    let copy_entity = game_entity(simulation.app.world(), copy).unwrap();
    let world = simulation.app.world();
    let runtime = world.get::<CardRuntime>(copy_entity).unwrap();
    assert_eq!(
        runtime.targeting,
        TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        })
    );
    assert_that!(
        world.get::<DefinitionId>(copy_entity).unwrap().0.as_str(),
        eq("synthetic:current_form")
    );
    assert_that!(
        world.get::<DisplayName>(copy_entity).unwrap().0.as_str(),
        eq("Current form")
    );
    assert_that!(runtime.base_cost, eq(5));
    assert_that!(runtime.cost, eq(5));
    assert_that!(
        world.get::<CurrentStats>(copy_entity),
        eq(Some(&CurrentStats {
            attack: 4,
            maximum_health: 6
        }))
    );
    assert_that!(world.get::<Silenced>(copy_entity), none());
    assert_that!(world.get::<Damage>(copy_entity), eq(Some(&Damage(0))));
    assert_that!(world.get::<PendingDestroy>(copy_entity), none());
    assert_that!(world.get::<AttackAuraCache>(copy_entity), none());
    assert_that!(world.get::<HealthAuraCache>(copy_entity), none());
    assert_that!(world.get::<OtherAuraCache>(copy_entity), none());
    assert_that!(
        world.get::<Controller>(copy_entity),
        eq(Some(&Controller(PlayerId::Two)))
    );
    assert_that!(
        crate::zone::semantic_zone_position(world, copy, PlayerId::Two, Zone::Play),
        eq(Some(0))
    );
    assert_that!(world.get::<PlayOrder>(copy_entity), eq(Some(&PlayOrder(0))));
    assert_that!(
        world.iter_entities().any(|entity| {
            entity.get::<AttachedTo>().map(|attached| attached.0) == Some(copy_entity)
        }),
        is_false()
    );
    assert_that!(
        world.resource::<CanonicalTrace>().entries.last(),
        eq(Some(&TraceEntry::EntityCopied {
            source,
            copy,
            policy: CopyStatePolicy::CurrentForm,
        }))
    );
}

#[googletest::test]
fn play_copy_clones_non_aura_state_and_eligible_enchantments() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let source = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Stateful source", 7, 3, 8)
            .with_keyword(Keyword::Charge)
            .with_targeting(TargetRequirement::RequiredIfAvailable(TargetFilter {
                audience: TargetAudience::Either,
                kind: TargetKind::Minion,
            })),
        Zone::Play,
    )
    .unwrap();
    let existing = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Existing destination", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let source_entity = game_entity(simulation.app.world(), source).unwrap();
    assert_eq!(
        copy_card_data(simulation.app.world(), source)
            .unwrap()
            .targeting,
        TargetRequirement::RequiredIfAvailable(TargetFilter {
            audience: TargetAudience::Either,
            kind: TargetKind::Minion,
        })
    );
    let source_order = crate::entity::allocate_play_order(simulation.app.world_mut());
    simulation
        .app
        .world_mut()
        .entity_mut(source_entity)
        .insert(source_order);
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    for effect in [
        Effect::AttachStatModifier {
            targets: Selector::Entity(source),
            modifier: StatModifier {
                attack: 4,
                health: -20,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
        Effect::AttachKeywordModifier {
            targets: Selector::Entity(source),
            modifier: KeywordModifier {
                keyword: Keyword::Taunt,
                granted: true,
                silence_removable: false,
            },
            duration: EnchantmentDuration::EndOfTurn(PlayerId::Two),
        },
        Effect::AttachCostModifier {
            targets: Selector::Entity(source),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -3,
                silence_removable: false,
            },
            duration: EnchantmentDuration::EndOfTurnSeries(PlayerId::One),
        },
        Effect::AttachContinuousEffect {
            targets: Selector::Entity(source),
            effect: ContinuousEffectDefinition {
                recipients: PlayerAudience::Controller,
                modifier: ContinuousModifier::SpellDamage(2),
            },
            silence_removable: false,
            duration: EnchantmentDuration::Permanent,
        },
        Effect::AttachTriggerEnchantment {
            targets: Selector::Entity(source),
            triggers: vec![TriggerDefinition {
                event: EventKind::TurnEnded,
                eligible_zones: vec![Zone::Play],
                conditions: Vec::new(),
                source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
                priority: 3,
                wounded_target_policy: WoundedTargetPolicy::IncludePendingDestroy,
                effect_program: Vec::new(),
            }],
            duration: EnchantmentDuration::Permanent,
            silence_removable: true,
        },
    ] {
        execute_effect(simulation.app.world_mut(), &context, &effect).unwrap();
    }
    let mut source_attachments = simulation
        .app
        .world()
        .iter_entities()
        .filter(|entity| {
            entity.get::<AttachedTo>().map(|attached| attached.0) == Some(source_entity)
        })
        .map(|entity| {
            (
                *entity.get::<PlayOrder>().unwrap(),
                *entity.get::<GameEntityId>().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    source_attachments.sort_by_key(|(order, id)| (order.0, *id));
    let source_attachment_ids = source_attachments
        .iter()
        .map(|(_, id)| *id)
        .collect::<Vec<_>>();
    simulation
        .app
        .world_mut()
        .entity_mut(source_entity)
        .insert((
            Damage(3),
            Silenced,
            PendingDestroy,
            AttackState {
                attacks_this_turn: 2,
                exhausted: false,
            },
            Keywords(std::collections::BTreeSet::from([
                Keyword::Charge,
                Keyword::DivineShield,
                Keyword::Taunt,
            ])),
            AttackAuraCache(vec![AuraApplication {
                provider: GameEntityId(900),
                definition_index: 0,
                modifier: AuraModifier::Attack(5),
            }]),
            HealthAuraCache(vec![AuraApplication {
                provider: GameEntityId(901),
                definition_index: 0,
                modifier: AuraModifier::MaximumHealth(5),
            }]),
            OtherAuraCache(vec![AuraApplication {
                provider: GameEntityId(902),
                definition_index: 0,
                modifier: AuraModifier::Immune,
            }]),
        ));
    let next_id = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    let next_order = simulation
        .app
        .world()
        .resource::<crate::entity::PlayOrderCounter>()
        .0;

    begin_sequence(simulation.app.world_mut()).unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Opponent,
            zone: Zone::Play,
            board_index: Some(0),
        },
    )
    .unwrap();
    drive_resolution(simulation.app.world_mut()).unwrap();
    finish_sequence(simulation.app.world_mut());

    let board = simulation.snapshot().players[1].board.clone();
    assert_that!(board.len(), eq(2));
    assert_that!(board[1], eq(existing));
    let copy = board[0];
    assert_that!(copy, eq(GameEntityId(next_id)));
    let copy_entity = game_entity(simulation.app.world(), copy).unwrap();
    let world = simulation.app.world();
    assert_that!(
        world.get::<Controller>(copy_entity),
        eq(Some(&Controller(PlayerId::Two)))
    );
    assert_that!(
        world.get::<PlayOrder>(copy_entity),
        eq(Some(&PlayOrder(next_order)))
    );
    assert_that!(world.get::<Damage>(copy_entity), eq(Some(&Damage(3))));
    assert_that!(world.get::<Silenced>(copy_entity), some(anything()));
    assert_that!(world.get::<PendingDestroy>(copy_entity), some(anything()));
    assert_that!(
        world.get::<AttackState>(copy_entity),
        eq(Some(&AttackState {
            attacks_this_turn: 0,
            exhausted: true,
        }))
    );
    assert_that!(world.get::<AttackAuraCache>(copy_entity), none());
    assert_that!(world.get::<HealthAuraCache>(copy_entity), none());
    assert_that!(world.get::<OtherAuraCache>(copy_entity), none());
    let expected_keywords = std::collections::BTreeSet::from([Keyword::Taunt]);
    assert_that!(
        world.get::<Keywords>(copy_entity).unwrap().0,
        eq(&expected_keywords)
    );
    assert_that!(
        world.get::<CurrentStats>(copy_entity),
        eq(Some(&CurrentStats {
            attack: 7,
            maximum_health: 0,
        }))
    );
    assert_that!(world.get::<CardRuntime>(copy_entity).unwrap().cost, eq(4));
    assert_eq!(
        world.get::<CardRuntime>(copy_entity).unwrap().targeting,
        TargetRequirement::RequiredIfAvailable(TargetFilter {
            audience: TargetAudience::Either,
            kind: TargetKind::Minion,
        })
    );

    let mut copy_attachments = world
        .iter_entities()
        .filter(|entity| entity.get::<AttachedTo>().map(|attached| attached.0) == Some(copy_entity))
        .map(|entity| {
            (
                *entity.get::<PlayOrder>().unwrap(),
                *entity.get::<GameEntityId>().unwrap(),
                entity.id(),
            )
        })
        .collect::<Vec<_>>();
    copy_attachments.sort_by_key(|(order, id, _)| (order.0, *id));
    assert_that!(copy_attachments.len(), eq(5));
    let expected_attachment_ids = (next_id + 1..=next_id + 5)
        .map(GameEntityId)
        .collect::<Vec<_>>();
    let copy_attachment_ids = copy_attachments
        .iter()
        .map(|(_, id, _)| *id)
        .collect::<Vec<_>>();
    assert_that!(copy_attachment_ids, eq(&expected_attachment_ids));
    assert_that!(copy_attachment_ids == source_attachment_ids, is_false());
    let copied_entities = copy_attachments
        .iter()
        .map(|(_, _, entity)| *entity)
        .collect::<Vec<_>>();
    assert_that!(
        world.get::<StatModifier>(copied_entities[0]),
        some(anything())
    );
    assert_that!(
        world.get::<KeywordModifier>(copied_entities[1]),
        some(anything())
    );
    assert_that!(
        world.get::<CostModifier>(copied_entities[2]),
        some(anything())
    );
    assert_that!(
        world.get::<RuntimeContinuousEffects>(copied_entities[3]),
        some(anything())
    );
    assert_that!(
        world.get::<RuntimeTriggers>(copied_entities[4]),
        some(anything())
    );
    assert_that!(
        world.get::<SilenceRemovable>(copied_entities[4]),
        some(anything())
    );
    assert_that!(
        crate::aura::current_spell_damage(world, PlayerId::Two),
        eq(2)
    );

    let checkpoint = simulation.checkpoint().unwrap();
    let copied_attachment_ids = copy_attachment_ids;
    assert_that!(
        checkpoint
            .entities
            .iter()
            .filter(|entity| entity.attached_to == Some(copy))
            .map(|entity| entity.id)
            .collect::<Vec<_>>(),
        eq(&copied_attachment_ids)
    );
    let json = checkpoint.to_json().unwrap();
    let restored =
        Simulation::from_checkpoint(SimulationCheckpoint::from_json(&json).unwrap()).unwrap();
    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
}

#[googletest::test]
fn invalid_play_copy_attachment_is_atomic_and_consumes_no_id() {
    let mut simulation = simulation();
    let source = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Malformed source", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    attach_stat_modifier(
        simulation.app.world_mut(),
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
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<StatModifier>().is_some())
        .unwrap()
        .id();
    simulation
        .app
        .world_mut()
        .entity_mut(enchantment)
        .remove::<DisplayName>();
    let before = simulation.snapshot();
    let next_id = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    let context = EffectContext {
        source: None,
        controller: PlayerId::One,
        declared_target: None,
        drawn_card: None,
        origin: EffectOrigin::Other,
    };
    begin_sequence(simulation.app.world_mut()).unwrap();
    execute_effect(
        simulation.app.world_mut(),
        &context,
        &Effect::Copy {
            targets: Selector::Entity(source),
            player: PlayerSelector::Controller,
            zone: Zone::Play,
            board_index: None,
        },
    )
    .unwrap();

    assert_that!(
        drive_resolution(simulation.app.world_mut()),
        err(matches_pattern!(SimulationError::Invariant(_)))
    );
    let after = simulation.snapshot();
    assert_that!(after.players, eq(&before.players));
    assert_that!(after.objects, eq(&before.objects));
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::NextGameEntityId>()
            .0,
        eq(next_id)
    );
}

#[googletest::test]
fn in_play_copy_policy_rejects_non_play_destinations_directly_and_after_restore() {
    let mut simulation = simulation();
    let source = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Play source", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let request = CopyRequest {
        source,
        originating_source: None,
        controller: PlayerId::Two,
        destination: Zone::Hand,
        board_index: None,
        policy: CopyStatePolicy::InPlayState,
    };
    let next_id = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    let next_order = simulation
        .app
        .world()
        .resource::<crate::entity::PlayOrderCounter>()
        .0;
    let trace = simulation.trace().to_vec();

    assert_that!(
        copy_entity(simulation.app.world_mut(), request),
        err(matches_pattern!(SimulationError::Invariant(_)))
    );
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::NextGameEntityId>()
            .0,
        eq(next_id)
    );
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::PlayOrderCounter>()
            .0,
        eq(next_order)
    );
    assert_that!(simulation.trace(), eq(trace.as_slice()));
    assert_that!(simulation.snapshot().players[1].hand, is_empty());

    let mut checkpoint = simulation.checkpoint().unwrap();
    let operation_id = checkpoint.resolution.next_resolution_id;
    checkpoint.resolution.next_resolution_id += 1;
    checkpoint.resolution.stack.push(StackedResolutionOp {
        id: ResolutionId(operation_id),
        operation: ResolutionOp::CopyEntity(request),
    });
    checkpoint.resolution.remaining_budget = checkpoint.ruleset.resolution_budget;
    checkpoint.resolution.sequence_active = true;
    checkpoint.game.status = SimulationStatus::Resolving;
    assert_that!(
        Simulation::from_checkpoint(checkpoint).map(|_| ()),
        err(matches_pattern!(SimulationError::Invariant(
            contains_substring("requires CurrentForm policy")
        )))
    );
}

#[googletest::test]
fn copy_operations_use_the_sources_current_zone_after_nested_work() {
    let mut simulation = simulation();
    let first = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("First source", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let second = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Moved source", 0, 2, 2),
        Zone::Play,
    )
    .unwrap();
    let first_entity = game_entity(simulation.app.world(), first).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<RuntimeTriggers>(first_entity)
        .unwrap()
        .0
        .push(TriggerDefinition {
            event: EventKind::Summoned,
            eligible_zones: vec![Zone::Play],
            conditions: Vec::new(),
            source_eligibility: SourceEligibilityPolicy::MustRemainInEligibleZone,
            priority: 0,
            wounded_target_policy: WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: vec![Effect::Move {
                targets: Selector::Entity(second),
                player: PlayerSelector::Controller,
                zone: Zone::Hand,
                kind: ZoneMovementKind::Normal,
            }],
        });
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
        &Effect::Copy {
            targets: Selector::FriendlyMinions,
            player: PlayerSelector::Controller,
            zone: Zone::Play,
            board_index: None,
        },
    )
    .unwrap();

    drive_resolution(world).unwrap();

    assert_that!(
        world.get::<Zone>(game_entity(world, second).unwrap()),
        eq(Some(&Zone::Hand))
    );
    assert_that!(
        world.resource::<CanonicalTrace>().entries.iter().any(
            |entry| matches!(entry, TraceEntry::EntityCopied {
                source,
                policy: CopyStatePolicy::CurrentForm,
                ..
            } if *source == second)
        ),
        is_true()
    );
}

#[googletest::test]
fn full_copy_destination_precedes_source_program_and_attachment_validation() {
    let mut simulation = simulation();
    let source = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Malformed source", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    attach_stat_modifier(
        simulation.app.world_mut(),
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
    let source_entity = game_entity(simulation.app.world(), source).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<CardRuntime>(source_entity)
        .unwrap()
        .program = vec![Effect::Native(NativeEffectId::new("missing:full_copy"))];
    let enchantment = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| entity.get::<StatModifier>().is_some())
        .unwrap()
        .id();
    simulation
        .app
        .world_mut()
        .entity_mut(enchantment)
        .remove::<DisplayName>();
    for index in 0..7 {
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::Two,
            Card::minion(format!("Full board {index}"), 0, 1, 1),
            Zone::Play,
        )
        .unwrap();
    }
    let before = simulation.snapshot();
    let next_id = simulation
        .app
        .world()
        .resource::<crate::entity::NextGameEntityId>()
        .0;
    let next_order = simulation
        .app
        .world()
        .resource::<crate::entity::PlayOrderCounter>()
        .0;
    let trace = simulation.trace().to_vec();

    assert_that!(
        copy_entity(
            simulation.app.world_mut(),
            CopyRequest {
                source,
                originating_source: None,
                controller: PlayerId::Two,
                destination: Zone::Play,
                board_index: None,
                policy: CopyStatePolicy::InPlayState,
            },
        ),
        ok(anything())
    );

    let after = simulation.snapshot();
    assert_that!(after.players, eq(&before.players));
    assert_that!(after.objects, eq(&before.objects));
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::NextGameEntityId>()
            .0,
        eq(next_id)
    );
    assert_that!(
        simulation
            .app
            .world()
            .resource::<crate::entity::PlayOrderCounter>()
            .0,
        eq(next_order)
    );
    assert_that!(simulation.trace(), eq(trace.as_slice()));
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
