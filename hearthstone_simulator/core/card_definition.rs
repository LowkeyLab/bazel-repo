use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::Resource;

use crate::{
    AuraDefinition, Card, ContinuousEffectDefinition, Effect, EntityKind, Keyword,
    TargetRequirement, TriggerDefinition,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardDefinition {
    pub id: String,
    pub name: String,
    pub kind: EntityKind,
    pub base_cost: i32,
    pub base_attack: i32,
    pub base_health: i32,
    pub base_keywords: BTreeSet<Keyword>,
    pub program: Vec<Effect>,
    pub triggers: Vec<TriggerDefinition>,
    pub auras: Vec<AuraDefinition>,
    pub continuous_effects: Vec<ContinuousEffectDefinition>,
    pub targeting: TargetRequirement,
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
            base_keywords: card.keywords,
            program: card.effects,
            triggers: card.triggers,
            auras: card.auras,
            continuous_effects: card.continuous_effects,
            targeting: card.targeting,
        }
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct CardDefinitions(pub BTreeMap<String, CardDefinition>);

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{
        Selector, TargetAudience, TargetFilter, TargetKind, TargetRequirement, ValueExpression,
    };

    #[googletest::test]
    fn card_targeting_defaults_builds_serializes_and_reaches_its_definition() {
        let targeting = TargetRequirement::Required(TargetFilter {
            audience: TargetAudience::Enemy,
            kind: TargetKind::Character,
        });
        assert_eq!(Card::spell("Plain", 0).targeting, TargetRequirement::None);
        let card = Card::spell("Bolt", 0).with_targeting(targeting);
        let json = serde_json::to_string(&card).unwrap();
        assert_eq!(serde_json::from_str::<Card>(&json).unwrap(), card);
        assert_eq!(CardDefinition::from(card).targeting, targeting);
    }

    #[googletest::test]
    fn card_definition_preserves_all_runtime_card_data() {
        let card = Card::minion("Archivist", 4, 3, 5).with_effects(vec![Effect::Heal {
            targets: Selector::Source,
            amount: ValueExpression::Constant(2),
        }]);

        let definition = CardDefinition::from(card);

        assert_that!(definition.id, eq("synthetic:archivist"));
        assert_that!(definition.name, eq("Archivist"));
        assert_that!(definition.kind, eq(EntityKind::Minion));
        assert_that!(definition.base_cost, eq(4));
        assert_that!(definition.base_attack, eq(3));
        assert_that!(definition.base_health, eq(5));
        assert_that!(definition.base_keywords.is_empty(), is_true());
        assert_that!(definition.program.len(), eq(1));
        assert_that!(definition.triggers.is_empty(), is_true());
        assert_that!(definition.auras.is_empty(), is_true());
        assert_that!(definition.continuous_effects.is_empty(), is_true());
    }
}
