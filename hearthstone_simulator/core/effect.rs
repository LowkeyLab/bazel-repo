use crate::{
    Card, ContinuousEffectDefinition, CostModifier, EnchantmentDuration, ExtraTurnTiming,
    GameEntityId, HeroClass, KeywordModifier, NativeEffectId, PlayerId, StatModifier, Zone,
    ZoneMovementKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum HeroClassPolicy {
    Keep,
    Replace(HeroClass),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum HeroHealthPolicy {
    Preserve,
    Set {
        maximum_health: i32,
        current_health: i32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct HeroReplacement {
    pub hero: Card,
    pub hero_power: Card,
    pub armor_gain: i32,
    pub health: HeroHealthPolicy,
    pub class: HeroClassPolicy,
    pub weapon: Option<Card>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Effect {
    DealDamage {
        targets: Selector,
        amount: ValueExpression,
    },
    Heal {
        targets: Selector,
        amount: ValueExpression,
    },
    ModifyEventValue {
        operation: EventValueOperation,
        value: ValueExpression,
    },
    Destroy {
        targets: Selector,
    },
    Draw {
        player: PlayerSelector,
        count: u32,
    },
    Move {
        targets: Selector,
        player: PlayerSelector,
        zone: Zone,
        kind: ZoneMovementKind,
    },
    GainResource {
        player: PlayerSelector,
        amount: i32,
        temporary: bool,
    },
    ScheduleExtraTurns {
        player: PlayerSelector,
        count: u32,
        timing: ExtraTurnTiming,
    },
    ReplaceHero {
        player: PlayerSelector,
        replacement: Box<HeroReplacement>,
    },
    Summon {
        player: PlayerSelector,
        card: Card,
        board_index: Option<usize>,
    },
    AttachStatModifier {
        targets: Selector,
        modifier: StatModifier,
        duration: EnchantmentDuration,
    },
    AttachKeywordModifier {
        targets: Selector,
        modifier: KeywordModifier,
        duration: EnchantmentDuration,
    },
    AttachCostModifier {
        targets: Selector,
        modifier: CostModifier,
        duration: EnchantmentDuration,
    },
    AttachContinuousEffect {
        targets: Selector,
        effect: ContinuousEffectDefinition,
        silence_removable: bool,
        duration: EnchantmentDuration,
    },
    Silence {
        targets: Selector,
    },
    Transform {
        targets: Selector,
        card: Card,
    },
    Copy {
        targets: Selector,
        player: PlayerSelector,
        zone: Zone,
    },
    Native(NativeEffectId),
    Sequence(Vec<Effect>),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Selector {
    Source,
    DeclaredTarget,
    Entity(GameEntityId),
    FriendlyMinions,
    EnemyMinions,
    AllMinions,
    FriendlyCharacters,
    EnemyCharacters,
    AllCharacters,
    InZone { player: PlayerSelector, zone: Zone },
    Random(Box<Selector>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PlayerSelector {
    Controller,
    Opponent,
    Player(PlayerId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum EventValueOperation {
    Replace,
    Add,
    Multiply,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ValueExpression {
    Constant(i32),
    SourceAttack,
    TargetCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum EffectOrigin {
    Other,
    Spell,
    HeroPower,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EffectContext {
    pub source: Option<GameEntityId>,
    pub controller: PlayerId,
    pub declared_target: Option<GameEntityId>,
    pub origin: EffectOrigin,
}
