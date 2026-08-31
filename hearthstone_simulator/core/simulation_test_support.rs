use super::*;

pub(super) fn simulation() -> Simulation {
    Simulation::new([
        PlayerConfig::new("Jaina", vec![Card::minion("Training Minion", 1, 3, 2)]),
        PlayerConfig::new("Rexxar", Vec::new()),
    ])
}

pub(super) fn hand_card(simulation: &mut Simulation, player: PlayerId) -> GameEntityId {
    simulation.snapshot().players[player.bucket() as usize].hand[0]
}

pub(super) fn hero(simulation: &mut Simulation, player: PlayerId) -> GameEntityId {
    simulation.snapshot().players[player.bucket() as usize].hero
}

pub(super) fn self_event_trigger(
    event: EventKind,
    effect_program: Vec<Effect>,
) -> crate::TriggerDefinition {
    crate::TriggerDefinition {
        event,
        eligible_zones: vec![Zone::Play],
        conditions: vec![crate::TimedCondition {
            timing: crate::ConditionTiming::QueueTime,
            condition: crate::TriggerCondition::EventTargetsSelf,
        }],
        source_eligibility: crate::SourceEligibilityPolicy::MustRemainInEligibleZone,
        priority: 0,
        wounded_target_policy: crate::WoundedTargetPolicy::IncludeMortallyWounded,
        effect_program,
    }
}
