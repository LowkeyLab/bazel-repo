use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use base64::Engine;
use flipped::{AnySession, Deck, ParticipantId, Session, TestTakerParticipant};
use flipped_anki::{
    ImportCancellation, ImportLimits, import_apkg_file_with_cancellation,
    import_apkg_with_cancellation,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::admission::{AdmissionController, ImportPermit};
use crate::command_log::{CachedCommandResult, Operation, command_input_hash};
use crate::credentials::{
    AccessClaims, AccessRole, CredentialService, InvitationStatus, OAuthTokenResponse,
    parse_canonical_uuid_v7, token_exchange_request_hash,
};
use crate::error::{Result, ServerError, SessionApplicationError, SessionErrorCode};
use crate::events::{
    ApplicationStatus, ExaminerEventPayload, ExaminerSnapshot, ExaminerWatchItem,
    TestTakerEventPayload, TestTakerSnapshot, TestTakerWatchItem,
};
use crate::observability::{
    EventDispatcher, EventErrorCode, Outcome, ServiceEvent, ServiceEventName, Severity,
    current_event_context,
};
use crate::store::{CachedExaminerCommand, InMemoryStore, SessionRecord, Subscriber};

const TOKEN_EXCHANGE_GRANT: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const INVITATION_TOKEN_TYPE: &str = "urn:flipped:params:oauth:token-type:examiner-invitation";
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

pub(crate) struct RequestEventGuard {
    events: EventDispatcher,
    name: ServiceEventName,
    started: Instant,
}

impl Drop for RequestEventGuard {
    fn drop(&mut self) {
        self.events.emit(
            Severity::Info,
            current_event_context(),
            ServiceEvent {
                name: self.name,
                outcome: Outcome::Success,
                error_code: None,
                duration_ms: Some(
                    self.started
                        .elapsed()
                        .as_millis()
                        .try_into()
                        .unwrap_or(u64::MAX),
                ),
            },
        );
    }
}

#[derive(Debug, Clone)]
pub struct CreatedSession {
    pub session_id: String,
    pub test_taker_access_token: String,
    pub examiner_invitation: String,
    pub expires_at: SystemTime,
    pub card_count: u32,
    pub revision: u64,
    pub initial_snapshot: TestTakerSnapshot,
}

#[derive(Debug, Clone)]
pub struct TokenExchangeRequest<'a> {
    pub client_id: &'a str,
    pub grant_type: &'a str,
    pub subject_token: &'a str,
    pub subject_token_type: &'a str,
    pub requested_token_type: &'a str,
    pub audience: &'a str,
    pub scope: &'a str,
    pub redemption_id: Uuid,
}

#[derive(Clone)]
pub struct Application {
    store: InMemoryStore,
    credentials: Arc<CredentialService>,
    events: EventDispatcher,
    import_limits: ImportLimits,
    oauth_client_id: String,
    audience: String,
    session_ttl: std::time::Duration,
    redemption_retry: std::time::Duration,
    command_capacity: usize,
    stream_capacity: usize,
    max_sessions: usize,
    admission: AdmissionController,
    tombstone_retention: std::time::Duration,
    session_ref_key: [u8; 32],
}

impl Application {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: InMemoryStore,
        credentials: Arc<CredentialService>,
        events: EventDispatcher,
        import_limits: ImportLimits,
        oauth_client_id: String,
        audience: String,
        session_ttl: std::time::Duration,
        redemption_retry: std::time::Duration,
        command_capacity: usize,
        stream_capacity: usize,
        max_sessions: usize,
        max_concurrent_imports: usize,
        max_global_watches: usize,
        max_watches_per_session: usize,
        tombstone_retention: std::time::Duration,
        session_ref_key: [u8; 32],
    ) -> Self {
        Self {
            store,
            credentials,
            events,
            import_limits,
            oauth_client_id,
            audience,
            session_ttl,
            redemption_retry,
            command_capacity,
            stream_capacity,
            max_sessions,
            admission: AdmissionController::new(
                max_concurrent_imports,
                max_global_watches,
                max_watches_per_session,
            ),
            tombstone_retention,
            session_ref_key,
        }
    }

    pub fn credentials(&self) -> &CredentialService {
        &self.credentials
    }

    pub(crate) fn request_event(&self, name: ServiceEventName) -> RequestEventGuard {
        RequestEventGuard {
            events: self.events.clone(),
            name,
            started: Instant::now(),
        }
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.import_limits.max_upload_bytes
    }

    pub fn reserve_import(&self) -> Result<ImportPermit> {
        self.admission.try_reserve_import().map_err(|error| {
            self.emit(
                ServiceEventName::ImportCapacityRejected,
                Outcome::Rejected,
                Some(EventErrorCode::ResourceExhausted),
                None,
            );
            error
        })
    }

    pub async fn create_session(
        &self,
        bytes: &[u8],
        extension: &str,
        now: SystemTime,
    ) -> Result<CreatedSession> {
        let permit = self.reserve_import()?;
        self.create_session_reserved(bytes, extension, now, permit)
            .await
    }

    async fn create_session_reserved(
        &self,
        bytes: &[u8],
        extension: &str,
        now: SystemTime,
        _permit: ImportPermit,
    ) -> Result<CreatedSession> {
        let bytes = bytes.to_vec();
        let extension = extension.to_owned();
        let limits = self.import_limits.clone();
        let cancellation = ImportCancellation::new();
        let worker_cancellation = cancellation.clone();
        let mut cancellation_guard = ImportCancellationGuard::new(cancellation);
        self.emit(
            ServiceEventName::ImportStarted,
            Outcome::Success,
            None,
            None,
        );
        let imported = tokio::task::spawn_blocking(move || {
            import_apkg_with_cancellation(&bytes, &extension, &limits, &worker_cancellation)
        })
        .await;
        cancellation_guard.disarm();
        let deck = imported
            .map_err(|_| ServerError::Internal)?
            .map_err(|error| {
                self.emit(
                    ServiceEventName::ImportRejected,
                    if error.code == flipped_anki::ImportErrorCode::Cancelled {
                        Outcome::Cancelled
                    } else {
                        Outcome::Rejected
                    },
                    Some(error.code.into()),
                    None,
                );
                ServerError::Import(error.code)
            })?;
        self.create_session_from_deck(deck, now).await
    }

    pub async fn create_session_from_file(
        &self,
        path: PathBuf,
        extension: String,
        now: SystemTime,
    ) -> Result<CreatedSession> {
        let permit = self.reserve_import()?;
        self.create_session_from_file_reserved(path, extension, now, permit)
            .await
    }

    pub async fn create_session_from_file_reserved(
        &self,
        path: PathBuf,
        extension: String,
        now: SystemTime,
        _permit: ImportPermit,
    ) -> Result<CreatedSession> {
        let limits = self.import_limits.clone();
        let cancellation = ImportCancellation::new();
        let worker_cancellation = cancellation.clone();
        let mut cancellation_guard = ImportCancellationGuard::new(cancellation);
        self.emit(
            ServiceEventName::ImportStarted,
            Outcome::Success,
            None,
            None,
        );
        let imported = tokio::task::spawn_blocking(move || {
            import_apkg_file_with_cancellation(&path, &extension, &limits, &worker_cancellation)
        })
        .await;
        cancellation_guard.disarm();
        let deck = imported
            .map_err(|_| ServerError::Internal)?
            .map_err(|error| {
                self.emit(
                    ServiceEventName::ImportRejected,
                    if error.code == flipped_anki::ImportErrorCode::Cancelled {
                        Outcome::Cancelled
                    } else {
                        Outcome::Rejected
                    },
                    Some(error.code.into()),
                    None,
                );
                ServerError::Import(error.code)
            })?;
        self.create_session_from_deck(deck, now).await
    }

    async fn create_session_from_deck(
        &self,
        deck: Deck,
        now: SystemTime,
    ) -> Result<CreatedSession> {
        let card_count = deck.len() as u32;
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let domain = AnySession::HasTestTaker(Session::new(deck).join_test_taker(test_taker));
        let session_id = domain.id().to_string();
        let expires_at = now + self.session_ttl;
        let (access_token, access_claims) =
            self.credentials
                .issue_access_token(&session_id, AccessRole::TestTaker, 1, now)?;
        let (invitation, invitation_record) = self.credentials.issue_invitation(
            &session_id,
            &self.oauth_client_id,
            &self.audience,
            now,
        )?;
        let jti = access_claims
            .jti
            .parse()
            .map_err(|_| ServerError::Internal)?;
        let record = SessionRecord {
            session: domain,
            test_taker,
            examiner: None,
            revision: 1,
            expires_at,
            expired: false,
            terminal_at: None,
            credential_version: 1,
            active_jtis: HashSet::from([jti]),
            invitation: invitation_record,
            command_log: crate::command_log::CommandLog::new(self.command_capacity),
            test_taker_watches: HashSet::new(),
            examiner_watches: HashSet::new(),
            test_taker_subscribers: Default::default(),
            examiner_subscribers: Default::default(),
            lagged_subscribers: 0,
        };
        let snapshot = record.test_taker_snapshot();
        if self
            .store
            .insert_bounded(record, self.max_sessions)
            .await
            .is_none()
        {
            return Err(ServerError::ResourceExhausted);
        }
        self.emit(
            ServiceEventName::ImportCompleted,
            Outcome::Success,
            None,
            Some(&session_id),
        );
        self.emit(
            ServiceEventName::SessionCreated,
            Outcome::Success,
            None,
            Some(&session_id),
        );
        self.emit(
            ServiceEventName::InvitationIssued,
            Outcome::Success,
            None,
            Some(&session_id),
        );
        Ok(CreatedSession {
            session_id,
            test_taker_access_token: access_token,
            examiner_invitation: invitation,
            expires_at,
            card_count,
            revision: 1,
            initial_snapshot: snapshot,
        })
    }

    pub fn authenticate(&self, token: &str, now: SystemTime) -> Result<AccessClaims> {
        self.credentials.validate_access_token(token, now)
    }

    pub async fn test_taker_snapshot(
        &self,
        session_id: &str,
        claims: &AccessClaims,
        now: SystemTime,
    ) -> std::result::Result<TestTakerSnapshot, SessionApplicationError> {
        let record = self
            .store
            .get(session_id)
            .await
            .ok_or_else(|| SessionApplicationError::new(SessionErrorCode::NotFound, 0))?;
        let record = record.lock().await;
        authorize(&record, session_id, claims, AccessRole::TestTaker, now)
            .map_err(|code| SessionApplicationError::new(code, record.revision))?;
        Ok(record.test_taker_snapshot())
    }

    pub async fn examiner_snapshot(
        &self,
        session_id: &str,
        claims: &AccessClaims,
        now: SystemTime,
    ) -> std::result::Result<ExaminerSnapshot, SessionApplicationError> {
        let record = self
            .store
            .get(session_id)
            .await
            .ok_or_else(|| SessionApplicationError::new(SessionErrorCode::NotFound, 0))?;
        let record = record.lock().await;
        authorize(&record, session_id, claims, AccessRole::Examiner, now)
            .map_err(|code| SessionApplicationError::new(code, record.revision))?;
        record.examiner_snapshot().ok_or_else(|| {
            SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
        })
    }

    pub async fn redeem_invitation(
        &self,
        request: TokenExchangeRequest<'_>,
        now: SystemTime,
    ) -> std::result::Result<Vec<u8>, &'static str> {
        if request.grant_type != TOKEN_EXCHANGE_GRANT {
            return Err("unsupported_grant_type");
        }
        if request.subject_token_type != INVITATION_TOKEN_TYPE
            || request.requested_token_type != ACCESS_TOKEN_TYPE
        {
            return Err("invalid_request");
        }
        if request.audience != self.audience {
            return Err("invalid_target");
        }
        if request.scope != "session:examine" {
            return Err("invalid_scope");
        }
        let request_hash = token_exchange_request_hash(&[
            request.grant_type,
            request.subject_token_type,
            request.requested_token_type,
            request.audience,
            request.scope,
            request.client_id,
        ]);
        let digest = self.credentials.hash_invitation(request.subject_token);
        if let Some(record) = self.store.get_by_invitation_digest(&digest).await {
            let mut record = record.lock().await;
            if !bool::from(record.invitation.digest.ct_eq(&digest)) {
                return Err("invalid_grant");
            }
            if record.invitation.status == InvitationStatus::Consumed {
                if record.invitation.redemption_id == Some(request.redemption_id)
                    && record.invitation.request_hash == Some(request_hash)
                    && record
                        .invitation
                        .cached_until
                        .is_some_and(|until| now <= until)
                {
                    return record
                        .invitation
                        .cached_token_response
                        .clone()
                        .ok_or("server_error");
                }
                return Err("invalid_grant");
            }
            if record.invitation.status != InvitationStatus::Available
                || request.client_id != record.invitation.bound_client_id
                || request.audience != record.invitation.bound_audience
                || now >= record.invitation.expires_at
                || now >= record.expires_at
                || record.expired
                || record.examiner.is_some()
            {
                self.emit(
                    ServiceEventName::InvitationRejected,
                    Outcome::Rejected,
                    Some(EventErrorCode::InvalidGrant),
                    Some(&record.session.id().to_string()),
                );
                return Err("invalid_grant");
            }

            let examiner = flipped::ExaminerParticipant::new(ParticipantId::new());
            let prospective_session = record
                .session
                .clone()
                .join_examiner(examiner)
                .map_err(|_| "invalid_grant")?;
            let (token, claims) = self
                .credentials
                .issue_access_token(
                    &record.session.id().to_string(),
                    AccessRole::Examiner,
                    record.credential_version,
                    now,
                )
                .map_err(|_| "server_error")?;
            let response = OAuthTokenResponse {
                access_token: token,
                issued_token_type: ACCESS_TOKEN_TYPE.to_owned(),
                token_type: "Bearer".to_owned(),
                expires_in: claims.exp.saturating_sub(claims.iat),
                scope: "session:examine".to_owned(),
            };
            let serialized = serde_json::to_vec(&response).map_err(|_| "server_error")?;
            let jti = claims.jti.parse().map_err(|_| "server_error")?;

            record.session = prospective_session;
            record.examiner = Some(examiner);
            record.active_jtis.insert(jti);
            record.revision += 1;
            record.invitation.status = InvitationStatus::Consumed;
            record.invitation.consumed_at = Some(now);
            record.invitation.redemption_id = Some(request.redemption_id);
            record.invitation.request_hash = Some(request_hash);
            record.invitation.cached_token_response = Some(serialized.clone());
            record.invitation.cached_until = Some(now + self.redemption_retry);
            record.publish_participant_changed(now);
            let session_id = record.session.id().to_string();
            drop(record);
            self.emit(
                ServiceEventName::SessionExaminerJoined,
                Outcome::Success,
                None,
                Some(&session_id),
            );
            self.emit(
                ServiceEventName::InvitationExchanged,
                Outcome::Success,
                None,
                Some(&session_id),
            );
            return Ok(serialized);
        }
        Err("invalid_grant")
    }

    pub async fn command(
        &self,
        session_id: &str,
        claims: &AccessClaims,
        command_id: &str,
        operation: Operation,
        now: SystemTime,
    ) -> std::result::Result<ExaminerSnapshot, SessionApplicationError> {
        let record = self
            .store
            .get(session_id)
            .await
            .ok_or_else(|| SessionApplicationError::new(SessionErrorCode::NotFound, 0))?;
        let mut record = record.lock().await;
        let command_id = parse_canonical_uuid_v7(command_id).map_err(|_| {
            SessionApplicationError::new(SessionErrorCode::InvalidCommandId, record.revision)
        })?;
        let jti: Uuid = claims.jti.parse().map_err(|_| {
            SessionApplicationError::new(SessionErrorCode::Unauthenticated, record.revision)
        })?;
        let hash = command_input_hash(session_id, operation);
        if let Some(cached) = record
            .command_log
            .lookup(claims.role, jti, command_id, operation, hash)
            .map_err(|code| SessionApplicationError::new(code, record.revision))?
        {
            return cached.value.map(|value| value.snapshot);
        }
        if !record.command_log.has_capacity() {
            return Err(SessionApplicationError::new(
                SessionErrorCode::CommandCapacityExceeded,
                record.revision,
            ));
        }
        if let Err(code) = authorize(&record, session_id, claims, AccessRole::Examiner, now) {
            return cache_command_rejection(
                &mut record,
                claims.role,
                jti,
                command_id,
                operation,
                hash,
                code,
            );
        }
        let Some(examiner) = record.examiner else {
            return cache_command_rejection(
                &mut record,
                claims.role,
                jti,
                command_id,
                operation,
                hash,
                SessionErrorCode::RoleForbidden,
            );
        };
        let next = match operation {
            Operation::Start => record.session.clone().start(&examiner),
            Operation::Advance => record.session.clone().advance(&examiner),
            Operation::End => record.session.clone().end(&examiner),
        };
        let next = match next {
            Ok(next) => next,
            Err(_) => {
                return cache_command_rejection(
                    &mut record,
                    claims.role,
                    jti,
                    command_id,
                    operation,
                    hash,
                    SessionErrorCode::InvalidState,
                );
            }
        };
        record.session = next;
        record.revision += 1;
        let snapshot = record.examiner_snapshot().ok_or_else(|| {
            SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
        })?;
        let test_snapshot = record.test_taker_snapshot();
        let lagged_before = record.lagged_subscribers;
        match operation {
            Operation::Start => {
                if let (Some(front), Some(full)) =
                    (test_snapshot.current_card, snapshot.current_card.clone())
                {
                    record.publish_test_taker(TestTakerEventPayload::Started(front), now);
                    record.publish_examiner(ExaminerEventPayload::Started(full), now);
                }
            }
            Operation::Advance => {
                if let (Some(front), Some(full)) =
                    (test_snapshot.current_card, snapshot.current_card.clone())
                {
                    record.publish_test_taker(TestTakerEventPayload::CardChanged(front), now);
                    record.publish_examiner(ExaminerEventPayload::CardChanged(full), now);
                } else {
                    record.publish_test_taker(
                        TestTakerEventPayload::Ended(ApplicationStatus::Completed),
                        now,
                    );
                    record.publish_examiner(
                        ExaminerEventPayload::Ended(ApplicationStatus::Completed),
                        now,
                    );
                }
            }
            Operation::End => {
                record.finish_terminal(ApplicationStatus::Terminated, now);
            }
        }
        // Lag/cancellation publication can itself advance the authoritative revision.
        // Cache and return the post-publication projection rather than a stale pre-send one.
        let snapshot = record.examiner_snapshot().ok_or_else(|| {
            SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
        })?;
        record
            .command_log
            .insert(
                claims.role,
                jti,
                command_id,
                operation,
                hash,
                CachedCommandResult {
                    value: Ok(CachedExaminerCommand {
                        snapshot: snapshot.clone(),
                    }),
                },
            )
            .map_err(|code| SessionApplicationError::new(code, record.revision))?;
        let session_id = record.session.id().to_string();
        let subscriber_lagged = record.lagged_subscribers > lagged_before;
        drop(record);
        if subscriber_lagged {
            self.emit(
                ServiceEventName::GrpcSubscriberLagged,
                Outcome::Failure,
                Some(EventErrorCode::ResyncRequired),
                Some(&session_id),
            );
        }
        let event_name = match (operation, snapshot.status) {
            (Operation::Start, _) => ServiceEventName::SessionStarted,
            (Operation::Advance, ApplicationStatus::Completed) => ServiceEventName::SessionEnded,
            (Operation::Advance, _) => ServiceEventName::SessionAdvanced,
            (Operation::End, _) => ServiceEventName::SessionEnded,
        };
        self.emit(event_name, Outcome::Success, None, Some(&session_id));
        Ok(snapshot)
    }

    pub async fn watch_test_taker(
        &self,
        session_id: &str,
        claims: &AccessClaims,
        after_revision: u64,
        now: SystemTime,
    ) -> std::result::Result<
        (
            Uuid,
            mpsc::Receiver<TestTakerWatchItem>,
            mpsc::Receiver<TestTakerWatchItem>,
        ),
        SessionApplicationError,
    > {
        let record = self
            .store
            .get(session_id)
            .await
            .ok_or_else(|| SessionApplicationError::new(SessionErrorCode::NotFound, 0))?;
        let mut record = record.lock().await;
        authorize(&record, session_id, claims, AccessRole::TestTaker, now)
            .map_err(|code| SessionApplicationError::new(code, record.revision))?;
        if after_revision > record.revision {
            return Err(SessionApplicationError::new(
                SessionErrorCode::InvalidCursor,
                record.revision,
            ));
        }
        let permit = self
            .admission
            .try_reserve_watch(record.test_taker_watches.len() + record.examiner_watches.len())
            .ok_or_else(|| {
                self.emit(
                    ServiceEventName::GrpcWatchRejected,
                    Outcome::Rejected,
                    Some(EventErrorCode::ResourceExhausted),
                    Some(session_id),
                );
                SessionApplicationError::new(SessionErrorCode::ResourceExhausted, record.revision)
            })?;
        let watch_id = Uuid::now_v7();
        let (data_tx, data_rx) = mpsc::channel(self.stream_capacity);
        let (control_tx, control_rx) = mpsc::channel(2);
        let first = record.test_taker_watches.is_empty();
        record.test_taker_watches.insert(watch_id);
        if first {
            record.revision += 1;
            record.publish_participant_changed(now);
        }
        record.test_taker_subscribers.insert(
            watch_id,
            Subscriber {
                data: data_tx.clone(),
                control: control_tx,
                _permit: permit,
            },
        );
        data_tx
            .try_send(TestTakerWatchItem::Snapshot(record.test_taker_snapshot()))
            .map_err(|_| {
                SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
            })?;
        Ok((watch_id, data_rx, control_rx))
    }

    pub async fn watch_examiner(
        &self,
        session_id: &str,
        claims: &AccessClaims,
        after_revision: u64,
        now: SystemTime,
    ) -> std::result::Result<
        (
            Uuid,
            mpsc::Receiver<ExaminerWatchItem>,
            mpsc::Receiver<ExaminerWatchItem>,
        ),
        SessionApplicationError,
    > {
        let record = self
            .store
            .get(session_id)
            .await
            .ok_or_else(|| SessionApplicationError::new(SessionErrorCode::NotFound, 0))?;
        let mut record = record.lock().await;
        authorize(&record, session_id, claims, AccessRole::Examiner, now)
            .map_err(|code| SessionApplicationError::new(code, record.revision))?;
        if after_revision > record.revision {
            return Err(SessionApplicationError::new(
                SessionErrorCode::InvalidCursor,
                record.revision,
            ));
        }
        let permit = self
            .admission
            .try_reserve_watch(record.test_taker_watches.len() + record.examiner_watches.len())
            .ok_or_else(|| {
                self.emit(
                    ServiceEventName::GrpcWatchRejected,
                    Outcome::Rejected,
                    Some(EventErrorCode::ResourceExhausted),
                    Some(session_id),
                );
                SessionApplicationError::new(SessionErrorCode::ResourceExhausted, record.revision)
            })?;
        let watch_id = Uuid::now_v7();
        let (data_tx, data_rx) = mpsc::channel(self.stream_capacity);
        let (control_tx, control_rx) = mpsc::channel(2);
        let first = record.examiner_watches.is_empty();
        record.examiner_watches.insert(watch_id);
        if first {
            record.revision += 1;
            record.publish_participant_changed(now);
        }
        record.examiner_subscribers.insert(
            watch_id,
            Subscriber {
                data: data_tx.clone(),
                control: control_tx,
                _permit: permit,
            },
        );
        let snapshot = record.examiner_snapshot().ok_or_else(|| {
            SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
        })?;
        data_tx
            .try_send(ExaminerWatchItem::Snapshot(snapshot))
            .map_err(|_| {
                SessionApplicationError::new(SessionErrorCode::InvalidState, record.revision)
            })?;
        Ok((watch_id, data_rx, control_rx))
    }

    pub async fn unregister_watch(
        &self,
        session_id: &str,
        role: AccessRole,
        watch_id: Uuid,
        now: SystemTime,
    ) {
        let Some(record) = self.store.get(session_id).await else {
            return;
        };
        let mut record = record.lock().await;
        let terminal = record.expired
            || matches!(
                record.session.status(),
                flipped::SessionStatus::Completed | flipped::SessionStatus::Terminated
            );
        let (removed, became_empty) = match role {
            AccessRole::TestTaker => {
                record.test_taker_subscribers.remove(&watch_id);
                let removed = record.test_taker_watches.remove(&watch_id);
                (removed, removed && record.test_taker_watches.is_empty())
            }
            AccessRole::Examiner => {
                record.examiner_subscribers.remove(&watch_id);
                let removed = record.examiner_watches.remove(&watch_id);
                (removed, removed && record.examiner_watches.is_empty())
            }
        };
        if became_empty && !terminal {
            record.revision += 1;
            record.publish_participant_changed(now);
        }
        drop(record);
        if removed {
            self.emit(
                ServiceEventName::GrpcStreamCancelled,
                Outcome::Cancelled,
                None,
                Some(session_id),
            );
        }
    }

    pub async fn expire_sessions(&self, now: SystemTime) -> usize {
        let mut expired_count = 0;
        let mut purge = Vec::new();
        for record in self.store.all().await {
            let mut record = record.lock().await;
            if record.expire(now) {
                let session_id = record.session.id().to_string();
                self.emit(
                    ServiceEventName::SessionExpired,
                    Outcome::Success,
                    None,
                    Some(&session_id),
                );
                self.emit(
                    ServiceEventName::InvitationRevoked,
                    Outcome::Success,
                    None,
                    Some(&session_id),
                );
                expired_count += 1;
            }
            if record.tombstone_expired(now, self.tombstone_retention) {
                purge.push(record.session.id().to_string());
            }
        }
        for session_id in purge {
            self.store.remove(&session_id).await;
        }
        expired_count
    }

    fn emit(
        &self,
        name: ServiceEventName,
        outcome: Outcome,
        error: Option<EventErrorCode>,
        session_id: Option<&str>,
    ) {
        self.events.emit(
            if matches!(outcome, Outcome::Success) {
                Severity::Info
            } else {
                Severity::Warn
            },
            {
                let mut context = current_event_context();
                context.session_ref = session_id.map(|id| self.session_ref(id));
                context
            },
            ServiceEvent {
                name,
                outcome,
                error_code: error,
                duration_ms: None,
            },
        );
    }

    fn session_ref(&self, session_id: &str) -> String {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.session_ref_key).expect("32-byte HMAC key");
        mac.update(session_id.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }
}

fn cache_command_rejection(
    record: &mut SessionRecord,
    role: AccessRole,
    jti: Uuid,
    command_id: Uuid,
    operation: Operation,
    hash: [u8; 32],
    code: SessionErrorCode,
) -> std::result::Result<ExaminerSnapshot, SessionApplicationError> {
    let error = SessionApplicationError::new(code, record.revision);
    record
        .command_log
        .insert(
            role,
            jti,
            command_id,
            operation,
            hash,
            CachedCommandResult { value: Err(error) },
        )
        .map_err(|insert_code| SessionApplicationError::new(insert_code, record.revision))?;
    Err(error)
}

struct ImportCancellationGuard {
    cancellation: Option<ImportCancellation>,
}

impl ImportCancellationGuard {
    fn new(cancellation: ImportCancellation) -> Self {
        Self {
            cancellation: Some(cancellation),
        }
    }

    fn disarm(&mut self) {
        self.cancellation = None;
    }
}

impl Drop for ImportCancellationGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    use flipped::{ExaminerParticipant, Flashcard};

    use super::*;
    use crate::credentials::{InvitationRecord, InvitationStatus};
    use crate::observability::{RecordingEventListener, ServiceIdentity};

    #[test]
    fn dropping_import_future_guard_signals_blocking_worker_cancellation() {
        let cancellation = ImportCancellation::new();
        {
            let _guard = ImportCancellationGuard::new(cancellation.clone());
        }
        assert!(cancellation.is_cancelled());

        let cancellation = ImportCancellation::new();
        let mut guard = ImportCancellationGuard::new(cancellation.clone());
        guard.disarm();
        drop(guard);
        assert!(!cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn exact_end_retry_returns_cached_success_after_terminal_revocation() {
        let (application, store, examiner_claims, _) = active_application(2).await;
        let session_id = examiner_claims.sub.clone();
        let command_id = Uuid::now_v7().to_string();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);

        let first = application
            .command(
                &session_id,
                &examiner_claims,
                &command_id,
                Operation::End,
                now,
            )
            .await
            .expect("end succeeds");
        let retry = application
            .command(
                &session_id,
                &examiner_claims,
                &command_id,
                Operation::End,
                now + Duration::from_secs(1),
            )
            .await
            .expect("exact retry is served before revoked credential validation");

        assert_eq!(retry, first);
        assert_eq!(retry.status, ApplicationStatus::Terminated);
        let record = store.get(&session_id).await.expect("session");
        let record = record.lock().await;
        assert_eq!(record.revision, first.revision);
        assert!(record.active_jtis.is_empty());
        assert_eq!(record.credential_version, 2);
    }

    #[tokio::test]
    async fn watch_admission_releases_after_unregister() {
        let (application, _, examiner_claims, test_taker_claims) = active_application(1).await;
        let session_id = examiner_claims.sub.clone();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let (test_watch, _test_data, _test_control) = application
            .watch_test_taker(&session_id, &test_taker_claims, 0, now)
            .await
            .expect("first watch");
        let _examiner_watch = application
            .watch_examiner(&session_id, &examiner_claims, 0, now)
            .await
            .expect("second watch");
        let saturated = match application
            .watch_test_taker(&session_id, &test_taker_claims, 0, now)
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("per-session/global limit must be enforced"),
        };
        assert_eq!(saturated.code, SessionErrorCode::ResourceExhausted);

        application
            .unregister_watch(
                &session_id,
                AccessRole::TestTaker,
                test_watch,
                now + Duration::from_secs(1),
            )
            .await;
        application
            .watch_test_taker(&session_id, &test_taker_claims, 0, now)
            .await
            .expect("released permit is reusable");
    }

    #[tokio::test]
    async fn completion_remains_authorized_until_examiner_ends() {
        let (application, store, examiner_claims, test_taker_claims) = active_application(1).await;
        let session_id = examiner_claims.sub.clone();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let (test_watch, mut test_data, mut test_control) = application
            .watch_test_taker(&session_id, &test_taker_claims, 0, now)
            .await
            .expect("test-taker watch");
        let (examiner_watch, mut examiner_data, mut examiner_control) = application
            .watch_examiner(&session_id, &examiner_claims, 0, now)
            .await
            .expect("examiner watch");

        let completed = application
            .command(
                &session_id,
                &examiner_claims,
                &Uuid::now_v7().to_string(),
                Operation::Advance,
                now + Duration::from_secs(1),
            )
            .await
            .expect("final advance completes");
        assert_eq!(completed.status, ApplicationStatus::Completed);
        let completed_revision = completed.revision;
        assert!(stream_contains_test_taker_end(&mut test_data, ApplicationStatus::Completed).await);
        assert!(
            stream_contains_examiner_end(&mut examiner_data, ApplicationStatus::Completed).await
        );
        {
            let record = store.get(&session_id).await.expect("session");
            let record = record.lock().await;
            assert_eq!(record.credential_version, 1);
            assert_eq!(record.active_jtis.len(), 2);
        }

        let ended = application
            .command(
                &session_id,
                &examiner_claims,
                &Uuid::now_v7().to_string(),
                Operation::End,
                now + Duration::from_secs(2),
            )
            .await
            .expect("examiner can explicitly end a completed examination");
        assert_eq!(ended.status, ApplicationStatus::Terminated);
        let terminal_revision = ended.revision;
        assert!(terminal_revision > completed_revision);
        assert!(matches!(
            test_control.recv().await,
            Some(TestTakerWatchItem::Event(crate::events::VersionedEvent {
                revision,
                payload: TestTakerEventPayload::Ended(ApplicationStatus::Terminated),
                ..
            })) if revision == terminal_revision
        ));
        assert!(matches!(
            examiner_control.recv().await,
            Some(ExaminerWatchItem::Event(crate::events::VersionedEvent {
                revision,
                payload: ExaminerEventPayload::Ended(ApplicationStatus::Terminated),
                ..
            })) if revision == terminal_revision
        ));

        application
            .unregister_watch(
                &session_id,
                AccessRole::TestTaker,
                test_watch,
                now + Duration::from_secs(3),
            )
            .await;
        application
            .unregister_watch(
                &session_id,
                AccessRole::Examiner,
                examiner_watch,
                now + Duration::from_secs(3),
            )
            .await;
        let record = store.get(&session_id).await.expect("session");
        let mut record = record.lock().await;
        assert_eq!(record.revision, terminal_revision);
        assert!(record.test_taker_watches.is_empty());
        assert!(record.examiner_watches.is_empty());
        assert!(record.active_jtis.is_empty());
        assert!(!record.expire(now + Duration::from_secs(7_200)));
        assert_eq!(record.revision, terminal_revision);
    }

    async fn stream_contains_test_taker_end(
        stream: &mut mpsc::Receiver<TestTakerWatchItem>,
        status: ApplicationStatus,
    ) -> bool {
        while let Some(item) = stream.recv().await {
            if matches!(
                item,
                TestTakerWatchItem::Event(crate::events::VersionedEvent {
                    payload: TestTakerEventPayload::Ended(found),
                    ..
                }) if found == status
            ) {
                return true;
            }
        }
        false
    }

    async fn stream_contains_examiner_end(
        stream: &mut mpsc::Receiver<ExaminerWatchItem>,
        status: ApplicationStatus,
    ) -> bool {
        while let Some(item) = stream.recv().await {
            if matches!(
                item,
                ExaminerWatchItem::Event(crate::events::VersionedEvent {
                    payload: ExaminerEventPayload::Ended(found),
                    ..
                }) if found == status
            ) {
                return true;
            }
        }
        false
    }

    async fn active_application(
        cards: usize,
    ) -> (Application, InMemoryStore, AccessClaims, AccessClaims) {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let deck = Deck::new(
            None,
            (0..cards)
                .map(|index| {
                    Flashcard::new(format!("front {index}"), format!("back {index}"))
                        .expect("valid card")
                })
                .collect(),
        )
        .expect("non-empty deck");
        let session = Session::new(deck)
            .join_test_taker(test_taker)
            .join_examiner(examiner)
            .start(&examiner)
            .expect("active session");
        let session = AnySession::InProgress(session);
        let session_id = session.id().to_string();
        let examiner_jti = Uuid::now_v7();
        let test_taker_jti = Uuid::now_v7();
        let claims = |role, jti: Uuid| AccessClaims {
            iss: "https://issuer.example".to_owned(),
            aud: "flipped".to_owned(),
            sub: session_id.clone(),
            role,
            token_use: "access".to_owned(),
            credential_version: 1,
            jti: jti.to_string(),
            iat: 10,
            nbf: 10,
            exp: 3_600,
        };
        let examiner_claims = claims(AccessRole::Examiner, examiner_jti);
        let test_taker_claims = claims(AccessRole::TestTaker, test_taker_jti);
        let store = InMemoryStore::default();
        store
            .insert_bounded(
                SessionRecord {
                    session,
                    test_taker,
                    examiner: Some(examiner),
                    revision: 1,
                    expires_at: now + Duration::from_secs(3_600),
                    expired: false,
                    terminal_at: None,
                    credential_version: 1,
                    active_jtis: HashSet::from([examiner_jti, test_taker_jti]),
                    invitation: InvitationRecord {
                        session_id: session_id.clone(),
                        digest: [7; 32],
                        expires_at: now + Duration::from_secs(900),
                        bound_client_id: "client".to_owned(),
                        bound_audience: "flipped".to_owned(),
                        status: InvitationStatus::Consumed,
                        consumed_at: Some(now),
                        redemption_id: None,
                        request_hash: None,
                        cached_token_response: None,
                        cached_until: None,
                    },
                    command_log: crate::command_log::CommandLog::new(cards + 4),
                    test_taker_watches: HashSet::new(),
                    examiner_watches: HashSet::new(),
                    test_taker_subscribers: HashMap::new(),
                    examiner_subscribers: HashMap::new(),
                    lagged_subscribers: 0,
                },
                1,
            )
            .await
            .expect("session inserted");
        let recording = Arc::new(RecordingEventListener::default());
        let application = Application::new(
            store.clone(),
            Arc::new(CredentialService::for_tests()),
            EventDispatcher::new(
                ServiceIdentity {
                    name: "test".to_owned(),
                    version: "1".to_owned(),
                    environment: "test".to_owned(),
                    instance_id: "test".to_owned(),
                },
                vec![recording],
            ),
            ImportLimits::default(),
            "client".to_owned(),
            "flipped".to_owned(),
            Duration::from_secs(3_600),
            Duration::from_secs(60),
            cards + 4,
            8,
            1,
            1,
            2,
            2,
            Duration::from_secs(60),
            [9; 32],
        );
        (application, store, examiner_claims, test_taker_claims)
    }
}

fn authorize(
    record: &SessionRecord,
    session_id: &str,
    claims: &AccessClaims,
    role: AccessRole,
    now: SystemTime,
) -> std::result::Result<(), SessionErrorCode> {
    if claims.sub != session_id {
        return Err(SessionErrorCode::NotFound);
    }
    if claims.role != role {
        return Err(SessionErrorCode::RoleForbidden);
    }
    let now_seconds = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| SessionErrorCode::Unauthenticated)?
        .as_secs();
    if claims.exp <= now_seconds || record.expired || now >= record.expires_at {
        return Err(SessionErrorCode::SessionExpired);
    }
    if claims.credential_version != record.credential_version {
        return Err(SessionErrorCode::CredentialVersionMismatch);
    }
    let jti = claims
        .jti
        .parse()
        .map_err(|_| SessionErrorCode::Unauthenticated)?;
    if !record.active_jtis.contains(&jti) {
        return Err(SessionErrorCode::CredentialVersionMismatch);
    }
    Ok(())
}
