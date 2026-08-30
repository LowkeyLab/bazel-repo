use bevy::prelude::Resource;

use crate::PlayerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct DominantPlayer(pub PlayerId);

impl Default for DominantPlayer {
    fn default() -> Self {
        Self(PlayerId::One)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum SimulationStatus {
    SettingUp,
    AwaitingAction,
    Resolving,
    AwaitingChoice,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum GameOutcome {
    Winner(PlayerId),
    Draw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ExtraTurnTiming {
    AfterCurrentTurn,
    DuringNextTurnSeries,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ScheduledTurnKind {
    Natural,
    Extra,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ScheduledTurn {
    pub player: PlayerId,
    pub kind: ScheduledTurnKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct TurnSchedule {
    pub pending: Vec<ScheduledTurn>,
    pub next_series_extras: [u32; 2],
    pub after_current_extras: [u32; 2],
    pub after_current_anchor: Option<PlayerId>,
    pub resume_natural_player: Option<PlayerId>,
}

impl TurnSchedule {
    pub fn schedule(
        &mut self,
        active_player: PlayerId,
        player: PlayerId,
        count: u32,
        timing: ExtraTurnTiming,
    ) {
        let index = player_index(player);
        match timing {
            ExtraTurnTiming::AfterCurrentTurn => {
                debug_assert!(
                    self.after_current_anchor
                        .is_none_or(|anchor| anchor == active_player),
                    "after-current grants must share the active turn anchor"
                );
                self.after_current_anchor = Some(active_player);
                self.after_current_extras[index] =
                    self.after_current_extras[index].saturating_add(count);
            }
            ExtraTurnTiming::DuringNextTurnSeries => {
                self.next_series_extras[index] =
                    self.next_series_extras[index].saturating_add(count);
            }
        }
    }

    pub fn next_turn(&mut self, ending_player: PlayerId) -> ScheduledTurn {
        if !self.pending.is_empty() {
            return self.pending.remove(0);
        }

        if let Some(player) = self.resume_natural_player.take() {
            return self.start_natural_series(player);
        }

        let natural_player = ending_player.opponent();
        if self.next_series_extras[player_index(natural_player)] > 0 {
            // A next-series grant (Temporus) takes precedence over an after-current grant (Time
            // Warp). The preempted grant then extends its beneficiary's next natural series. This
            // normalization is independent of the order in which the effects were executed.
            for (index, extras) in self.after_current_extras.iter_mut().enumerate() {
                self.next_series_extras[index] =
                    self.next_series_extras[index].saturating_add(std::mem::take(extras));
            }
            self.after_current_anchor = None;
            return self.start_natural_series(natural_player);
        }

        if self.after_current_extras.iter().any(|count| *count > 0) {
            self.resume_natural_player = Some(natural_player);
            self.after_current_anchor = None;
            for player in PlayerId::ALL {
                let count = std::mem::take(&mut self.after_current_extras[player_index(player)]);
                self.pending.extend(std::iter::repeat_n(
                    ScheduledTurn {
                        player,
                        kind: ScheduledTurnKind::Extra,
                    },
                    count as usize,
                ));
            }
            return self.pending.remove(0);
        }

        self.start_natural_series(natural_player)
    }

    fn start_natural_series(&mut self, player: PlayerId) -> ScheduledTurn {
        let extras = std::mem::take(&mut self.next_series_extras[player_index(player)]);
        self.pending.extend(std::iter::repeat_n(
            ScheduledTurn {
                player,
                kind: ScheduledTurnKind::Extra,
            },
            extras as usize,
        ));
        ScheduledTurn {
            player,
            kind: ScheduledTurnKind::Natural,
        }
    }
}

const fn player_index(player: PlayerId) -> usize {
    match player {
        PlayerId::One => 0,
        PlayerId::Two => 1,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Resource, serde::Deserialize, serde::Serialize)]
pub struct GameState {
    pub active_player: PlayerId,
    pub turn_number: u32,
    pub outcome: Option<GameOutcome>,
    pub status: SimulationStatus,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            active_player: PlayerId::One,
            turn_number: 1,
            outcome: None,
            status: SimulationStatus::SettingUp,
        }
    }
}
