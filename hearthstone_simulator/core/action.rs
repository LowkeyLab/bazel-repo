use crate::{ChoiceId, GameEntityId, PlayerId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GameAction {
    PlayCard {
        player: PlayerId,
        card: GameEntityId,
        target: Option<GameEntityId>,
        board_index: Option<usize>,
        choice: Option<ChoiceId>,
    },
    Attack {
        player: PlayerId,
        attacker: GameEntityId,
        defender: GameEntityId,
    },
    EndTurn {
        player: PlayerId,
    },
    Concede {
        player: PlayerId,
    },
}

impl GameAction {
    pub const fn player(&self) -> PlayerId {
        match self {
            Self::PlayCard { player, .. }
            | Self::Attack { player, .. }
            | Self::EndTurn { player }
            | Self::Concede { player } => *player,
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::PlayCard { .. } => "PlayCard",
            Self::Attack { .. } => "Attack",
            Self::EndTurn { .. } => "EndTurn",
            Self::Concede { .. } => "Concede",
        }
    }
}
