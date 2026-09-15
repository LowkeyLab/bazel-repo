use googletest::prelude::*;

use super::{card_runtime::CardRuntime, test_support::*, *};
use crate::{
    AttackState, AuraDefinition, AuraModifier, AuraTarget, EnchantmentDuration, HeroClass,
    HeroClassPolicy, HeroHealthPolicy, HeroPowerState, HeroReplacement, OtherAuraCache,
    OtherAuraModifier,
};

fn replacement(
    health: HeroHealthPolicy,
    class: HeroClassPolicy,
    weapon: Option<Card>,
) -> HeroReplacement {
    HeroReplacement {
        hero: Card::hero("Archmage", 30),
        hero_power: Card::hero_power("Arcane Spark", 2),
        armor_gain: 5,
        health,
        class,
        weapon,
    }
}

#[googletest::test]
fn hero_replacement_preserves_combat_state_and_refreshes_the_power() {
    let replacement = replacement(
        HeroHealthPolicy::Preserve,
        HeroClassPolicy::Replace(HeroClass::Mage),
        None,
    );
    let card = Card::spell("Ascend", 0).with_effects(vec![Effect::ReplaceHero {
        player: PlayerSelector::Controller,
        replacement: Box::new(replacement),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![card]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let before = simulation.snapshot();
    let old_hero = before.players[0].hero;
    let old_power = before.players[0].hero_power.unwrap();
    assert_that!(
        before
            .objects
            .iter()
            .find(|object| object.id == old_power)
            .unwrap()
            .zone,
        eq(Zone::Play)
    );
    assert_that!(before.players[0].board.contains(&old_power), is_false());
    let hero_entity = game_entity(simulation.app.world(), old_hero).unwrap();
    simulation.app.world_mut().entity_mut(hero_entity).insert((
        Damage(5),
        Armor(3),
        AttackState {
            attacks_this_turn: 1,
            readiness_blocked: true,
        },
        Keywords(std::collections::BTreeSet::from([Keyword::Frozen])),
    ));
    let power_entity = game_entity(simulation.app.world(), old_power).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(power_entity)
        .insert(HeroPowerState {
            uses_this_turn: 1,
            exhausted: true,
        });
    attach_stat_modifier(
        simulation.app.world_mut(),
        PlayerId::One,
        old_hero,
        StatModifier {
            attack: 2,
            health: 2,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )
    .unwrap();
    let weapon = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::weapon("Old Blade", 0, 3, 2),
        Zone::Play,
    )
    .unwrap();
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

    let snapshot = simulation.snapshot();
    let player = &snapshot.players[0];
    assert_that!(player.hero == old_hero, is_false());
    assert_that!(player.hero_power == Some(old_power), is_false());
    assert_that!(player.health, eq(27));
    assert_that!(player.armor, eq(8));
    assert_that!(player.hero_class, eq(HeroClass::Mage));
    let hero = snapshot
        .objects
        .iter()
        .find(|object| object.id == player.hero)
        .unwrap();
    assert_that!(hero.maximum_health, eq(Some(32)));
    assert_that!(hero.damage, eq(5));
    assert_that!(hero.exhausted, eq(Some(true)));
    let hero_entity = game_entity(simulation.app.world(), player.hero).unwrap();
    assert_that!(
        simulation
            .app
            .world()
            .get::<Keywords>(hero_entity)
            .unwrap()
            .0
            .contains(&Keyword::Frozen),
        is_false()
    );
    let power_entity = game_entity(simulation.app.world(), player.hero_power.unwrap()).unwrap();
    assert_that!(
        simulation.app.world().get::<HeroPowerState>(power_entity),
        eq(Some(&HeroPowerState::default()))
    );
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == old_hero)
            .unwrap()
            .zone,
        eq(Zone::SetAside)
    );
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == old_power)
            .unwrap()
            .zone,
        eq(Zone::RemovedFromGame)
    );
    assert_that!(
        simulation
            .snapshot()
            .objects
            .iter()
            .find(|object| object.id == weapon)
            .unwrap()
            .zone,
        eq(Zone::Play)
    );
}

#[googletest::test]
fn hero_power_replacement_recalculates_cost_after_detaching_modifiers() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let old_power = simulation.snapshot().players[0].hero_power.unwrap();
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
            targets: Selector::Entity(old_power),
            modifier: CostModifier {
                operation: CostOperation::Add,
                value: -1,
                silence_removable: false,
            },
            duration: EnchantmentDuration::Permanent,
        },
    )
    .unwrap();

    replace_hero(
        simulation.app.world_mut(),
        PlayerId::One,
        &replacement(HeroHealthPolicy::Preserve, HeroClassPolicy::Keep, None),
    )
    .unwrap();

    let old_power = game_entity(simulation.app.world(), old_power).unwrap();
    let runtime = simulation
        .app
        .world()
        .get::<CardRuntime>(old_power)
        .unwrap();
    assert_that!(runtime.cost, eq(runtime.base_cost));
}

#[googletest::test]
fn replacement_can_override_health_and_equip_a_new_weapon() {
    let replacement = replacement(
        HeroHealthPolicy::Set {
            maximum_health: 8,
            current_health: 8,
        },
        HeroClassPolicy::Keep,
        Some(Card::weapon("Doomhammer Fixture", 0, 5, 2)),
    );
    let card = Card::spell("Become Ragnaros", 0).with_effects(vec![Effect::ReplaceHero {
        player: PlayerSelector::Controller,
        replacement: Box::new(replacement),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![card]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let old_weapon = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::weapon("Old Weapon", 0, 2, 2),
        Zone::Play,
    )
    .unwrap();
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

    let snapshot = simulation.snapshot();
    assert_that!(snapshot.players[0].health, eq(8));
    assert_that!(snapshot.players[0].hero_class, eq(HeroClass::Neutral));
    assert_that!(
        snapshot
            .objects
            .iter()
            .find(|object| object.id == old_weapon)
            .unwrap()
            .zone,
        eq(Zone::Graveyard)
    );
    assert_that!(
        snapshot
            .objects
            .iter()
            .any(|object| { object.name == "Doomhammer Fixture" && object.zone == Zone::Play }),
        is_true()
    );
}

#[googletest::test]
fn replacing_a_mortally_wounded_hero_before_death_creation_prevents_defeat() {
    let replacement = replacement(
        HeroHealthPolicy::Set {
            maximum_health: 8,
            current_health: 8,
        },
        HeroClassPolicy::Keep,
        None,
    );
    let card = Card::spell("Last Second Rescue", 0).with_effects(vec![Effect::Sequence(vec![
        Effect::DealDamage {
            targets: Selector::FriendlyCharacters,
            amount: ValueExpression::Constant(30),
        },
        Effect::ReplaceHero {
            player: PlayerSelector::Controller,
            replacement: Box::new(replacement),
        },
    ])]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![card]),
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

    assert_that!(simulation.snapshot().game.outcome, none());
    assert_that!(simulation.snapshot().players[0].health, eq(8));
}

#[googletest::test]
fn replacement_after_death_creation_cannot_clear_defeat() {
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", Vec::new()),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let hero = hero(&mut simulation, PlayerId::One);
    let hero_entity = game_entity(simulation.app.world(), hero).unwrap();
    simulation
        .app
        .world_mut()
        .entity_mut(hero_entity)
        .insert(Damage(30));
    crate::death::create_deaths(simulation.app.world_mut());
    replace_hero(
        simulation.app.world_mut(),
        PlayerId::One,
        &replacement(
            HeroHealthPolicy::Set {
                maximum_health: 8,
                current_health: 8,
            },
            HeroClassPolicy::Keep,
            None,
        ),
    )
    .unwrap();
    check_outcome(simulation.app.world_mut());

    assert_that!(
        simulation.snapshot().game.outcome,
        some(eq(GameOutcome::Winner(PlayerId::Two)))
    );
}

#[googletest::test]
fn hero_replacement_waits_for_the_phase_boundary_before_refreshing_auras() {
    let replacement = HeroReplacement {
        hero: Card::hero("Immune replacement", 30).with_aura(AuraDefinition {
            targets: AuraTarget::FriendlyMinions,
            attack: 0,
            health: 0,
            other: vec![OtherAuraModifier::Immune],
        }),
        hero_power: Card::hero_power("Replacement power", 2),
        armor_gain: 0,
        health: HeroHealthPolicy::Preserve,
        class: HeroClassPolicy::Keep,
        weapon: None,
    };
    let replace_then_damage = Card::spell("Replace then damage", 0).with_effects(vec![
        Effect::ReplaceHero {
            player: PlayerSelector::Controller,
            replacement: Box::new(replacement),
        },
        Effect::DealDamage {
            targets: Selector::FriendlyMinions,
            amount: ValueExpression::Constant(1),
        },
    ]);
    let mut simulation = Simulation::new([
        PlayerConfig::new(
            "Jaina",
            vec![
                Card::minion("Aura timing target", 0, 1, 3),
                replace_then_damage,
            ],
        ),
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
    let spell = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: spell,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();

    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    assert_that!(
        simulation.app.world().get::<Damage>(target_entity),
        eq(Some(&Damage(1)))
    );
    assert_that!(
        simulation
            .app
            .world()
            .get::<OtherAuraCache>(target_entity)
            .unwrap()
            .0
            .iter()
            .any(|application| application.modifier == AuraModifier::Immune),
        is_true()
    );
}

#[googletest::test]
fn invalid_hero_replacements_are_rejected_before_mutation() {
    let invalid_replacements = [
        HeroReplacement {
            hero: Card::minion("Not A Hero", 0, 1, 1),
            hero_power: Card::hero_power("Power", 2),
            armor_gain: 0,
            health: HeroHealthPolicy::Preserve,
            class: HeroClassPolicy::Keep,
            weapon: None,
        },
        HeroReplacement {
            hero: Card::hero("Hero", 30),
            hero_power: Card::spell("Not A Power", 2),
            armor_gain: 0,
            health: HeroHealthPolicy::Preserve,
            class: HeroClassPolicy::Keep,
            weapon: None,
        },
        HeroReplacement {
            hero: Card::hero("Hero", 30),
            hero_power: Card::hero_power("Power", 2),
            armor_gain: 0,
            health: HeroHealthPolicy::Preserve,
            class: HeroClassPolicy::Keep,
            weapon: Some(Card::minion("Not A Weapon", 0, 1, 1)),
        },
        HeroReplacement {
            hero: Card::hero("Hero", 30),
            hero_power: Card::hero_power("Power", 2),
            armor_gain: 0,
            health: HeroHealthPolicy::Set {
                maximum_health: 5,
                current_health: 6,
            },
            class: HeroClassPolicy::Keep,
            weapon: None,
        },
    ];

    for invalid in invalid_replacements {
        let card = Card::spell("Invalid", 0).with_effects(vec![Effect::ReplaceHero {
            player: PlayerSelector::Controller,
            replacement: Box::new(invalid),
        }]);
        let mut simulation = Simulation::new([
            PlayerConfig::new("Jaina", vec![card]),
            PlayerConfig::new("Rexxar", Vec::new()),
        ]);
        let before = simulation.snapshot();
        let card = hand_card(&mut simulation, PlayerId::One);

        assert_that!(
            simulation.apply(GameAction::PlayCard {
                player: PlayerId::One,
                card,
                target: None,
                board_index: None,
                choice: None,
            }),
            err(matches_pattern!(SimulationError::InvalidHeroReplacement(_)))
        );
        assert_that!(simulation.snapshot(), eq(&before));
    }
}

fn install_power(simulation: &mut Simulation, power: Card) -> GameEntityId {
    let card = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::spell("Install power", 0).with_effects(vec![power_replacement(power)]),
        Zone::Hand,
    )
    .unwrap();
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    simulation.snapshot().players[0].hero_power.unwrap()
}

fn power_replacement(power: Card) -> Effect {
    Effect::ReplaceHero {
        player: PlayerSelector::Controller,
        replacement: Box::new(HeroReplacement {
            hero: Card::hero("Power test hero", 30),
            hero_power: power,
            armor_gain: 0,
            health: HeroHealthPolicy::Preserve,
            class: HeroClassPolicy::Keep,
            weapon: None,
        }),
    }
}

fn activate(power: GameEntityId, target: Option<GameEntityId>) -> GameAction {
    GameAction::UseHeroPower {
        player: PlayerId::One,
        power,
        target,
    }
}

fn power_state(simulation: &Simulation, power: GameEntityId) -> HeroPowerState {
    *simulation
        .app
        .world()
        .get::<HeroPowerState>(game_entity(simulation.app.world(), power).unwrap())
        .unwrap()
}

fn power_gain(amount: i32) -> Effect {
    Effect::GainResource {
        player: PlayerSelector::Controller,
        amount,
        temporary: true,
    }
}

fn after_power_trigger(effects: Vec<Effect>) -> crate::TriggerDefinition {
    crate::TriggerDefinition {
        event: EventKind::AfterHeroPower,
        eligible_zones: vec![Zone::Play],
        conditions: vec![],
        source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::IncludeMortallyWounded,
        effect_program: effects,
    }
}

#[test]
fn hero_power_pays_before_effects_exhausts_after_effects_and_refreshes_next_turn() {
    let mut simulation = simulation();
    simulation
        .register_native_effect(
            "inspect_activation",
            |context: &EffectContext, world: &World| {
                let source = game_entity(world, context.source.unwrap()).unwrap();
                assert_eq!(
                    world.get::<HeroPowerState>(source).unwrap(),
                    &HeroPowerState::default()
                );
                assert_eq!(
                    player(world, context.controller)
                        .unwrap()
                        .1
                        .available_resources(),
                    0
                );
                vec![power_gain(2)]
            },
        )
        .unwrap();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Refresh", 1)
            .with_effects(vec![Effect::Native("inspect_activation".into())]),
    );
    simulation.apply(activate(power, None)).unwrap();
    assert_eq!(simulation.snapshot().players[0].available_resources, 2);
    assert_eq!(
        power_state(&simulation, power),
        HeroPowerState {
            uses_this_turn: 1,
            exhausted: true
        }
    );
    let before = simulation.checkpoint().unwrap();
    assert_eq!(
        simulation.apply(activate(power, None)),
        Err(SimulationError::HeroPowerExhausted(power))
    );
    let mut after = simulation.checkpoint().unwrap();
    assert!(matches!(
        after.trace.entries.pop(),
        Some(TraceEntry::ActionRejected { .. })
    ));
    assert_eq!(after, before);
    for player in PlayerId::ALL {
        simulation.apply(GameAction::EndTurn { player }).unwrap();
    }
    assert_eq!(power_state(&simulation, power), HeroPowerState::default());
    assert!(simulation.legal_actions().contains(&activate(power, None)));
}

#[test]
fn hero_power_damage_uses_its_own_modifier_and_captured_source() {
    let mut simulation = simulation();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Spark", 1)
            .with_targeting(crate::TargetRequirement::Required(crate::TargetFilter {
                audience: crate::TargetAudience::Enemy,
                kind: crate::TargetKind::Character,
            }))
            .with_effects(vec![Effect::DealDamage {
                targets: Selector::DeclaredTarget,
                amount: ValueExpression::Constant(1),
            }]),
    );
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Spell bonus", 0, 0, 2).with_spell_damage(7),
        Zone::Play,
    )
    .unwrap();
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Power bonus", 0, 0, 2).with_aura(AuraDefinition {
            targets: AuraTarget::ControllerPlayer,
            attack: 0,
            health: 0,
            other: vec![OtherAuraModifier::HeroPowerDamage(3)],
        }),
        Zone::Play,
    )
    .unwrap();
    crate::aura::refresh_all_auras(simulation.app.world_mut());
    let target = hero(&mut simulation, PlayerId::Two);
    simulation.apply(activate(power, Some(target))).unwrap();
    assert_eq!(simulation.snapshot().players[1].health, 26);
    assert!(simulation.trace().iter().any(|entry| matches!(entry,
        TraceEntry::Damage { source: Some(source), target: damaged, actual: 4, .. }
        if *source == power && *damaged == target)));
}

#[test]
fn hero_power_validation_and_enumeration_agree_without_mutation() {
    use crate::{TargetAudience, TargetFilter, TargetKind, TargetRequirement};
    let filter = TargetFilter {
        audience: TargetAudience::Enemy,
        kind: TargetKind::Minion,
    };
    for requirement in [
        TargetRequirement::None,
        TargetRequirement::Required(filter),
        TargetRequirement::Optional(filter),
        TargetRequirement::RequiredIfAvailable(filter),
    ] {
        for has_target in [false, true] {
            let mut simulation = simulation();
            let power = install_power(
                &mut simulation,
                Card::hero_power("Options", 0).with_targeting(requirement),
            );
            let target = spawn_card(
                simulation.app.world_mut(),
                PlayerId::Two,
                Card::minion("Candidate", 0, 1, 1),
                if has_target { Zone::Play } else { Zone::Hand },
            )
            .unwrap();
            let checkpoint = simulation.checkpoint().unwrap();
            let legal = simulation.legal_actions();
            assert_eq!(simulation.legal_actions(), legal);
            assert_eq!(simulation.checkpoint().unwrap(), checkpoint);
            let absent_valid = match requirement {
                TargetRequirement::None | TargetRequirement::Optional(_) => true,
                TargetRequirement::Required(_) => false,
                TargetRequirement::RequiredIfAvailable(_) => !has_target,
            };
            let target_valid = has_target && requirement != TargetRequirement::None;
            let expected = [(None, absent_valid), (Some(target), target_valid)];
            let expected_actions = expected
                .iter()
                .filter(|(_, valid)| *valid)
                .map(|(target, _)| activate(power, *target))
                .collect::<Vec<_>>();
            assert_eq!(
                legal
                    .iter()
                    .filter(|action| matches!(action, GameAction::UseHeroPower { .. }))
                    .cloned()
                    .collect::<Vec<_>>(),
                expected_actions
            );
            for (target, valid) in expected {
                let mut fork = simulation.fork().unwrap();
                assert_eq!(fork.apply(activate(power, target)).is_ok(), valid);
            }
            for action in legal {
                assert!(
                    super::action_validation::validate_action(simulation.app.world(), &action)
                        .is_ok()
                );
            }
        }
    }
}

#[test]
fn invalid_hero_power_declarations_preserve_checkpoint_except_rejection_trace() {
    let mut simulation = simulation();
    let power = install_power(&mut simulation, Card::hero_power("Power", 0));
    let snapshot = simulation.snapshot();
    let enemy_power = snapshot.players[1].hero_power.unwrap();
    let hand = snapshot.players[0].hand[0];
    let missing = GameEntityId(u64::MAX);
    for (action, error) in [
        (
            activate(missing, None),
            SimulationError::EntityNotFound(missing),
        ),
        (
            activate(snapshot.players[0].hero, None),
            SimulationError::NotActiveHeroPower(snapshot.players[0].hero),
        ),
        (
            activate(enemy_power, None),
            SimulationError::NotControlled {
                entity: enemy_power,
            },
        ),
        (
            activate(hand, None),
            SimulationError::WrongZone {
                entity: hand,
                expected: Zone::Play,
            },
        ),
        (
            activate(power, Some(missing)),
            SimulationError::UnexpectedTarget(power),
        ),
        (
            GameAction::UseHeroPower {
                player: PlayerId::Two,
                power: enemy_power,
                target: None,
            },
            SimulationError::NotPlayersTurn(PlayerId::Two),
        ),
    ] {
        let before = simulation.checkpoint().unwrap();
        assert_eq!(simulation.apply(action), Err(error));
        let mut after = simulation.checkpoint().unwrap();
        assert!(matches!(
            after.trace.entries.pop(),
            Some(TraceEntry::ActionRejected { .. })
        ));
        assert_eq!(after, before);
    }
    let expensive = install_power(&mut simulation, Card::hero_power("Expensive", 2));
    assert_eq!(
        simulation.apply(activate(expensive, None)),
        Err(SimulationError::NotEnoughMana {
            player: PlayerId::One,
            required: 2,
            available: 1,
        })
    );
    assert_eq!(
        simulation.apply(activate(power, None)),
        Err(SimulationError::WrongZone {
            entity: power,
            expected: Zone::Play
        })
    );
    for cost in [-2, 0] {
        let free = install_power(&mut simulation, Card::hero_power("Free", cost));
        let before = simulation.snapshot().players[0].available_resources;
        simulation.apply(activate(free, None)).unwrap();
        assert_eq!(simulation.snapshot().players[0].available_resources, before);
    }
}

#[test]
fn after_hero_power_uses_initial_seeds_and_runs_after_deaths() {
    let mut simulation = simulation();
    let observer = Card::minion("Observer", 0, 0, 1)
        .with_triggers(vec![after_power_trigger(vec![power_gain(1)])]);
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        observer.clone(),
        Zone::Play,
    )
    .unwrap();
    let removed = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        observer.clone(),
        Zone::Play,
    )
    .unwrap();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Summon observer", 0).with_effects(vec![
            Effect::Destroy {
                targets: Selector::Entity(removed),
            },
            Effect::Summon {
                player: PlayerSelector::Controller,
                card: observer,
                board_index: None,
            },
        ]),
    );
    simulation.apply(activate(power, None)).unwrap();
    assert_eq!(simulation.snapshot().players[0].temporary_resources, 1);
    assert!(
        simulation
            .snapshot()
            .deaths
            .iter()
            .any(|death| death.entity == removed)
    );
    assert!(
        simulation
            .checkpoint()
            .unwrap()
            .resolution
            .events
            .is_empty()
    );
}

#[test]
fn replacing_hero_power_during_effects_or_after_use_keeps_new_power_ready() {
    for during_effects in [false, true] {
        let mut simulation = simulation();
        let replacement = power_replacement(Card::hero_power("New power", 0));
        let effects = if during_effects {
            vec![replacement.clone(), power_gain(2)]
        } else {
            vec![power_gain(2)]
        };
        let power = install_power(
            &mut simulation,
            Card::hero_power("Old power", 0).with_effects(effects),
        );
        spawn_card(
            simulation.app.world_mut(),
            PlayerId::One,
            Card::minion("After-use observer", 0, 0, 2).with_triggers(vec![after_power_trigger(
                if during_effects {
                    vec![power_gain(1)]
                } else {
                    vec![replacement, power_gain(1)]
                },
            )]),
            Zone::Play,
        )
        .unwrap();
        simulation.apply(activate(power, None)).unwrap();
        let new_power = simulation.snapshot().players[0].hero_power.unwrap();
        assert_ne!(new_power, power);
        assert_eq!(
            power_state(&simulation, new_power),
            HeroPowerState::default()
        );
        assert_eq!(
            power_state(&simulation, power),
            HeroPowerState {
                uses_this_turn: 1,
                exhausted: true
            }
        );
        assert_eq!(simulation.snapshot().players[0].temporary_resources, 3);
        assert!(
            simulation
                .legal_actions()
                .contains(&activate(new_power, None))
        );
    }
}

#[test]
fn lethal_hero_power_still_runs_after_use_before_outcome() {
    let mut simulation = simulation();
    let target = hero(&mut simulation, PlayerId::Two);
    let power = install_power(
        &mut simulation,
        Card::hero_power("Lethal", 0).with_effects(vec![Effect::DealDamage {
            targets: Selector::Entity(target),
            amount: ValueExpression::Constant(30),
        }]),
    );
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("After lethal", 0, 0, 2)
            .with_triggers(vec![after_power_trigger(vec![power_gain(3)])]),
        Zone::Play,
    )
    .unwrap();
    simulation.apply(activate(power, None)).unwrap();
    assert_eq!(simulation.snapshot().players[0].temporary_resources, 3);
    assert_eq!(
        simulation.snapshot().game.outcome,
        Some(GameOutcome::Winner(PlayerId::One))
    );
    simulation.assert_invariants().unwrap();
}

#[test]
fn suspended_hero_power_restores_original_subject_target_and_seed_set() {
    let mut simulation = simulation();
    simulation
        .register_native_effect("pause_activation", |_: &EffectContext, _: &World| {
            vec![Effect::Choose {
                id: ChoiceId(40),
                player: PlayerSelector::Controller,
                options: vec![hearthstone_simulator_core::EffectChoiceOption {
                    id: ChoiceId(41),
                    effects: vec![],
                }],
            }]
        })
        .unwrap();
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Moved target", 0, 1, 1),
        Zone::Play,
    )
    .unwrap();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Suspended", 0)
            .with_targeting(crate::TargetRequirement::Required(crate::TargetFilter {
                audience: crate::TargetAudience::Enemy,
                kind: crate::TargetKind::Minion,
            }))
            .with_effects(vec![
                Effect::Move {
                    targets: Selector::DeclaredTarget,
                    player: PlayerSelector::Opponent,
                    zone: Zone::Hand,
                    kind: crate::ZoneMovementKind::Normal,
                },
                power_replacement(Card::hero_power("Fresh", 0)),
                Effect::Native("pause_activation".into()),
                power_gain(2),
            ]),
    );
    let observer = spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Retained observer", 0, 0, 2)
            .with_triggers(vec![after_power_trigger(vec![power_gain(1)])]),
        Zone::Play,
    )
    .unwrap();
    simulation.apply(activate(power, Some(target))).unwrap();
    assert_eq!(
        simulation.snapshot().game.status,
        SimulationStatus::AwaitingChoice
    );
    assert!(simulation.legal_actions().is_empty());
    assert_eq!(power_state(&simulation, power), HeroPowerState::default());
    let checkpoint = simulation.checkpoint().unwrap();
    let event = checkpoint
        .resolution
        .events
        .values()
        .find(|event| event.context.kind == EventKind::AfterHeroPower)
        .unwrap();
    assert_eq!(event.context.source, Some(power));
    assert_eq!(event.context.targets, vec![target]);
    assert!(
        event
            .prechecked_triggers
            .as_ref()
            .unwrap()
            .iter()
            .any(|seed| seed.source == observer)
    );
    let mut fork = simulation.fork().unwrap();
    let mut restored = simulation.fork().unwrap();
    restored
        .restore(SimulationCheckpoint::from_json(&checkpoint.to_json().unwrap()).unwrap())
        .unwrap();
    for candidate in [&mut simulation, &mut fork, &mut restored] {
        candidate.choose(ChoiceId(41)).unwrap();
        assert_eq!(candidate.snapshot().players[0].temporary_resources, 3);
        candidate.assert_invariants().unwrap();
    }
    assert_eq!(simulation.checkpoint().unwrap(), fork.checkpoint().unwrap());
    assert_eq!(
        simulation.checkpoint().unwrap(),
        restored.checkpoint().unwrap()
    );
    for corrupt_source in [true, false] {
        let mut invalid = checkpoint.clone();
        if corrupt_source {
            let step = invalid
                .resolution
                .stack
                .iter_mut()
                .find_map(|op| match &mut op.operation {
                    ResolutionOp::RunSequenceStep(crate::SequenceStep::FinishHeroPower {
                        power,
                    }) => Some(power),
                    _ => None,
                })
                .unwrap();
            *step = GameEntityId(u64::MAX);
        } else {
            let event = invalid
                .resolution
                .events
                .values_mut()
                .find(|event| event.context.kind == EventKind::AfterHeroPower)
                .unwrap();
            event.prechecked_triggers.as_mut().unwrap()[0].source = GameEntityId(u64::MAX);
        }
        assert!(restored.restore(invalid).is_err());
    }
}

#[test]
fn hero_power_precheck_conditions_are_evaluated_before_effects() {
    let mut simulation = simulation();
    let mut trigger = after_power_trigger(vec![power_gain(10)]);
    trigger.conditions.push(crate::TimedCondition {
        timing: crate::ConditionTiming::PreCheck,
        condition: crate::TriggerCondition::MinimumEntityCount {
            selector: Selector::EnemyMinions,
            count: 1,
        },
    });
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Initially ineligible", 0, 0, 2).with_triggers(vec![trigger]),
        Zone::Play,
    )
    .unwrap();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Enable observer", 0).with_effects(vec![Effect::Summon {
            player: PlayerSelector::Opponent,
            card: Card::minion("Enabler", 0, 0, 2),
            board_index: None,
        }]),
    );
    simulation.apply(activate(power, None)).unwrap();
    assert_eq!(simulation.snapshot().players[0].temporary_resources, 0);
}

#[test]
fn hero_power_target_errors_are_atomic_and_exhaustion_is_the_usage_gate() {
    let mut simulation = simulation();
    let power = install_power(
        &mut simulation,
        Card::hero_power("Targeted", 0).with_targeting(crate::TargetRequirement::Required(
            crate::TargetFilter {
                audience: crate::TargetAudience::Enemy,
                kind: crate::TargetKind::Minion,
            },
        )),
    );
    let target = spawn_card(
        simulation.app.world_mut(),
        PlayerId::Two,
        Card::minion("Valid", 0, 0, 2),
        Zone::Play,
    )
    .unwrap();
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    for invalid in [
        None,
        Some(power),
        Some(enemy_hero),
        Some(GameEntityId(u64::MAX)),
    ] {
        let before = simulation.checkpoint().unwrap();
        let expected = invalid.map_or(SimulationError::MissingTarget(power), |target| {
            SimulationError::InvalidTarget {
                card: power,
                target,
            }
        });
        assert_eq!(simulation.apply(activate(power, invalid)), Err(expected));
        let mut after = simulation.checkpoint().unwrap();
        assert!(matches!(
            after.trace.entries.pop(),
            Some(TraceEntry::ActionRejected { .. })
        ));
        assert_eq!(after, before);
    }
    let entity = game_entity(simulation.app.world(), power).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<HeroPowerState>(entity)
        .unwrap()
        .uses_this_turn = 5;
    simulation.apply(activate(power, Some(target))).unwrap();
    assert_eq!(
        power_state(&simulation, power),
        HeroPowerState {
            uses_this_turn: 6,
            exhausted: true
        }
    );
}

#[test]
fn hero_power_declaration_validates_program_and_game_status() {
    let mut simulation = simulation();
    let power = install_power(&mut simulation, Card::hero_power("Guarded", 0));
    for status in [
        SimulationStatus::Resolving,
        SimulationStatus::AwaitingChoice,
    ] {
        let mut fork = simulation.fork().unwrap();
        fork.app.world_mut().resource_mut::<GameState>().status = status;
        assert_eq!(
            fork.apply(activate(power, None)),
            Err(SimulationError::NotAwaitingAction)
        );
        assert!(fork.legal_actions().is_empty());
    }
    let entity = game_entity(simulation.app.world(), power).unwrap();
    simulation
        .app
        .world_mut()
        .get_mut::<CardRuntime>(entity)
        .unwrap()
        .program = vec![Effect::Native("missing".into())];
    let before = simulation.checkpoint().unwrap();
    assert_eq!(
        simulation.apply(activate(power, None)),
        Err(SimulationError::NativeEffectNotRegistered("missing".into()))
    );
    let mut after = simulation.checkpoint().unwrap();
    after.trace.entries.pop();
    assert_eq!(after, before);
    simulation
        .apply(GameAction::Concede {
            player: PlayerId::One,
        })
        .unwrap();
    assert_eq!(
        simulation.apply(activate(power, None)),
        Err(SimulationError::GameOver)
    );
}

#[test]
fn retained_hero_power_entry_validates_references_and_skips_a_replaced_subject() {
    let mut simulation = simulation();
    let old_power = simulation.snapshot().players[0].hero_power.unwrap();
    install_power(&mut simulation, Card::hero_power("New", 0));
    let target = hero(&mut simulation, PlayerId::Two);
    let world = simulation.app.world_mut();
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    begin_sequence(world).unwrap();
    push_resolution_ops(
        world,
        [
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![crate::SubjectGuard {
                    subject: old_power,
                    required_zone: Zone::Play,
                }],
                step: crate::SequenceStep::UseHeroPower {
                    player: PlayerId::One,
                    power: old_power,
                    target: Some(target),
                },
            },
            ResolutionOp::RequestChoice(ChoiceRequest {
                id: ChoiceId(50),
                player: PlayerId::One,
                options: vec![ChoiceOption {
                    id: ChoiceId(51),
                    operations: vec![],
                }],
            }),
        ],
    );
    let checkpoint = simulation.checkpoint().unwrap();
    for invalid_target in [false, true] {
        let mut invalid = checkpoint.clone();
        let ResolutionOp::RunGuardedSequenceStep {
            step: crate::SequenceStep::UseHeroPower { power, target, .. },
            ..
        } = &mut invalid.resolution.stack.last_mut().unwrap().operation
        else {
            panic!("expected activation");
        };
        if invalid_target {
            *target = Some(GameEntityId(u64::MAX));
        } else {
            *power = GameEntityId(u64::MAX);
        }
        assert!(Simulation::from_checkpoint(invalid).is_err());
    }
    let mut restored = Simulation::from_checkpoint(checkpoint).unwrap();
    drive_resolution(restored.app.world_mut()).unwrap();
    assert!(restored.trace().iter().any(|entry| matches!(entry,
        TraceEntry::SequenceStepSkipped { subject, .. } if *subject == old_power)));
    restored.choose(ChoiceId(51)).unwrap();
    restored.assert_invariants().unwrap();
}

#[test]
fn windfury_hero_replacement_preserves_spent_attacks_and_readiness_with_live_allowance() {
    for windfury in [false, true] {
        for readiness_blocked in [false, true] {
            for attacks_this_turn in [1, 2] {
                let mut simulation = simulation();
                let old = hero(&mut simulation, PlayerId::One);
                let old_entity = game_entity(simulation.app.world(), old).unwrap();
                let state = AttackState {
                    attacks_this_turn,
                    readiness_blocked,
                };
                simulation
                    .app
                    .world_mut()
                    .entity_mut(old_entity)
                    .insert(state);
                let mut replacement =
                    replacement(HeroHealthPolicy::Preserve, HeroClassPolicy::Keep, None);
                if windfury {
                    replacement.hero = replacement.hero.with_keyword(Keyword::Windfury);
                }
                replace_hero(simulation.app.world_mut(), PlayerId::One, &replacement).unwrap();
                let current = hero(&mut simulation, PlayerId::One);
                let entity = game_entity(simulation.app.world(), current).unwrap();
                assert_eq!(
                    simulation.app.world().get::<AttackState>(entity),
                    Some(&state)
                );
                simulation
                    .app
                    .world_mut()
                    .get_mut::<crate::CurrentStats>(entity)
                    .unwrap()
                    .attack = 1;
                let defender = hero(&mut simulation, PlayerId::Two);
                let action = GameAction::Attack {
                    player: PlayerId::One,
                    attacker: current,
                    defender,
                };
                let allowed = windfury && attacks_this_turn == 1 && !readiness_blocked;
                assert_eq!(simulation.legal_actions().contains(&action), allowed);
                assert_eq!(
                    simulation
                        .snapshot()
                        .objects
                        .iter()
                        .find(|object| object.id == current)
                        .unwrap()
                        .exhausted,
                    Some(!allowed)
                );
                if allowed {
                    simulation.apply(action).unwrap();
                } else {
                    assert_eq!(
                        simulation.apply(action),
                        Err(SimulationError::CannotAttack(current))
                    );
                }
            }
        }
    }
}
