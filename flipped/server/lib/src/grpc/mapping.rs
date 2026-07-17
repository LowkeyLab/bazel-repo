use std::time::{SystemTime, UNIX_EPOCH};

use examination_proto::flipped::examination::v1 as pb;
use examination_proto::timestamp_proto::google::protobuf::Timestamp;

use crate::error::{SessionErrorCode, StreamErrorCode};
use crate::events::{
    ApplicationStatus, CardFrontProjection, CardFullProjection, ExaminerEventPayload,
    ExaminerSnapshot, ExaminerWatchItem, TestTakerEventPayload, TestTakerSnapshot,
    TestTakerWatchItem,
};

pub fn timestamp(value: SystemTime) -> Option<Timestamp> {
    let duration = value.duration_since(UNIX_EPOCH).ok()?;
    Some(Timestamp {
        seconds: i64::try_from(duration.as_secs()).ok()?,
        nanos: duration.subsec_nanos() as i32,
    })
}

pub fn session_error(code: SessionErrorCode, revision: u64) -> pb::SessionApplicationError {
    pb::SessionApplicationError {
        code: match code {
            SessionErrorCode::NotFound => pb::SessionErrorCode::NotFound,
            SessionErrorCode::Unauthenticated => pb::SessionErrorCode::Unauthenticated,
            SessionErrorCode::RoleForbidden => pb::SessionErrorCode::RoleForbidden,
            SessionErrorCode::SessionExpired => pb::SessionErrorCode::SessionExpired,
            SessionErrorCode::InvalidState => pb::SessionErrorCode::InvalidState,
            SessionErrorCode::InvalidCommandId => pb::SessionErrorCode::InvalidCommandId,
            // InvalidCursor is stream-only and is never mapped as a unary error.
            SessionErrorCode::InvalidCursor => pb::SessionErrorCode::InvalidState,
            SessionErrorCode::CommandCapacityExceeded => {
                pb::SessionErrorCode::CommandCapacityExceeded
            }
            // ResourceExhausted is admitted at the streaming transport boundary.
            SessionErrorCode::ResourceExhausted => pb::SessionErrorCode::InvalidState,
            SessionErrorCode::CredentialVersionMismatch => {
                pb::SessionErrorCode::CredentialVersionMismatch
            }
        } as i32,
        current_revision: revision,
    }
}

pub fn import_error(code: flipped_anki::ImportErrorCode) -> pb::InvalidUploadError {
    use flipped_anki::ImportErrorCode as Source;
    pb::InvalidUploadError {
        code: match code {
            Source::UploadTooLarge => pb::ImportErrorCode::UploadTooLarge,
            Source::UnsupportedExtension => pb::ImportErrorCode::UnsupportedExtension,
            Source::InvalidZip => pb::ImportErrorCode::InvalidZip,
            Source::EntryCountExceeded => pb::ImportErrorCode::EntryCountExceeded,
            Source::EntrySizeExceeded => pb::ImportErrorCode::EntrySizeExceeded,
            Source::TotalExtractedSizeExceeded => pb::ImportErrorCode::TotalExtractedSizeExceeded,
            Source::CompressionRatioExceeded => pb::ImportErrorCode::CompressionRatioExceeded,
            Source::MissingCollectionDatabase => pb::ImportErrorCode::MissingCollectionDatabase,
            Source::UnsupportedPackageVersion => pb::ImportErrorCode::UnsupportedPackageVersion,
            Source::SqliteLimitExceeded => pb::ImportErrorCode::SqliteLimitExceeded,
            Source::SqliteSchemaInvalid => pb::ImportErrorCode::SqliteSchemaInvalid,
            Source::NoSupportedNotes => pb::ImportErrorCode::NoSupportedNotes,
            Source::TooManyCards => pb::ImportErrorCode::TooManyCards,
            Source::ClozeRejected => pb::ImportErrorCode::ClozeRejected,
            Source::MediaRejected => pb::ImportErrorCode::MediaRejected,
            Source::CustomTemplateRejected => pb::ImportErrorCode::CustomTemplateRejected,
            Source::InvalidUtf8 => pb::ImportErrorCode::InvalidUtf8,
            Source::EmptyFront => pb::ImportErrorCode::EmptyFront,
            Source::EmptyBack => pb::ImportErrorCode::EmptyBack,
            Source::Cancelled => pb::ImportErrorCode::Cancelled,
        } as i32,
    }
}

impl From<ApplicationStatus> for pb::SessionStatus {
    fn from(status: ApplicationStatus) -> Self {
        match status {
            ApplicationStatus::WaitingForExaminer => Self::WaitingForExaminer,
            ApplicationStatus::Ready => Self::Ready,
            ApplicationStatus::InProgress => Self::InProgress,
            ApplicationStatus::Completed => Self::Completed,
            ApplicationStatus::Terminated => Self::Terminated,
            ApplicationStatus::Expired => Self::Expired,
        }
    }
}

impl From<CardFrontProjection> for pb::CardFront {
    fn from(card: CardFrontProjection) -> Self {
        Self {
            card_id: card.card_id,
            position: card.position,
            total: card.total,
            front: card.front,
        }
    }
}

impl From<CardFullProjection> for pb::CardFull {
    fn from(card: CardFullProjection) -> Self {
        Self {
            card_id: card.card_id,
            position: card.position,
            total: card.total,
            front: card.front,
            back: card.back,
        }
    }
}

impl From<TestTakerSnapshot> for pb::TestTakerSnapshot {
    fn from(snapshot: TestTakerSnapshot) -> Self {
        Self {
            session_id: snapshot.session_id,
            revision: snapshot.revision,
            status: pb::SessionStatus::from(snapshot.status) as i32,
            examiner_connected: snapshot.examiner_connected,
            current_card: snapshot.current_card.map(Into::into),
            expires_at: timestamp(snapshot.expires_at),
        }
    }
}

impl From<ExaminerSnapshot> for pb::ExaminerSnapshot {
    fn from(snapshot: ExaminerSnapshot) -> Self {
        Self {
            session_id: snapshot.session_id,
            revision: snapshot.revision,
            status: pb::SessionStatus::from(snapshot.status) as i32,
            test_taker_connected: snapshot.test_taker_connected,
            current_card: snapshot.current_card.map(Into::into),
            expires_at: timestamp(snapshot.expires_at),
        }
    }
}

pub fn test_taker_watch(item: TestTakerWatchItem) -> pb::TestTakerWatchResponse {
    use pb::test_taker_watch_response::Result as ResultKind;
    let result = match item {
        TestTakerWatchItem::Snapshot(snapshot) => ResultKind::Snapshot(snapshot.into()),
        TestTakerWatchItem::Error(code, revision) => {
            ResultKind::Error(stream_error(code, revision))
        }
        TestTakerWatchItem::Event(event) => {
            let payload = match event.payload {
                TestTakerEventPayload::ParticipantChanged {
                    test_taker_connected,
                    examiner_connected,
                    status,
                } => pb::test_taker_session_event::Payload::ParticipantChanged(
                    pb::ParticipantChanged {
                        test_taker_connected,
                        examiner_connected,
                        status: pb::SessionStatus::from(status) as i32,
                    },
                ),
                TestTakerEventPayload::Started(card) => {
                    pb::test_taker_session_event::Payload::Started(pb::TestTakerStarted {
                        current_card: Some(card.into()),
                    })
                }
                TestTakerEventPayload::CardChanged(card) => {
                    pb::test_taker_session_event::Payload::CardChanged(pb::TestTakerCardChanged {
                        current_card: Some(card.into()),
                    })
                }
                TestTakerEventPayload::Ended(status) => {
                    pb::test_taker_session_event::Payload::Ended(pb::SessionEnded {
                        status: pb::SessionStatus::from(status) as i32,
                    })
                }
            };
            ResultKind::Event(pb::TestTakerSessionEvent {
                revision: event.revision,
                occurred_at: timestamp(event.occurred_at),
                payload: Some(payload),
            })
        }
    };
    pb::TestTakerWatchResponse {
        result: Some(result),
    }
}

pub fn examiner_watch(item: ExaminerWatchItem) -> pb::ExaminerWatchResponse {
    use pb::examiner_watch_response::Result as ResultKind;
    let result = match item {
        ExaminerWatchItem::Snapshot(snapshot) => ResultKind::Snapshot(snapshot.into()),
        ExaminerWatchItem::Error(code, revision) => ResultKind::Error(stream_error(code, revision)),
        ExaminerWatchItem::Event(event) => {
            let payload = match event.payload {
                ExaminerEventPayload::ParticipantChanged {
                    test_taker_connected,
                    examiner_connected,
                    status,
                } => pb::examiner_session_event::Payload::ParticipantChanged(
                    pb::ParticipantChanged {
                        test_taker_connected,
                        examiner_connected,
                        status: pb::SessionStatus::from(status) as i32,
                    },
                ),
                ExaminerEventPayload::Started(card) => {
                    pb::examiner_session_event::Payload::Started(pb::ExaminerStarted {
                        current_card: Some(card.into()),
                    })
                }
                ExaminerEventPayload::CardChanged(card) => {
                    pb::examiner_session_event::Payload::CardChanged(pb::ExaminerCardChanged {
                        current_card: Some(card.into()),
                    })
                }
                ExaminerEventPayload::Ended(status) => {
                    pb::examiner_session_event::Payload::Ended(pb::SessionEnded {
                        status: pb::SessionStatus::from(status) as i32,
                    })
                }
            };
            ResultKind::Event(pb::ExaminerSessionEvent {
                revision: event.revision,
                occurred_at: timestamp(event.occurred_at),
                payload: Some(payload),
            })
        }
    };
    pb::ExaminerWatchResponse {
        result: Some(result),
    }
}

fn stream_error(code: StreamErrorCode, revision: u64) -> pb::SessionStreamError {
    pb::SessionStreamError {
        code: match code {
            StreamErrorCode::InvalidCursor => pb::StreamErrorCode::InvalidCursor,
            StreamErrorCode::ResyncRequired => pb::StreamErrorCode::ResyncRequired,
            StreamErrorCode::SessionExpired => pb::StreamErrorCode::StreamSessionExpired,
            StreamErrorCode::CredentialVersionMismatch => {
                pb::StreamErrorCode::StreamCredentialVersionMismatch
            }
        } as i32,
        current_revision: revision,
    }
}
