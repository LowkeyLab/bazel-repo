use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use flipped::{AnySession, ExaminerParticipant, TestTakerParticipant};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::admission::WatchPermit;
use crate::command_log::CommandLog;
use crate::credentials::{InvitationRecord, InvitationStatus, OAuthTokenResponse};
use crate::error::StreamErrorCode;
use crate::events::{
    ApplicationStatus, ExaminerEventPayload, ExaminerSnapshot, ExaminerWatchItem,
    TestTakerEventPayload, TestTakerSnapshot, TestTakerWatchItem, VersionedEvent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedExaminerCommand {
    pub snapshot: ExaminerSnapshot,
}

pub(crate) struct Subscriber<T> {
    pub data: mpsc::Sender<T>,
    pub control: mpsc::Sender<T>,
    pub _permit: WatchPermit,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PublishReport {
    pub lagged: usize,
    pub closed: usize,
}

impl PublishReport {
    fn merge(&mut self, other: Self) {
        self.lagged += other.lagged;
        self.closed += other.closed;
    }
}

pub struct SessionRecord {
    pub session: AnySession,
    pub test_taker: TestTakerParticipant,
    pub examiner: Option<ExaminerParticipant>,
    pub revision: u64,
    pub expires_at: SystemTime,
    pub expired: bool,
    pub terminal_at: Option<SystemTime>,
    pub credential_version: u64,
    pub active_jtis: HashSet<Uuid>,
    pub invitation: InvitationRecord,
    pub command_log: CommandLog<CachedExaminerCommand>,
    pub test_taker_watches: HashSet<Uuid>,
    pub examiner_watches: HashSet<Uuid>,
    pub(crate) test_taker_subscribers: HashMap<Uuid, Subscriber<TestTakerWatchItem>>,
    pub(crate) examiner_subscribers: HashMap<Uuid, Subscriber<ExaminerWatchItem>>,
    pub lagged_subscribers: u64,
}

impl SessionRecord {
    pub fn status(&self) -> ApplicationStatus {
        if self.expired {
            ApplicationStatus::Expired
        } else {
            self.session.status().into()
        }
    }

    pub fn test_taker_snapshot(&self) -> TestTakerSnapshot {
        let card = if self.expired {
            None
        } else {
            self.session
                .test_taker_view(&self.test_taker)
                .ok()
                .flatten()
                .map(Into::into)
        };
        TestTakerSnapshot {
            session_id: self.session.id().to_string(),
            revision: self.revision,
            status: self.status(),
            examiner_connected: !self.examiner_watches.is_empty(),
            current_card: card,
            expires_at: self.expires_at,
        }
    }

    pub fn examiner_snapshot(&self) -> Option<ExaminerSnapshot> {
        let examiner = self.examiner.as_ref()?;
        let card = if self.expired {
            None
        } else {
            self.session
                .examiner_view(examiner)
                .ok()
                .flatten()
                .map(Into::into)
        };
        Some(ExaminerSnapshot {
            session_id: self.session.id().to_string(),
            revision: self.revision,
            status: self.status(),
            test_taker_connected: !self.test_taker_watches.is_empty(),
            current_card: card,
            expires_at: self.expires_at,
        })
    }

    pub fn publish_participant_changed(&mut self, now: SystemTime) -> PublishReport {
        let mut report = PublishReport::default();
        loop {
            let status = self.status();
            let test_taker_connected = !self.test_taker_watches.is_empty();
            let examiner_connected = !self.examiner_watches.is_empty();
            let (test_report, test_changed) = self.send_test_taker(
                TestTakerEventPayload::ParticipantChanged {
                    test_taker_connected,
                    examiner_connected,
                    status,
                },
                now,
            );
            let (examiner_report, examiner_changed) = self.send_examiner(
                ExaminerEventPayload::ParticipantChanged {
                    test_taker_connected,
                    examiner_connected,
                    status,
                },
                now,
            );
            report.merge(test_report);
            report.merge(examiner_report);
            if !test_changed && !examiner_changed {
                break;
            }
        }
        report
    }

    pub fn publish_test_taker(
        &mut self,
        payload: TestTakerEventPayload,
        now: SystemTime,
    ) -> PublishReport {
        let (mut report, changed) = self.send_test_taker(payload, now);
        if changed {
            report.merge(self.publish_participant_changed(now));
        }
        report
    }

    pub fn publish_examiner(
        &mut self,
        payload: ExaminerEventPayload,
        now: SystemTime,
    ) -> PublishReport {
        let (mut report, changed) = self.send_examiner(payload, now);
        if changed {
            report.merge(self.publish_participant_changed(now));
        }
        report
    }

    fn send_test_taker(
        &mut self,
        payload: TestTakerEventPayload,
        now: SystemTime,
    ) -> (PublishReport, bool) {
        let item = TestTakerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload,
        });
        let failed: Vec<_> = self
            .test_taker_subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                subscriber
                    .data
                    .try_send(item.clone())
                    .err()
                    .map(|error| (*id, matches!(error, mpsc::error::TrySendError::Full(_))))
            })
            .collect();
        let was_connected = !self.test_taker_watches.is_empty();
        let mut report = PublishReport::default();
        let mut controls = Vec::new();
        for (id, full) in failed {
            if let Some(subscriber) = self.test_taker_subscribers.remove(&id) {
                self.test_taker_watches.remove(&id);
                if full {
                    report.lagged += 1;
                    controls.push(subscriber.control);
                } else {
                    report.closed += 1;
                }
            }
        }
        let changed = was_connected && self.test_taker_watches.is_empty();
        if changed {
            self.revision += 1;
        }
        self.lagged_subscribers = self.lagged_subscribers.saturating_add(report.lagged as u64);
        for control in controls {
            let _ = control.try_send(TestTakerWatchItem::Error(
                StreamErrorCode::ResyncRequired,
                self.revision,
            ));
        }
        (report, changed)
    }

    fn send_examiner(
        &mut self,
        payload: ExaminerEventPayload,
        now: SystemTime,
    ) -> (PublishReport, bool) {
        let item = ExaminerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload,
        });
        let failed: Vec<_> = self
            .examiner_subscribers
            .iter()
            .filter_map(|(id, subscriber)| {
                subscriber
                    .data
                    .try_send(item.clone())
                    .err()
                    .map(|error| (*id, matches!(error, mpsc::error::TrySendError::Full(_))))
            })
            .collect();
        let was_connected = !self.examiner_watches.is_empty();
        let mut report = PublishReport::default();
        let mut controls = Vec::new();
        for (id, full) in failed {
            if let Some(subscriber) = self.examiner_subscribers.remove(&id) {
                self.examiner_watches.remove(&id);
                if full {
                    report.lagged += 1;
                    controls.push(subscriber.control);
                } else {
                    report.closed += 1;
                }
            }
        }
        let changed = was_connected && self.examiner_watches.is_empty();
        if changed {
            self.revision += 1;
        }
        self.lagged_subscribers = self.lagged_subscribers.saturating_add(report.lagged as u64);
        for control in controls {
            let _ = control.try_send(ExaminerWatchItem::Error(
                StreamErrorCode::ResyncRequired,
                self.revision,
            ));
        }
        (report, changed)
    }

    pub fn expire(&mut self, now: SystemTime) -> bool {
        if self.expired
            || matches!(
                self.session.status(),
                flipped::SessionStatus::Completed | flipped::SessionStatus::Terminated
            )
            || now < self.expires_at
        {
            return false;
        }
        self.expired = true;
        self.terminal_at.get_or_insert(now);
        self.revision += 1;
        self.credential_version += 1;
        self.active_jtis.clear();
        self.invitation.status = InvitationStatus::Revoked;
        self.invitation.cached_token_response = None;
        self.invitation.cached_until = None;
        self.expire_streams(now);
        true
    }

    pub fn tombstone_expired(&self, now: SystemTime, retention: std::time::Duration) -> bool {
        self.terminal_at
            .and_then(|terminal_at| terminal_at.checked_add(retention))
            .is_some_and(|purge_at| now >= purge_at)
    }

    pub fn finish_terminal(&mut self, status: ApplicationStatus, now: SystemTime) {
        debug_assert_eq!(status, ApplicationStatus::Terminated);
        self.terminal_at.get_or_insert(now);
        self.credential_version += 1;
        self.active_jtis.clear();
        self.invitation.status = InvitationStatus::Revoked;
        self.invitation.cached_token_response = None;
        self.invitation.cached_until = None;

        let test_event = TestTakerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload: TestTakerEventPayload::Ended(status),
        });
        for (_, subscriber) in self.test_taker_subscribers.drain() {
            let _ = subscriber.control.try_send(test_event.clone());
        }
        let examiner_event = ExaminerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload: ExaminerEventPayload::Ended(status),
        });
        for (_, subscriber) in self.examiner_subscribers.drain() {
            let _ = subscriber.control.try_send(examiner_event.clone());
        }
        self.test_taker_watches.clear();
        self.examiner_watches.clear();
    }

    fn expire_streams(&mut self, now: SystemTime) {
        let test_event = TestTakerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload: TestTakerEventPayload::Ended(ApplicationStatus::Expired),
        });
        for (_, subscriber) in self.test_taker_subscribers.drain() {
            let _ = subscriber.control.try_send(test_event.clone());
            let _ = subscriber.control.try_send(TestTakerWatchItem::Error(
                StreamErrorCode::SessionExpired,
                self.revision,
            ));
        }
        let examiner_event = ExaminerWatchItem::Event(VersionedEvent {
            revision: self.revision,
            occurred_at: now,
            payload: ExaminerEventPayload::Ended(ApplicationStatus::Expired),
        });
        for (_, subscriber) in self.examiner_subscribers.drain() {
            let _ = subscriber.control.try_send(examiner_event.clone());
            let _ = subscriber.control.try_send(ExaminerWatchItem::Error(
                StreamErrorCode::SessionExpired,
                self.revision,
            ));
        }
        self.test_taker_watches.clear();
        self.examiner_watches.clear();
    }

    pub fn cache_oauth_response(&mut self, response: &OAuthTokenResponse, bytes: Vec<u8>) {
        let _ = response;
        self.invitation.cached_token_response = Some(bytes);
    }
}

#[derive(Default)]
struct StoreState {
    sessions: HashMap<String, Arc<Mutex<SessionRecord>>>,
    invitation_index: HashMap<[u8; 32], String>,
    session_invitations: HashMap<String, [u8; 32]>,
}

#[derive(Clone, Default)]
pub struct InMemoryStore {
    state: Arc<Mutex<StoreState>>,
}

impl InMemoryStore {
    pub async fn insert_bounded(
        &self,
        record: SessionRecord,
        capacity: usize,
    ) -> Option<Arc<Mutex<SessionRecord>>> {
        let id = record.session.id().to_string();
        let invitation_digest = record.invitation.digest;
        let mut state = self.state.lock().await;
        if capacity == 0
            || state.sessions.len() >= capacity
            || state.sessions.contains_key(&id)
            || state.invitation_index.contains_key(&invitation_digest)
        {
            return None;
        }
        let record = Arc::new(Mutex::new(record));
        state.invitation_index.insert(invitation_digest, id.clone());
        state
            .session_invitations
            .insert(id.clone(), invitation_digest);
        state.sessions.insert(id, Arc::clone(&record));
        Some(record)
    }

    pub async fn get(&self, session_id: &str) -> Option<Arc<Mutex<SessionRecord>>> {
        self.state.lock().await.sessions.get(session_id).cloned()
    }

    pub async fn get_by_invitation_digest(
        &self,
        digest: &[u8; 32],
    ) -> Option<Arc<Mutex<SessionRecord>>> {
        let state = self.state.lock().await;
        let session_id = state.invitation_index.get(digest)?;
        state.sessions.get(session_id).cloned()
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<Mutex<SessionRecord>>> {
        let mut state = self.state.lock().await;
        if let Some(digest) = state.session_invitations.remove(session_id) {
            state.invitation_index.remove(&digest);
        }
        state.sessions.remove(session_id)
    }

    pub async fn all(&self) -> Vec<Arc<Mutex<SessionRecord>>> {
        self.state.lock().await.sessions.values().cloned().collect()
    }

    pub async fn len(&self) -> usize {
        self.state.lock().await.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flipped::{Deck, Flashcard, ParticipantId, Session};

    fn record(expires_at: SystemTime) -> SessionRecord {
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let deck = Deck::new(
            None,
            vec![Flashcard::new("front", "back").expect("valid card")],
        )
        .expect("non-empty deck");
        let session = AnySession::HasTestTaker(Session::new(deck).join_test_taker(test_taker));
        let session_id = session.id().to_string();
        SessionRecord {
            session,
            test_taker,
            examiner: None,
            revision: 1,
            expires_at,
            expired: false,
            terminal_at: None,
            credential_version: 1,
            active_jtis: HashSet::from([Uuid::now_v7()]),
            invitation: InvitationRecord {
                session_id,
                digest: [7; 32],
                expires_at,
                bound_client_id: "client".to_owned(),
                bound_audience: "audience".to_owned(),
                status: InvitationStatus::Available,
                consumed_at: None,
                redemption_id: None,
                request_hash: None,
                cached_token_response: None,
                cached_until: None,
            },
            command_log: CommandLog::new(4),
            test_taker_watches: HashSet::new(),
            examiner_watches: HashSet::new(),
            test_taker_subscribers: HashMap::new(),
            examiner_subscribers: HashMap::new(),
            lagged_subscribers: 0,
        }
    }

    #[test]
    fn expiry_is_authoritative_and_idempotent() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let mut record = record(now);
        assert!(record.expire(now));
        assert!(record.expired);
        assert_eq!(record.revision, 2);
        assert_eq!(record.credential_version, 2);
        assert!(record.active_jtis.is_empty());
        assert_eq!(record.invitation.status, InvitationStatus::Revoked);
        assert_eq!(record.status(), ApplicationStatus::Expired);
        assert!(!record.expire(now + std::time::Duration::from_secs(1)));
        assert_eq!(record.revision, 2);
    }

    #[tokio::test]
    async fn store_bounds_sessions_and_indexes_only_invitation_digests() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let store = InMemoryStore::default();
        let first = record(now + std::time::Duration::from_secs(60));
        let first_id = first.session.id().to_string();
        let digest = first.invitation.digest;
        store
            .insert_bounded(first, 1)
            .await
            .expect("first session fits");
        assert!(store.get_by_invitation_digest(&digest).await.is_some());
        assert!(
            store
                .insert_bounded(record(now + std::time::Duration::from_secs(60)), 1)
                .await
                .is_none(),
            "capacity cannot grow without bound",
        );
        store.remove(&first_id).await.expect("session is removed");
        assert!(store.get_by_invitation_digest(&digest).await.is_none());
    }

    #[test]
    fn tombstone_retention_is_bounded_from_terminal_time() {
        let terminal = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let mut record = record(terminal);
        assert!(record.expire(terminal));
        assert!(!record.tombstone_expired(
            terminal + std::time::Duration::from_secs(4),
            std::time::Duration::from_secs(5),
        ));
        assert!(record.tombstone_expired(
            terminal + std::time::Duration::from_secs(5),
            std::time::Duration::from_secs(5),
        ));
    }

    #[tokio::test]
    async fn saturated_last_subscriber_gets_resync_at_disconnect_revision() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let mut record = record(now + std::time::Duration::from_secs(60));
        let watch_id = Uuid::now_v7();
        let (data, _data_rx) = mpsc::channel(1);
        let (control, mut control_rx) = mpsc::channel(2);
        data.try_send(TestTakerWatchItem::Error(
            StreamErrorCode::ResyncRequired,
            record.revision,
        ))
        .expect("prefill queue");
        record.test_taker_watches.insert(watch_id);
        let permit = crate::admission::AdmissionController::new(1, 1, 1)
            .try_reserve_watch(0)
            .expect("watch admitted");
        record.test_taker_subscribers.insert(
            watch_id,
            Subscriber {
                data,
                control,
                _permit: permit,
            },
        );

        let report = record.publish_test_taker(
            TestTakerEventPayload::Ended(ApplicationStatus::Terminated),
            now,
        );

        assert_eq!(report.lagged, 1);
        assert!(record.test_taker_watches.is_empty());
        assert_eq!(record.revision, 2);
        assert_eq!(
            control_rx.recv().await,
            Some(TestTakerWatchItem::Error(
                StreamErrorCode::ResyncRequired,
                2,
            ))
        );
    }
}
