use serde::{Deserialize, Serialize};

use crate::domain::Market;

/// An outcome's stake-weighted percentage, saved to one decimal place.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeOdds {
    pub label: String,
    tenths_percent: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    movement: Option<Movement>,
    // Older V1 readers ignore this field and keep accepting the movement enum.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    unchanged: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Movement {
    Up,
    Down,
}

impl OutcomeOdds {
    pub(crate) fn for_market(market: &Market) -> Vec<Self> {
        let total = i128::from(market.total_staked.0);
        market
            .options
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let staked: i128 = market
                    .bets
                    .iter()
                    .filter(|bet| bet.outcome.0 == index)
                    .map(|bet| i128::from(bet.amount.0))
                    .sum();
                let tenths_percent = (total > 0).then(|| {
                    // Widen before multiplying; valid pools can reach i64::MAX.
                    u16::try_from((staked * 1000 + total / 2) / total)
                        .expect("an outcome's share of a valid pool is at most 100%")
                });
                Self {
                    label: label.clone(),
                    tenths_percent,
                    movement: None,
                    unchanged: false,
                }
            })
            .collect()
    }

    /// Compare displayed percentages for the same outcome before and after a bet.
    pub(crate) fn with_previous(mut self, previous: &Self) -> Self {
        self.unchanged = matches!(
            (previous.tenths_percent, self.tenths_percent),
            (Some(before), Some(after)) if before == after
        );
        self.movement = match (previous.tenths_percent, self.tenths_percent) {
            (Some(before), Some(after)) if after > before => Some(Movement::Up),
            (Some(before), Some(after)) if after < before => Some(Movement::Down),
            _ => None,
        };
        self
    }

    pub(crate) fn movement_indicator(&self) -> &'static str {
        match self.movement {
            Some(Movement::Up) => " 🟢 ⬆️",
            Some(Movement::Down) => " 🔴 ⬇️",
            None if self.unchanged => " ➖ unchanged",
            None => "",
        }
    }

    pub(crate) fn chance(&self) -> String {
        self.tenths_percent.map_or_else(
            || "N/A (no bets)".to_owned(),
            |value| format!("{}.{:01}%", value / 10, value % 10),
        )
    }
}
