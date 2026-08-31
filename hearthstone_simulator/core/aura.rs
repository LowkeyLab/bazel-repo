use bevy::prelude::Component;

use crate::GameEntityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum AuraTarget {
    FriendlyMinions,
    OtherFriendlyMinions,
    EnemyMinions,
    AllMinions,
    FriendlyCharacters,
    OtherFriendlyCharacters,
    EnemyCharacters,
    AllCharacters,
    ControllerPlayer,
    OpponentPlayer,
    BothPlayers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum OtherAuraModifier {
    Immune,
    HeroPowerDamage(i32),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuraDefinition {
    pub targets: AuraTarget,
    pub attack: i32,
    pub health: i32,
    pub other: Vec<OtherAuraModifier>,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RuntimeAuras(pub Vec<AuraDefinition>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PlayerAudience {
    Controller,
    Opponent,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ContinuousModifier {
    SpellDamage(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ContinuousEffectDefinition {
    pub recipients: PlayerAudience,
    pub modifier: ContinuousModifier,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RuntimeContinuousEffects(pub Vec<ContinuousEffectDefinition>);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SilenceRemovable;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum AuraModifier {
    Attack(i32),
    MaximumHealth(i32),
    Immune,
    HeroPowerDamage(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AuraApplication {
    pub provider: GameEntityId,
    pub definition_index: u32,
    pub modifier: AuraModifier,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum AuraCategory {
    Health,
    Attack,
    Other,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct HealthAuraCache(pub Vec<AuraApplication>);

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AttackAuraCache(pub Vec<AuraApplication>);

#[derive(Component, Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct OtherAuraCache(pub Vec<AuraApplication>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum AuraRefreshPlan {
    PlayedProvider(GameEntityId),
    Summon,
}
