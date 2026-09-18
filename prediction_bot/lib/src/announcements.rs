pub(crate) mod persistence;
pub(crate) mod render;
pub(crate) mod worker;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SnapshotV1 {
    Created {
        id: String,
        question: String,
        creator: u64,
        options: Vec<String>,
        closes_at: i64,
        occurred_at: i64,
    },
    Resolved {
        id: String,
        question: String,
        winner: String,
        refunded: bool,
        occurred_at: i64,
    },
    Cancelled {
        id: String,
        question: String,
        occurred_at: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationChange {
    Set { channel_id: u64 },
    Disable,
}

#[derive(Clone, Debug)]
pub struct AnnouncementStatus {
    pub channel_id: Option<u64>,
    pub enabled: bool,
    pub version: i64,
    pub pause_reason: Option<String>,
    pub pending: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingAnnouncement {
    pub guild: u64,
    pub revision: i64,
    pub channel_id: u64,
    pub configuration_version: i64,
    pub attempts: i64,
    pub snapshot: SnapshotV1,
}

pub type Clock = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;
