use bevy::prelude::{Component, Entity};

use crate::{Keyword, PlayerId};

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TemporaryDuration {
    EndOfTurn(PlayerId),
    EndOfTurnSeries(PlayerId),
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct KeywordModifier {
    pub keyword: Keyword,
    pub granted: bool,
    pub silence_removable: bool,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StatModifier {
    pub attack: i32,
    pub health: i32,
    pub silence_removable: bool,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[relationship(relationship_target = AttachedEnchantments)]
pub struct AttachedTo(#[relationship] pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = AttachedTo, linked_spawn)]
pub struct AttachedEnchantments(#[relationship] Vec<Entity>);

impl AttachedEnchantments {
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}
