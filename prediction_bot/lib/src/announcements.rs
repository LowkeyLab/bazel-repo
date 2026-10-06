use crate::odds::OutcomeOdds;
use crate::types::{
    ChannelId, ConfigurationVersion, EventRevision, GuildId, MarketId, Points, UserId,
};
pub(crate) mod persistence;
pub(crate) mod render;
pub(crate) mod worker;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SnapshotV1 {
    Enabled {
        occurred_at: i64,
    },
    MemberEnrolled {
        user_id: UserId,
        occurred_at: i64,
    },
    BetPlaced {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stakes: Option<StakeSummary>,
        id: MarketId,
        question: String,
        bet_count: usize,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        odds: Vec<OutcomeOdds>,
        occurred_at: i64,
    },
    Created {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stakes: Option<StakeSummary>,
        id: MarketId,
        question: String,
        creator: UserId,
        options: Vec<String>,
        closes_at: i64,
        occurred_at: i64,
    },
    Resolved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stakes: Option<StakeSummary>,
        id: MarketId,
        question: String,
        winner: String,
        refunded: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        odds: Vec<OutcomeOdds>,
        occurred_at: i64,
    },
    Cancelled {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stakes: Option<StakeSummary>,
        id: MarketId,
        question: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        odds: Vec<OutcomeOdds>,
        occurred_at: i64,
    },
}

/// Exact event-time amounts, ordered like the saved market outcomes.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StakeSummary {
    pub total: Points,
    pub outcomes: Vec<Points>,
}

impl StakeSummary {
    pub(crate) fn for_market(market: &crate::domain::Market) -> Self {
        Self {
            total: market.total_staked,
            outcomes: market
                .options
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    Points(
                        market
                            .bets
                            .iter()
                            .filter(|bet| bet.outcome.0 == index)
                            .map(|bet| bet.amount.0)
                            .sum(),
                    )
                })
                .collect(),
        }
    }
}

impl SnapshotV1 {
    const fn stakes(&self) -> Option<&StakeSummary> {
        match self {
            Self::Created { stakes, .. }
            | Self::BetPlaced { stakes, .. }
            | Self::Resolved { stakes, .. }
            | Self::Cancelled { stakes, .. } => stakes.as_ref(),
            Self::Enabled { .. } | Self::MemberEnrolled { .. } => None,
        }
    }

    fn odds(&self) -> &[OutcomeOdds] {
        match self {
            Self::BetPlaced { odds, .. }
            | Self::Resolved { odds, .. }
            | Self::Cancelled { odds, .. } => odds,
            Self::Enabled { .. } | Self::Created { .. } | Self::MemberEnrolled { .. } => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationChange {
    Set { channel_id: ChannelId },
    Disable,
}

#[derive(Clone, Debug)]
pub struct AnnouncementStatus {
    pub channel_id: Option<ChannelId>,
    pub enabled: bool,
    pub version: ConfigurationVersion,
    pub pause_reason: Option<String>,
    pub pending: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingAnnouncement {
    pub guild: GuildId,
    pub revision: EventRevision,
    pub channel_id: ChannelId,
    pub configuration_version: ConfigurationVersion,
    pub attempts: i64,
    pub snapshot: SnapshotV1,
}

pub type Clock = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;

pub use worker::{deliver_due, start_announcement_worker};
