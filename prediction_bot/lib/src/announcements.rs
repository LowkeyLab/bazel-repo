use crate::types::{ChannelId, ConfigurationVersion, EventRevision, GuildId, MarketId, UserId};
pub(crate) mod persistence;
pub(crate) mod render;
pub(crate) mod worker;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SnapshotV1 {
    MemberEnrolled {
        user_id: UserId,
        occurred_at: i64,
    },
    BetPlaced {
        id: MarketId,
        question: String,
        bet_count: usize,
        occurred_at: i64,
    },
    Created {
        id: MarketId,
        question: String,
        creator: UserId,
        options: Vec<String>,
        closes_at: i64,
        occurred_at: i64,
    },
    Resolved {
        id: MarketId,
        question: String,
        winner: String,
        refunded: bool,
        occurred_at: i64,
    },
    Cancelled {
        id: MarketId,
        question: String,
        occurred_at: i64,
    },
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
