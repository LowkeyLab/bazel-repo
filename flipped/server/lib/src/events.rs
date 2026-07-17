use std::time::SystemTime;

use flipped::{ExaminerCardView, SessionStatus, TestTakerCardView};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ApplicationStatus {
    WaitingForExaminer,
    Ready,
    InProgress,
    Completed,
    Terminated,
    Expired,
}

impl From<SessionStatus> for ApplicationStatus {
    fn from(status: SessionStatus) -> Self {
        match status {
            SessionStatus::Empty | SessionStatus::HasTestTaker | SessionStatus::HasExaminer => {
                Self::WaitingForExaminer
            }
            SessionStatus::Ready => Self::Ready,
            SessionStatus::InProgress { .. } => Self::InProgress,
            SessionStatus::Completed => Self::Completed,
            SessionStatus::Terminated => Self::Terminated,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CardFrontProjection {
    pub card_id: String,
    pub position: u32,
    pub total: u32,
    pub front: String,
}

impl From<TestTakerCardView> for CardFrontProjection {
    fn from(card: TestTakerCardView) -> Self {
        Self {
            card_id: card.card_id.to_string(),
            position: card.position as u32,
            total: card.total as u32,
            front: card.front,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CardFullProjection {
    pub card_id: String,
    pub position: u32,
    pub total: u32,
    pub front: String,
    pub back: String,
}

impl From<ExaminerCardView> for CardFullProjection {
    fn from(card: ExaminerCardView) -> Self {
        Self {
            card_id: card.card_id.to_string(),
            position: card.position as u32,
            total: card.total as u32,
            front: card.front,
            back: card.back,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestTakerSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub status: ApplicationStatus,
    pub examiner_connected: bool,
    pub current_card: Option<CardFrontProjection>,
    pub expires_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExaminerSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub status: ApplicationStatus,
    pub test_taker_connected: bool,
    pub current_card: Option<CardFullProjection>,
    pub expires_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestTakerEventPayload {
    ParticipantChanged {
        test_taker_connected: bool,
        examiner_connected: bool,
        status: ApplicationStatus,
    },
    Started(CardFrontProjection),
    CardChanged(CardFrontProjection),
    Ended(ApplicationStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExaminerEventPayload {
    ParticipantChanged {
        test_taker_connected: bool,
        examiner_connected: bool,
        status: ApplicationStatus,
    },
    Started(CardFullProjection),
    CardChanged(CardFullProjection),
    Ended(ApplicationStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedEvent<T> {
    pub revision: u64,
    pub occurred_at: SystemTime,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestTakerWatchItem {
    Snapshot(TestTakerSnapshot),
    Event(VersionedEvent<TestTakerEventPayload>),
    Error(crate::error::StreamErrorCode, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExaminerWatchItem {
    Snapshot(ExaminerSnapshot),
    Event(VersionedEvent<ExaminerEventPayload>),
    Error(crate::error::StreamErrorCode, u64),
}
