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
            exhausted: true,
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
        Card::weapon("Old Blade", 0, 3),
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
        Some(Card::weapon("Doomhammer Fixture", 0, 5)),
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
        Card::weapon("Old Weapon", 0, 2),
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
