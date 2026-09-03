use googletest::prelude::*;

use super::{test_support::*, *};
use crate::{
    AuraCategory, AuraDefinition, AuraTarget, ContinuousEffectDefinition, ContinuousModifier,
    Controller, EnchantmentDuration, OtherAuraCache, OtherAuraModifier, PlayerAudience,
};

fn stat_aura(targets: AuraTarget, attack: i32, health: i32) -> AuraDefinition {
    AuraDefinition {
        targets,
        attack,
        health,
        other: Vec::new(),
    }
}

fn other_aura(targets: AuraTarget, modifier: OtherAuraModifier) -> AuraDefinition {
    AuraDefinition {
        targets,
        attack: 0,
        health: 0,
        other: vec![modifier],
    }
}

#[googletest::test]
fn both_player_continuous_effects_and_unrelated_other_auras_are_neutral() {
    let mut simulation = simulation();
    spawn_card(
        simulation.app.world_mut(),
        PlayerId::One,
        Card::minion("Shared spell damage", 0, 1, 1).with_continuous_effect(
            ContinuousEffectDefinition {
                recipients: PlayerAudience::Both,
                modifier: ContinuousModifier::SpellDamage(2),
            },
        ),
        Zone::Play,
    )
    .unwrap();
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::One),
        eq(2)
    );
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::Two),
        eq(2)
    );

    let player = simulation
        .app
        .world()
        .iter_entities()
        .find(|entity| {
            entity.get::<EntityKind>() == Some(&EntityKind::Player)
                && entity.get::<Controller>() == Some(&Controller(PlayerId::One))
        })
        .unwrap()
        .id();
    simulation
        .app
        .world_mut()
        .entity_mut(player)
        .insert(OtherAuraCache(vec![AuraApplication {
            provider: GameEntityId(999),
            definition_index: 0,
            modifier: AuraModifier::Immune,
        }]));
    assert_that!(
        crate::aura::hero_power_damage_bonus(simulation.app.world(), PlayerId::One),
        eq(0)
    );
}

fn object(simulation: &mut Simulation, id: GameEntityId) -> GameObjectSnapshot {
    simulation
        .snapshot()
        .objects
        .into_iter()
        .find(|object| object.id == id)
        .expect("game object should be present")
}

#[googletest::test]
fn played_aura_is_active_before_the_provider_program() {
    let provider = Card::minion("Battle Leader", 0, 2, 2)
        .with_aura(stat_aura(AuraTarget::FriendlyMinions, 1, 2))
        .with_effects(vec![Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::SourceAttack,
        }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let provider = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: provider,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(object(&mut simulation, provider).attack, eq(Some(3)));
    assert_that!(
        object(&mut simulation, provider).maximum_health,
        eq(Some(4))
    );
    assert_that!(simulation.snapshot().players[1].health, eq(27));
    assert_that!(
        simulation.trace().iter().any(|entry| matches!(
            entry,
            TraceEntry::AuraUpdated {
                target,
                category: AuraCategory::Attack,
                applications,
            } if *target == provider && applications.len() == 1
        )),
        is_true()
    );
}

#[googletest::test]
fn summoned_spell_damage_is_read_by_later_spell_work() {
    let spell = Card::spell("Conjure Bolt", 0).with_effects(vec![Effect::Sequence(vec![
        Effect::Summon {
            player: PlayerSelector::Controller,
            card: Card::minion("Lesser Arcane Totem", 0, 0, 2).with_spell_damage(1),
            board_index: None,
        },
        Effect::Summon {
            player: PlayerSelector::Controller,
            card: Card::minion("Arcane Totem", 0, 0, 2).with_spell_damage(2),
            board_index: None,
        },
        Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        },
    ])]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![spell]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let spell = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: spell,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[1].health, eq(26));
}

#[googletest::test]
fn spell_damage_uses_live_silence_state_within_one_spell() {
    let provider = Card::minion("Spell Power", 0, 1, 3).with_spell_damage(2);
    let spell = Card::spell("Fading Volley", 0).with_effects(vec![Effect::Sequence(vec![
        Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        },
        Effect::Silence {
            targets: Selector::FriendlyMinions,
        },
        Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        },
    ])]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, spell]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let provider = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: provider,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let spell = hand_card(&mut simulation, PlayerId::One);
    let target = hero(&mut simulation, PlayerId::Two);

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: spell,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[1].health, eq(26));
}

#[googletest::test]
fn attached_and_opponent_facing_spell_damage_are_live() {
    let minion = Card::minion("Chosen", 0, 1, 3);
    let blessing =
        Card::spell("Arcane Blessing", 0).with_effects(vec![Effect::AttachContinuousEffect {
            targets: Selector::DeclaredTarget,
            effect: ContinuousEffectDefinition {
                recipients: PlayerAudience::Controller,
                modifier: ContinuousModifier::SpellDamage(2),
            },
            silence_removable: true,
            duration: EnchantmentDuration::Permanent,
        }]);
    let bolt = Card::spell("Bolt", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::DeclaredTarget,
        amount: ValueExpression::Constant(1),
    }]);
    let moonkin = Card::minion("Moonkin", 0, 1, 3).with_opponent_spell_damage(2);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![minion, blessing, bolt.clone(), moonkin]),
        PlayerConfig::new("Rexxar", vec![bolt]),
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
    let blessing = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: blessing,
            target: Some(minion),
            board_index: None,
            choice: None,
        })
        .unwrap();
    let bolt = hand_card(&mut simulation, PlayerId::One);
    let enemy_hero = hero(&mut simulation, PlayerId::Two);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: bolt,
            target: Some(enemy_hero),
            board_index: None,
            choice: None,
        })
        .unwrap();
    assert_that!(simulation.snapshot().players[1].health, eq(27));
    silence_entity(simulation.app.world_mut(), minion).unwrap();
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::One),
        eq(0)
    );
    let moonkin = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: moonkin,
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
    let bolt = hand_card(&mut simulation, PlayerId::Two);
    let target = hero(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::Two,
            card: bolt,
            target: Some(target),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[0].health, eq(27));
}

#[googletest::test]
fn hero_power_uses_only_its_dedicated_other_aura_modifier() {
    let mut simulation = simulation();
    let world = simulation.app.world_mut();
    spawn_card(
        world,
        PlayerId::One,
        Card::minion("Spell Power", 0, 0, 2).with_spell_damage(2),
        Zone::Play,
    )
    .unwrap();
    spawn_card(
        world,
        PlayerId::One,
        Card::minion("Hero Power Mentor", 0, 0, 2).with_aura(other_aura(
            AuraTarget::ControllerPlayer,
            OtherAuraModifier::HeroPowerDamage(3),
        )),
        Zone::Play,
    )
    .unwrap();
    crate::aura::refresh_all_auras(world);
    let target = hero(&mut simulation, PlayerId::Two);
    let world = simulation.app.world_mut();
    begin_sequence(world).unwrap();
    execute_effect(
        world,
        &EffectContext {
            source: None,
            controller: PlayerId::One,
            declared_target: Some(target),
            origin: EffectOrigin::HeroPower,
        },
        &Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(1),
        },
    )
    .unwrap();
    drive_resolution(world).unwrap();
    finish_sequence(world);

    assert_that!(simulation.snapshot().players[1].health, eq(26));
}

#[googletest::test]
fn trigger_damage_does_not_receive_spell_damage() {
    let provider = Card::minion("Spell Power", 0, 1, 3).with_spell_damage(2);
    let deathrattle =
        Card::minion("Last Ping", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::EnemyCharacters,
            amount: ValueExpression::Constant(1),
        }]);
    let destroy = Card::spell("Dismiss", 0).with_effects(vec![Effect::Destroy {
        targets: Selector::DeclaredTarget,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, deathrattle, destroy]),
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
    let victim = simulation.snapshot().players[0]
        .board
        .iter()
        .copied()
        .find(|id| object(&mut simulation, *id).name == "Last Ping")
        .unwrap();
    let destroy = hand_card(&mut simulation, PlayerId::One);

    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: destroy,
            target: Some(victim),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[1].health, eq(29));
}

#[googletest::test]
fn injured_minion_survives_health_aura_removal_under_h2() {
    let provider = Card::minion("Health Captain", 0, 1, 1).with_aura(stat_aura(
        AuraTarget::OtherFriendlyMinions,
        0,
        2,
    ));
    let target = Card::minion("Protected", 0, 1, 2);
    let blast = Card::spell("Three Damage", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(3),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, target, blast]),
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

    let queued = simulation
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            TraceEntry::DeathPhaseQueued { deaths } => Some(deaths.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_that!(queued.len(), eq(1));
    assert_that!(queued[0].len(), eq(1));
    let snapshot = simulation.snapshot();
    assert_that!(snapshot.deaths.len(), eq(1));
    let survivor = snapshot
        .objects
        .iter()
        .find(|object| object.name == "Protected")
        .unwrap();
    assert_that!(survivor.maximum_health, eq(Some(2)));
    assert_that!(survivor.damage, eq(1));
}

#[googletest::test]
fn attack_aura_expires_before_deathrattle_reads_source_attack() {
    let provider = Card::minion("Attack Captain", 0, 1, 1).with_aura(stat_aura(
        AuraTarget::OtherFriendlyMinions,
        3,
        0,
    ));
    let deathrattle =
        Card::minion("Attack Reader", 0, 1, 1).with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::EnemyCharacters,
            amount: ValueExpression::SourceAttack,
        }]);
    let blast = Card::spell("Board Clear", 0).with_effects(vec![Effect::DealDamage {
        targets: Selector::AllMinions,
        amount: ValueExpression::Constant(5),
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, deathrattle, blast]),
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

    assert_that!(simulation.snapshot().players[1].health, eq(29));
}

#[googletest::test]
fn summoned_other_aura_refreshes_immediately() {
    let immune = Card::minion("Immune Totem", 0, 0, 2).with_aura(other_aura(
        AuraTarget::FriendlyCharacters,
        OtherAuraModifier::Immune,
    ));
    let spell = Card::spell("Conjure Protection", 0).with_effects(vec![Effect::Sequence(vec![
        Effect::Summon {
            player: PlayerSelector::Controller,
            card: immune,
            board_index: None,
        },
        Effect::DealDamage {
            targets: Selector::DeclaredTarget,
            amount: ValueExpression::Constant(3),
        },
    ])]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![spell]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let spell = hand_card(&mut simulation, PlayerId::One);
    let hero = hero(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: spell,
            target: Some(hero),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[0].health, eq(30));
}

#[googletest::test]
fn other_aura_expires_before_provider_deathrattle() {
    let provider = Card::minion("Mortal Ward", 0, 1, 1)
        .with_aura(other_aura(
            AuraTarget::FriendlyCharacters,
            OtherAuraModifier::Immune,
        ))
        .with_deathrattle(vec![Effect::DealDamage {
            targets: Selector::FriendlyCharacters,
            amount: ValueExpression::Constant(2),
        }]);
    let destroy = Card::spell("Dismiss Ward", 0).with_effects(vec![Effect::Destroy {
        targets: Selector::DeclaredTarget,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, destroy]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let provider = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: provider,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    let destroy = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: destroy,
            target: Some(provider),
            board_index: None,
            choice: None,
        })
        .unwrap();

    assert_that!(simulation.snapshot().players[0].health, eq(28));
}

#[googletest::test]
fn play_to_play_copy_preserves_silence_without_received_aura_cache() {
    let provider = Card::minion("Silenced Provider", 0, 1, 2)
        .with_aura(stat_aura(AuraTarget::FriendlyMinions, 1, 0))
        .with_spell_damage(2);
    let silence = Card::spell("Silence", 0).with_effects(vec![Effect::Silence {
        targets: Selector::DeclaredTarget,
    }]);
    let copy = Card::spell("Copy", 0).with_effects(vec![Effect::Copy {
        targets: Selector::DeclaredTarget,
        player: PlayerSelector::Controller,
        zone: Zone::Play,
    }]);
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![provider, silence, copy]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ]);
    let provider = hand_card(&mut simulation, PlayerId::One);
    simulation
        .apply(GameAction::PlayCard {
            player: PlayerId::One,
            card: provider,
            target: None,
            board_index: None,
            choice: None,
        })
        .unwrap();
    for _ in 0..2 {
        let spell = hand_card(&mut simulation, PlayerId::One);
        simulation
            .apply(GameAction::PlayCard {
                player: PlayerId::One,
                card: spell,
                target: Some(provider),
                board_index: None,
                choice: None,
            })
            .unwrap();
    }

    let minions = simulation
        .snapshot()
        .objects
        .into_iter()
        .filter(|object| object.kind == EntityKind::Minion && object.zone == Zone::Play)
        .map(|object| object.id)
        .collect::<Vec<_>>();
    assert_that!(minions.len(), eq(2));
    for minion in minions {
        let entity = game_entity(simulation.app.world(), minion).unwrap();
        assert_that!(
            simulation.app.world().get::<Silenced>(entity).is_some(),
            is_true()
        );
        assert_that!(
            simulation
                .app
                .world()
                .get::<AttackAuraCache>(entity)
                .is_none_or(|cache| cache.0.is_empty()),
            is_true()
        );
    }
    assert_that!(
        crate::aura::current_spell_damage(simulation.app.world(), PlayerId::One),
        eq(0)
    );
}

#[googletest::test]
fn aura_cache_ordering_and_checkpoint_state_are_deterministic() {
    let first = Card::minion("First Aura", 0, 1, 2)
        .with_aura(stat_aura(AuraTarget::AllMinions, 1, 0))
        .with_spell_damage(1);
    let second =
        Card::minion("Second Aura", 0, 1, 2).with_aura(stat_aura(AuraTarget::AllMinions, 2, 0));
    let mut simulation = Simulation::new([
        PlayerConfig::new("Jaina", vec![first, second]),
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
    let board = simulation.snapshot().players[0].board.clone();
    let target = board
        .iter()
        .copied()
        .find(|id| object(&mut simulation, *id).kind == EntityKind::Minion)
        .unwrap();
    let target_entity = game_entity(simulation.app.world(), target).unwrap();
    let cache = simulation
        .app
        .world()
        .get::<AttackAuraCache>(target_entity)
        .unwrap();
    assert_that!(cache.0.len(), eq(2));
    assert_that!(cache.0[0].modifier, eq(AuraModifier::Attack(1)));
    assert_that!(cache.0[1].modifier, eq(AuraModifier::Attack(2)));

    silence_entity(simulation.app.world_mut(), target).unwrap();
    let copied = copy_card_data(simulation.app.world(), target).unwrap();
    assert_that!(copied.auras.len(), eq(1));
    assert_that!(copied.continuous_effects.len(), eq(1));
    crate::aura::refresh_health_attack_auras(simulation.app.world_mut());

    let checkpoint = simulation.checkpoint().unwrap();
    let restored = Simulation::from_checkpoint(checkpoint.clone()).unwrap();
    assert_that!(restored.checkpoint().unwrap(), eq(&checkpoint));
}
