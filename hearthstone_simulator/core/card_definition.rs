use std::collections::BTreeMap;

use bevy::prelude::Resource;

use crate::{Card, Effect, EntityKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardDefinition {
    pub id: String,
    pub name: String,
    pub kind: EntityKind,
    pub base_cost: i32,
    pub base_attack: i32,
    pub base_health: i32,
    pub program: Vec<Effect>,
}

impl From<Card> for CardDefinition {
    fn from(card: Card) -> Self {
        Self {
            id: card.definition_id,
            name: card.name,
            kind: card.kind,
            base_cost: card.mana_cost,
            base_attack: card.attack,
            base_health: card.health,
            program: card.effects,
        }
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct CardDefinitions(pub BTreeMap<String, CardDefinition>);
