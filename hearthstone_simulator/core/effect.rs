use crate::{Card, GameEntityId, NativeEffectId, PlayerId, StatModifier, Zone};

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
    GainResource {
        player: PlayerSelector,
        amount: i32,
        temporary: bool,
    },
    Summon {
        player: PlayerSelector,
        card: Card,
        board_index: Option<usize>,
    },
    AttachStatModifier {
        targets: Selector,
        modifier: StatModifier,
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

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EffectContext {
    pub source: Option<GameEntityId>,
    pub controller: PlayerId,
    pub declared_target: Option<GameEntityId>,
}
