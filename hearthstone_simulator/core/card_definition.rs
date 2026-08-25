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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Selector, ValueExpression};

    #[test]
    fn card_definition_preserves_all_runtime_card_data() {
        let card = Card::minion("Archivist", 4, 3, 5).with_effects(vec![Effect::Heal {
            targets: Selector::Source,
            amount: ValueExpression::Constant(2),
        }]);

        let definition = CardDefinition::from(card);

        assert_eq!(definition.id, "synthetic:archivist");
        assert_eq!(definition.name, "Archivist");
        assert_eq!(definition.kind, EntityKind::Minion);
        assert_eq!(definition.base_cost, 4);
        assert_eq!(definition.base_attack, 3);
        assert_eq!(definition.base_health, 5);
        assert_eq!(definition.program.len(), 1);
    }
}
