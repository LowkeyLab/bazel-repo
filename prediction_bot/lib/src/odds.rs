use serde::{Deserialize, Serialize};

use crate::domain::Market;

/// An outcome's stake-weighted percentage, saved to one decimal place.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutcomeOdds {
    pub label: String,
    tenths_percent: Option<u16>,
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
                }
            })
            .collect()
    }

    pub(crate) fn chance(&self) -> String {
        self.tenths_percent.map_or_else(
            || "N/A (no bets)".to_owned(),
            |value| format!("{}.{:01}%", value / 10, value % 10),
        )
    }
}
