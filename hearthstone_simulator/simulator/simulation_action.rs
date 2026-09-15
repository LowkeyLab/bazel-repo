use std::collections::VecDeque;

use bevy::prelude::*;

use crate::{
    AttachedTo, AttackState, CanonicalTrace, ChoiceId, Controller, CurrentResolutionOp,
    CurrentStats, DamageRequest, EffectContext, EnchantmentDuration, EntityKind, EventContext,
    EventKind, GameAction, GameEntityId, GameOutcome, GameState, HeroPowerState, PhaseBoundaryPlan,
    PlayerId, ResolutionOp, ResolutionWork, ResolveFrame, Ruleset, ScheduledTurnKind, SequenceStep,
    SimulationStatus, TraceEntry, TurnSchedule, Zone, ZoneMoveRequest, ZoneMovementKind,
    enchantment::{recalculate_cost, recalculate_keywords, recalculate_stats},
    entity::game_entity,
    resolver::{
        abandon_sequence, begin_sequence, consume_budget, finish_sequence, pop_resolution_op,
        push_resolution_ops,
    },
    trigger::collect_trigger_seeds,
    zone::{
        ZoneIndex, ZoneMoveOutcome, assert_zone_invariants, move_entity, move_entity_with_request,
    },
};

use super::{
    card_runtime::CardRuntime,
    effect_executor::contains_played_self_transform,
    error::SimulationError,
    event_resolver::OperationFailure,
    player::{assert_player_role_invariants, controlled_entity_in_zone, player_mut},
    snapshot::assert_game_entity_index,
};

#[derive(Default, Resource)]
struct PendingActions(VecDeque<GameAction>);

#[derive(Default, Resource)]
struct ActionResults(VecDeque<Result<(), SimulationError>>);

pub(super) fn configure_actions(app: &mut App) {
    app.init_resource::<PendingActions>()
        .init_resource::<ActionResults>()
        .add_systems(Update, process_next_action);
}

pub(super) fn submit_action(app: &mut App, action: GameAction) -> Result<(), SimulationError> {
    app.world_mut()
        .resource_mut::<PendingActions>()
        .0
        .push_back(action);
    app.update();
    app.world_mut()
        .resource_mut::<ActionResults>()
        .0
        .pop_front()
        .ok_or(SimulationError::MissingActionResult)?
}

pub(super) fn submit_choice(app: &mut App, option: ChoiceId) -> Result<(), SimulationError> {
    answer_choice(app.world_mut(), option)?;
    resolve_to_pause(app.world_mut())
}

pub(super) fn legal_actions(world: &mut World) -> Vec<GameAction> {
    let world: &World = world;
    let game = world.resource::<GameState>();
    if game.status != SimulationStatus::AwaitingAction || game.outcome.is_some() {
        return Vec::new();
    }
    let active = game.active_player;
    let mut actions = Vec::new();
    let mut offer = |candidate: GameAction| {
        if let Ok(normalized) = super::action_validation::validate_action(world, &candidate) {
            actions.push(normalized);
        }
    };

    offer(GameAction::EndTurn { player: active });

    let mut hand = world
        .resource::<ZoneIndex>()
        .entities(active, Zone::Hand)
        .to_vec();
    hand.sort_unstable();
    hand.dedup();
    for card in hand {
        let Some(entity) = game_entity(world, card) else {
            continue;
        };
        let (kind, targeting) = match (
            world.get::<EntityKind>(entity),
            world.get::<CardRuntime>(entity),
        ) {
            (Some(kind), Some(runtime)) => (*kind, runtime.targeting),
            _ => continue,
        };
        match kind {
            EntityKind::Minion => {
                let board_len = crate::zone::board_entities(world, active).len();
                for target in super::action_validation::target_options(world, active, targeting) {
                    for board_index in 0..=board_len {
                        offer(GameAction::PlayCard {
                            player: active,
                            card,
                            target,
                            board_index: Some(board_index),
                            choice: None,
                        });
                    }
                }
            }
            EntityKind::Spell => {
                for target in super::action_validation::target_options(world, active, targeting) {
                    offer(GameAction::PlayCard {
                        player: active,
                        card,
                        target,
                        board_index: None,
                        choice: None,
                    });
                }
            }
            _ => {}
        }
    }

    let mut attackers = world
        .resource::<ZoneIndex>()
        .entities(active, Zone::Play)
        .to_vec();
    attackers.sort_unstable();
    attackers.dedup();
    let mut defenders = world
        .resource::<ZoneIndex>()
        .entities(active.opponent(), Zone::Play)
        .to_vec();
    defenders.sort_unstable();
    defenders.dedup();
    for attacker in attackers {
        for defender in &defenders {
            offer(GameAction::Attack {
                player: active,
                attacker,
                defender: *defender,
            });
        }
    }

    offer_hero_power_actions(world, active, &mut offer);

    offer(GameAction::Concede { player: active });
    actions
}

fn offer_hero_power_actions(world: &World, active: PlayerId, mut offer: impl FnMut(GameAction)) {
    let mut powers = world
        .resource::<ZoneIndex>()
        .entities(active, Zone::Play)
        .to_vec();
    powers.sort_unstable();
    powers.dedup();
    for power in powers {
        let Some(entity) = game_entity(world, power) else {
            continue;
        };
        if world.get::<EntityKind>(entity) != Some(&EntityKind::HeroPower) {
            continue;
        }
        let Some(runtime) = world.get::<CardRuntime>(entity) else {
            continue;
        };
        for target in super::action_validation::target_options(world, active, runtime.targeting) {
            offer(GameAction::UseHeroPower {
                player: active,
                power,
                target,
            });
        }
    }
}

fn process_next_action(world: &mut World) {
    let Some(action) = world.resource_mut::<PendingActions>().0.pop_front() else {
        return;
    };
    let player = action.player();
    let label = action.label().to_string();
    let result = apply_action(world, &action);
    match &result {
        Ok(()) => world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::ActionAccepted {
                player,
                action: label,
            }),
        Err(error) => {
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::ActionRejected {
                    player,
                    reason: error.to_string(),
                });
        }
    }
    world.resource_mut::<ActionResults>().0.push_back(result);
}

fn apply_action(world: &mut World, action: &GameAction) -> Result<(), SimulationError> {
    let action = super::action_validation::validate_action(world, action)?;
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    begin_sequence(world)?;
    let operation = match &action {
        GameAction::PlayCard {
            player,
            card,
            target,
            board_index,
            ..
        } => ResolutionOp::RunGuardedSequenceStep {
            guards: vec![crate::SubjectGuard {
                subject: *card,
                required_zone: Zone::Hand,
            }],
            step: SequenceStep::PlayCard {
                player: *player,
                card: *card,
                target: *target,
                board_index: *board_index,
            },
        },
        GameAction::Attack {
            player,
            attacker,
            defender,
        } => ResolutionOp::RunGuardedSequenceStep {
            guards: vec![
                crate::SubjectGuard {
                    subject: *attacker,
                    required_zone: Zone::Play,
                },
                crate::SubjectGuard {
                    subject: *defender,
                    required_zone: Zone::Play,
                },
            ],
            step: SequenceStep::Attack {
                player: *player,
                attacker: *attacker,
                defender: *defender,
            },
        },
        GameAction::UseHeroPower {
            player,
            power,
            target,
        } => ResolutionOp::RunGuardedSequenceStep {
            guards: vec![crate::SubjectGuard {
                subject: *power,
                required_zone: Zone::Play,
            }],
            step: SequenceStep::UseHeroPower {
                player: *player,
                power: *power,
                target: *target,
            },
        },
        GameAction::EndTurn { player } => {
            ResolutionOp::RunSequenceStep(SequenceStep::EndTurn { player: *player })
        }
        GameAction::Concede { player } => {
            ResolutionOp::RunSequenceStep(SequenceStep::Concede { player: *player })
        }
    };
    push_resolution_ops(
        world,
        [
            operation,
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::CheckOutcome,
        ],
    );
    resolve_to_pause(world)
}

fn resolve_to_pause(world: &mut World) -> Result<(), SimulationError> {
    let result = drive_resolution(world).and_then(|()| finish_resolution_if_idle(world));
    if result.is_err() {
        abandon_sequence(world);
        world.resource_mut::<GameState>().status =
            if world.resource::<GameState>().outcome.is_some() {
                SimulationStatus::Complete
            } else {
                SimulationStatus::AwaitingAction
            };
    }
    result
}

pub(super) fn drive_resolution(world: &mut World) -> Result<(), SimulationError> {
    while let Some(operation) = pop_resolution_op(world) {
        consume_budget(world, operation.id)?;
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::OperationPopped {
                id: operation.id,
                kind: operation.operation.kind().to_string(),
            });
        world.resource_mut::<CurrentResolutionOp>().0 = Some(operation);
        world.run_schedule(ResolveFrame);
        if let Some(error) = world.resource_mut::<OperationFailure>().0.take() {
            return Err(error);
        }
        if world.resource::<GameState>().status == SimulationStatus::AwaitingChoice {
            break;
        }
    }
    Ok(())
}

fn finish_resolution_if_idle(world: &mut World) -> Result<(), SimulationError> {
    if world.resource::<GameState>().status == SimulationStatus::AwaitingChoice {
        return Ok(());
    }
    if !world.resource::<ResolutionWork>().stack.is_empty() {
        return Ok(());
    }
    finish_sequence(world);
    crate::resolver::assert_resolution_invariants(world).map_err(SimulationError::Invariant)?;
    world.resource_mut::<GameState>().status = if world.resource::<GameState>().outcome.is_some() {
        SimulationStatus::Complete
    } else {
        SimulationStatus::AwaitingAction
    };
    assert_zone_invariants(world).map_err(SimulationError::Invariant)?;
    assert_player_role_invariants(world).map_err(SimulationError::Invariant)?;
    assert_game_entity_index(world).map_err(SimulationError::Invariant)
}

fn answer_choice(world: &mut World, option: ChoiceId) -> Result<(), SimulationError> {
    let pending = world
        .resource_mut::<ResolutionWork>()
        .pending_choice
        .take()
        .ok_or(crate::resolver::ResolutionError::NoPendingChoice)?;
    let Some(selected) = pending
        .request
        .options
        .iter()
        .find(|candidate| candidate.id == option)
        .cloned()
    else {
        world.resource_mut::<ResolutionWork>().pending_choice = Some(pending);
        return Err(crate::resolver::ResolutionError::InvalidChoice(option).into());
    };
    world.resource_mut::<GameState>().status = SimulationStatus::Resolving;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ChoiceAnswered {
            choice: pending.request.id,
            option,
        });
    push_resolution_ops(world, selected.operations);
    Ok(())
}

pub(super) fn run_sequence_step(
    world: &mut World,
    step: &SequenceStep,
) -> Result<(), SimulationError> {
    match step {
        SequenceStep::PlayCard {
            player,
            card,
            target,
            board_index,
        } => play_card(world, *player, *card, *target, *board_index),
        SequenceStep::Attack {
            player,
            attacker,
            defender,
        } => {
            attack(world, *player, *attacker, *defender);
            Ok(())
        }
        SequenceStep::PrepareCombatDamage {
            player,
            attacker,
            defender,
        } => {
            prepare_combat_damage(world, *player, *attacker, *defender);
            Ok(())
        }
        SequenceStep::BreakAttackStealth { attacker } => break_attack_stealth(world, *attacker),
        SequenceStep::FinishAttack {
            player,
            attacker,
            defender,
        } => {
            finish_attack(world, *player, *attacker, *defender);
            Ok(())
        }
        SequenceStep::UseHeroPower {
            player,
            power,
            target,
        } => use_hero_power(world, *player, *power, *target),
        SequenceStep::FinishHeroPower { power } => {
            finish_hero_power(world, *power);
            Ok(())
        }
        SequenceStep::EndTurn { player } => {
            end_turn(world, *player);
            Ok(())
        }
        SequenceStep::ThawCharacters { player } => thaw_characters(world, *player),
        SequenceStep::AdvanceTurn { ending_player } => {
            advance_turn(world, *ending_player);
            Ok(())
        }
        SequenceStep::StartTurn { player, kind } => start_turn(world, *player, *kind),
        SequenceStep::Concede { player } => {
            concede(world, *player);
            Ok(())
        }
    }
}

pub(super) fn run_guarded_sequence_step(
    world: &mut World,
    guards: &[crate::SubjectGuard],
    step: &SequenceStep,
) -> Result<(), SimulationError> {
    for guard in guards {
        let actual_zone =
            game_entity(world, guard.subject).and_then(|entity| world.get::<Zone>(entity).copied());
        if actual_zone != Some(guard.required_zone) {
            world
                .resource_mut::<CanonicalTrace>()
                .entries
                .push(TraceEntry::SequenceStepSkipped {
                    step: step.clone(),
                    subject: guard.subject,
                    expected_zone: guard.required_zone,
                    actual_zone,
                });
            return Ok(());
        }
    }
    run_sequence_step(world, step)
}

fn play_card(
    world: &mut World,
    player_id: PlayerId,
    card_id: GameEntityId,
    declared_target: Option<GameEntityId>,
    board_index: Option<usize>,
) -> Result<(), SimulationError> {
    let card_entity = controlled_entity_in_zone(world, player_id, card_id, Zone::Hand)?;
    let kind = *world
        .get::<EntityKind>(card_entity)
        .ok_or(SimulationError::NotPlayable(card_id))?;
    let runtime = world
        .get::<CardRuntime>(card_entity)
        .cloned()
        .ok_or(SimulationError::NotPlayable(card_id))?;
    let finishes_played_self =
        kind == EntityKind::Minion && contains_played_self_transform(&runtime.program);
    spend_resources(world, player_id, runtime.cost)?;
    let destination = if kind == EntityKind::Minion {
        Zone::Play
    } else {
        Zone::Graveyard
    };
    let outcome = move_entity(world, card_id, destination, board_index)?;
    let ZoneMoveOutcome::Moved { from, .. } = outcome else {
        return Err(SimulationError::Invariant(format!(
            "validated card move produced {outcome:?}"
        )));
    };
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ZoneMoved {
            entity: card_id,
            from,
            to: destination,
        });
    let context = EffectContext {
        source: Some(card_id),
        controller: player_id,
        declared_target,
        drawn_card: None,
        origin: if kind == EntityKind::Spell {
            crate::EffectOrigin::Spell
        } else {
            crate::EffectOrigin::Other
        },
    };
    let original_after_play = finishes_played_self.then(|| {
        collect_trigger_seeds(
            world,
            &EventContext {
                kind: EventKind::AfterPlay,
                source: Some(card_id),
                targets: vec![card_id],
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            },
        )
    });
    let mut operations = vec![ResolutionOp::PrepareEvent(EventContext {
        kind: EventKind::CardPlayed,
        source: Some(card_id),
        targets: declared_target.into_iter().collect(),
        controller: player_id,
        proposed_value: None,
        actual_value: None,
        simultaneous_ordinal: 0,
    })];
    if kind == EntityKind::Minion {
        operations.push(ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::Summoned,
            source: Some(card_id),
            targets: vec![card_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        }));
        operations.push(ResolutionOp::RefreshAuras(
            crate::AuraRefreshPlan::PlayedProvider(card_id),
        ));
    }
    let play_scope =
        original_after_play.map(|seeds| crate::resolver::open_play_scope(world, card_id, seeds));
    operations.extend(runtime.program.into_iter().map(|effect| match play_scope {
        Some(scope) => ResolutionOp::RunPlayEffect {
            scope,
            context: context.clone(),
            effect,
        },
        None => ResolutionOp::RunEffect {
            context: context.clone(),
            effect,
            event: None,
        },
    }));
    push_resolution_ops(world, operations);
    Ok(())
}

fn use_hero_power(
    world: &mut World,
    player: PlayerId,
    power: GameEntityId,
    target: Option<GameEntityId>,
) -> Result<(), SimulationError> {
    let entity = controlled_entity_in_zone(world, player, power, Zone::Play)?;
    let runtime = world
        .get::<CardRuntime>(entity)
        .cloned()
        .ok_or(SimulationError::NotActiveHeroPower(power))?;
    let event_context = EventContext {
        kind: EventKind::AfterHeroPower,
        source: Some(power),
        targets: target.into_iter().collect(),
        controller: player,
        proposed_value: None,
        actual_value: None,
        simultaneous_ordinal: 0,
    };
    let seeds = collect_trigger_seeds(world, &event_context);
    let after_use = super::event_resolver::prepare_event_with_seeds(world, event_context, seeds);
    spend_resources(world, player, runtime.cost)?;
    let context = EffectContext {
        source: Some(power),
        controller: player,
        declared_target: target,
        drawn_card: None,
        origin: crate::EffectOrigin::HeroPower,
    };
    let mut operations = runtime
        .program
        .into_iter()
        .map(|effect| ResolutionOp::RunEffect {
            context: context.clone(),
            effect,
            event: None,
        })
        .collect::<Vec<_>>();
    operations.extend([
        ResolutionOp::RunSequenceStep(SequenceStep::FinishHeroPower { power }),
        ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
        ResolutionOp::ResolveEvent(after_use),
    ]);
    push_resolution_ops(world, operations);
    Ok(())
}

// OriginalPowerCompletion: record the use on the original power, including after
// replacement moves it out of Play. Never exhaust the newly installed power.
fn finish_hero_power(world: &mut World, power: GameEntityId) {
    let Some(entity) = game_entity(world, power) else {
        return;
    };
    if world.get::<EntityKind>(entity) != Some(&EntityKind::HeroPower) {
        return;
    }
    if let Some(mut state) = world.get_mut::<HeroPowerState>(entity) {
        state.uses_this_turn = state.uses_this_turn.saturating_add(1);
        state.exhausted = true;
    }
}

fn attack(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) {
    push_resolution_ops(
        world,
        [
            ResolutionOp::PrepareEvent(EventContext {
                kind: EventKind::Attack,
                source: Some(attacker_id),
                targets: vec![defender_id],
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            }),
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![crate::SubjectGuard {
                    subject: attacker_id,
                    required_zone: Zone::Play,
                }],
                step: SequenceStep::BreakAttackStealth {
                    attacker: attacker_id,
                },
            },
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::CheckOutcome,
            ResolutionOp::RunGuardedSequenceStep {
                guards: vec![
                    crate::SubjectGuard {
                        subject: attacker_id,
                        required_zone: Zone::Play,
                    },
                    crate::SubjectGuard {
                        subject: defender_id,
                        required_zone: Zone::Play,
                    },
                ],
                step: SequenceStep::PrepareCombatDamage {
                    player: player_id,
                    attacker: attacker_id,
                    defender: defender_id,
                },
            },
        ],
    );
}

// Combat continuation uses the accepted identities, not declaration-time targeting or
// readiness rules. The preparation boundary has resolved deaths and refreshed auras.
fn prepare_combat_damage(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) {
    if world.resource::<GameState>().outcome.is_some() {
        return;
    }
    let attacker = game_entity(world, attacker_id).expect("validated attacker remains indexed");
    let defender = game_entity(world, defender_id).expect("validated defender remains indexed");
    let attack_value = world
        .get::<CurrentStats>(attacker)
        .map_or(0, |stats| stats.attack);
    let counter_damage = world
        .get::<CurrentStats>(defender)
        .map_or(0, |stats| stats.attack);
    let mut damage = vec![DamageRequest {
        source: Some(attacker_id),
        target: defender_id,
        proposed: attack_value,
    }];
    if counter_damage > 0 {
        damage.push(DamageRequest {
            source: Some(defender_id),
            target: attacker_id,
            proposed: counter_damage,
        });
    }
    push_resolution_ops(
        world,
        [
            ResolutionOp::ProcessDamageBatch(damage),
            ResolutionOp::RunSequenceStep(SequenceStep::FinishAttack {
                player: player_id,
                attacker: attacker_id,
                defender: defender_id,
            }),
        ],
    );
}

// Consume all grants present after attack reactions. An ordered removal survives
// recalculation and in-Play copying, while later grants can restore Stealth.
fn break_attack_stealth(world: &mut World, attacker: GameEntityId) -> Result<(), SimulationError> {
    let Some(entity) = game_entity(world, attacker) else {
        return Ok(());
    };
    if !crate::aura::has_keyword(world, entity, crate::Keyword::Stealth) {
        return Ok(());
    }
    let controller = world
        .get::<Controller>(entity)
        .ok_or(SimulationError::EntityNotFound(attacker))?
        .0;
    super::effect_executor::attach_keyword_modifier(
        world,
        controller,
        attacker,
        crate::KeywordModifier {
            keyword: crate::Keyword::Stealth,
            granted: false,
            silence_removable: true,
        },
        EnchantmentDuration::Permanent,
    )?;
    Ok(())
}

fn finish_attack(
    world: &mut World,
    player_id: PlayerId,
    attacker_id: GameEntityId,
    defender_id: GameEntityId,
) {
    if let Some(attacker) = game_entity(world, attacker_id)
        && let Some(mut state) = world.get_mut::<AttackState>(attacker)
    {
        state.attacks_this_turn += 1;
    }
    push_resolution_ops(
        world,
        [ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::AfterAttack,
            source: Some(attacker_id),
            targets: vec![defender_id],
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        })],
    );
}

fn end_turn(world: &mut World, player_id: PlayerId) {
    push_resolution_ops(
        world,
        [
            ResolutionOp::PrepareEvent(EventContext {
                kind: EventKind::TurnEnded,
                source: None,
                targets: Vec::new(),
                controller: player_id,
                proposed_value: None,
                actual_value: None,
                simultaneous_ordinal: 0,
            }),
            ResolutionOp::RunPhaseBoundary(PhaseBoundaryPlan::Ordinary),
            ResolutionOp::CheckOutcome,
            ResolutionOp::RunSequenceStep(SequenceStep::ThawCharacters { player: player_id }),
            ResolutionOp::RunSequenceStep(SequenceStep::AdvanceTurn {
                ending_player: player_id,
            }),
        ],
    );
}

fn thaw_characters(world: &mut World, player: PlayerId) -> Result<(), SimulationError> {
    let thawing = world
        .resource::<ZoneIndex>()
        .entities(player, Zone::Play)
        .iter()
        .copied()
        .filter(|id| {
            game_entity(world, *id).is_some_and(|entity| {
                crate::aura::has_keyword(world, entity, crate::Keyword::Frozen)
                    && super::action_validation::can_thaw(world, entity)
            })
        })
        .collect::<Vec<_>>();
    for id in thawing {
        super::effect_executor::attach_keyword_modifier(
            world,
            player,
            id,
            crate::KeywordModifier {
                keyword: crate::Keyword::Frozen,
                granted: false,
                silence_removable: true,
            },
            EnchantmentDuration::Permanent,
        )?;
    }
    Ok(())
}

fn advance_turn(world: &mut World, ending_player: PlayerId) {
    if world.resource::<GameState>().outcome.is_some() {
        return;
    }
    let next = world
        .resource_mut::<TurnSchedule>()
        .next_turn(ending_player);
    expire_temporary_effects(world, ending_player, next.player);
    push_resolution_ops(
        world,
        [ResolutionOp::RunSequenceStep(SequenceStep::StartTurn {
            player: next.player,
            kind: next.kind,
        })],
    );
}

fn expire_temporary_effects(world: &mut World, ending_player: PlayerId, next_player: PlayerId) {
    let expiring = world
        .iter_entities()
        .filter_map(|entity| {
            let attached_to = entity.get::<AttachedTo>()?.0;
            if entity.get::<Zone>() != Some(&Zone::Play) {
                return None;
            }
            let duration = *entity.get::<EnchantmentDuration>()?;
            let expires = match duration {
                EnchantmentDuration::Permanent => false,
                EnchantmentDuration::EndOfTurn(player) => player == ending_player,
                EnchantmentDuration::EndOfTurnSeries(player) => {
                    player == ending_player && next_player != ending_player
                }
            };
            if !expires {
                return None;
            }
            Some((
                *entity.get::<GameEntityId>()?,
                entity.get::<Controller>()?.0,
                attached_to,
                entity.id(),
            ))
        })
        .collect::<Vec<_>>();
    for (id, controller, attached_to, entity) in expiring {
        let target = world.get::<GameEntityId>(attached_to).copied();
        world.entity_mut(entity).remove::<AttachedTo>();
        let _ = move_entity_with_request(
            world,
            ZoneMoveRequest {
                entity: id,
                destination_controller: controller,
                destination: Zone::RemovedFromGame,
                position: None,
                kind: ZoneMovementKind::DetachEnchantment,
            },
        );
        world
            .resource_mut::<CanonicalTrace>()
            .entries
            .push(TraceEntry::TemporaryEffectExpired { entity: id });
        if let Some(target) = target {
            recalculate_stats(world, target);
            recalculate_keywords(world, target);
            recalculate_cost(world, target);
        }
    }
}

fn start_turn(
    world: &mut World,
    player_id: PlayerId,
    turn_kind: ScheduledTurnKind,
) -> Result<(), SimulationError> {
    let maximum_mana = world.resource::<Ruleset>().maximum_mana;
    {
        let mut game = world.resource_mut::<GameState>();
        game.active_player = player_id;
        game.turn_number += 1;
    }
    let (_, mut player, _, _) = player_mut(world, player_id)?;
    player.maximum_resources = (player.maximum_resources + 1).min(maximum_mana);
    player.used_resources = 0;
    player.temporary_resources = 0;
    player.locked_overload = player.pending_overload;
    player.pending_overload = 0;
    let turn_entities = world
        .resource::<ZoneIndex>()
        .entities(player_id, Zone::Play)
        .to_vec();
    for id in turn_entities {
        if let Some(entity) = game_entity(world, id) {
            if let Some(mut state) = world.get_mut::<AttackState>(entity) {
                state.attacks_this_turn = 0;
                state.readiness_blocked = false;
            }
            if let Some(mut state) = world.get_mut::<HeroPowerState>(entity) {
                state.uses_this_turn = 0;
                state.exhausted = false;
            }
        }
    }
    let turn = world.resource::<GameState>().turn_number;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::TurnChanged {
            active_player: player_id,
            turn,
            kind: turn_kind,
        });
    push_resolution_ops(
        world,
        [ResolutionOp::PrepareEvent(EventContext {
            kind: EventKind::TurnStarted,
            source: None,
            targets: Vec::new(),
            controller: player_id,
            proposed_value: None,
            actual_value: None,
            simultaneous_ordinal: 0,
        })],
    );
    Ok(())
}

fn concede(world: &mut World, player: PlayerId) {
    let winner = player.opponent();
    world.resource_mut::<GameState>().outcome = Some(GameOutcome::Winner(winner));
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::Outcome {
            winner: Some(winner),
        });
}

fn spend_resources(
    world: &mut World,
    player_id: PlayerId,
    amount: i32,
) -> Result<(), SimulationError> {
    let amount = amount.max(0);
    let (_, mut player, _, _) = player_mut(world, player_id)?;
    player.used_resources += amount;
    player.resources_spent += amount;
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::ResourceSpent {
            player: player_id,
            amount,
        });
    Ok(())
}
