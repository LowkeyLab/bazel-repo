use crate::{Effect, EntityKind, PlayerId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Card {
    pub definition_id: String,
    pub name: String,
    pub kind: EntityKind,
    pub mana_cost: i32,
    pub attack: i32,
    pub health: i32,
    pub effects: Vec<Effect>,
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
        }
    }

    pub fn with_effects(mut self, effects: Vec<Effect>) -> Self {
        self.effects = effects;
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
    use super::*;

    #[test]
    fn deck_configuration_starts_with_an_empty_hand() {
        let config = PlayerConfig::with_deck("Jaina", vec![Card::spell("Arcane! Bolt", 1)]);

        assert_eq!(config.name, "Jaina");
        assert_eq!(config.deck[0].definition_id, "synthetic:arcane__bolt");
        assert!(config.hand.is_empty());
    }
}
