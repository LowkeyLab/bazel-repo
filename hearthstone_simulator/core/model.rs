use crate::{
    ConditionTiming, Effect, EntityKind, EventKind, PlayerId, SourceEligibilityPolicy,
    TimedCondition, TriggerCondition, TriggerDefinition, WoundedTargetPolicy, Zone,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Card {
    pub definition_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub mana_cost: i32,
    pub attack: i32,
    pub health: i32,
    pub effects: Vec<Effect>,
    pub triggers: Vec<TriggerDefinition>,
}

impl Card {
    pub fn minion(name: impl Into<String>, mana_cost: i32, attack: i32, health: i32) -> Self {
        let name = name.into();
        Self {
            definition_id: synthetic_definition_id(&name),
            name,
            kind: EntityKind::Minion,
            mana_cost,
            attack,
            health,
            effects: Vec::new(),
            triggers: Vec::new(),
        }
    }

    pub fn spell(name: impl Into<String>, mana_cost: i32) -> Self {
        let name = name.into();
        Self {
            definition_id: synthetic_definition_id(&name),
            name,
            kind: EntityKind::Spell,
            mana_cost,
            attack: 0,
            health: 0,
            effects: Vec::new(),
            triggers: Vec::new(),
        }
    }

    pub fn with_effects(mut self, effects: Vec<Effect>) -> Self {
        self.effects = effects;
        self
    }

    pub fn with_triggers(mut self, triggers: Vec<TriggerDefinition>) -> Self {
        self.triggers = triggers;
        self
    }

    #[must_use]
    pub fn with_deathrattle(mut self, effects: Vec<Effect>) -> Self {
        self.triggers.push(TriggerDefinition {
            event: EventKind::Death,
            eligible_zones: vec![Zone::Graveyard],
            conditions: vec![TimedCondition {
                timing: ConditionTiming::QueueTime,
                condition: TriggerCondition::EventSourceIsSelf,
            }],
            source_eligibility: SourceEligibilityPolicy::RememberedSource,
            priority: 0,
            wounded_target_policy: WoundedTargetPolicy::IncludePendingDestroy,
            effect_program: effects,
        });
        self
    }
}

fn synthetic_definition_id(name: &str) -> String {
    let normalized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("synthetic:{normalized}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerConfig {
    pub name: String,
    pub deck: Vec<Card>,
    pub hand: Vec<Card>,
}

impl PlayerConfig {
    pub fn new(name: impl Into<String>, hand: Vec<Card>) -> Self {
        Self {
            name: name.into(),
            deck: Vec::new(),
            hand,
        }
    }

    pub fn with_deck(name: impl Into<String>, deck: Vec<Card>) -> Self {
        Self {
            name: name.into(),
            deck,
            hand: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerRef {
    pub id: PlayerId,
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[googletest::test]
    fn deck_configuration_starts_with_an_empty_hand() {
        let config = PlayerConfig::with_deck("Jaina", vec![Card::spell("Arcane! Bolt", 1)]);

        assert_that!(config.name, eq("Jaina"));
        assert_that!(config.deck[0].definition_id, eq("synthetic:arcane__bolt"));
        assert_that!(config.hand.is_empty(), is_true());
    }
}
