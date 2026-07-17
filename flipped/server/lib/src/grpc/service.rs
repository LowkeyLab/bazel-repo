use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_stream::try_stream;
use examination_proto::flipped::examination::v1 as pb;
use futures::Stream;
use pb::examination_service_server::ExaminationService;
use tokio::io::AsyncWriteExt;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::application::Application;
use crate::command_log::Operation;
use crate::credentials::{AccessClaims, AccessRole};
use crate::error::{ServerError, SessionApplicationError, SessionErrorCode, StreamErrorCode};
use crate::events::ApplicationStatus;
use crate::observability::{TraceContext, scope_trace};

use super::auth::{bearer, reject_authorization};
use super::mapping;

#[derive(Clone)]
pub struct ExaminationGrpcService {
    application: Arc<Application>,
}

impl ExaminationGrpcService {
    pub fn new(application: Arc<Application>) -> Self {
        Self { application }
    }

    fn authenticate<T>(&self, request: &Request<T>) -> Result<AccessClaims, Status> {
        let token = bearer(request.metadata())?;
        self.application
            .authenticate(token, SystemTime::now())
            .map_err(|_| Status::unauthenticated("invalid bearer credential"))
    }
}

#[tonic::async_trait]
impl ExaminationService for ExaminationGrpcService {
    async fn create_session(
        &self,
        request: Request<tonic::Streaming<pb::UploadChunk>>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        reject_authorization(request.metadata())?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let mut stream = request.into_inner();
        let first = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("upload metadata is required"))?;
        let metadata = match first.chunk {
            Some(pb::upload_chunk::Chunk::Metadata(metadata)) => metadata,
            _ => return Err(Status::invalid_argument("upload metadata must be first")),
        };
        if metadata.declared_size_bytes > self.application.max_upload_bytes() {
            return Ok(Response::new(pb::CreateSessionResponse {
                result: Some(pb::create_session_response::Result::Error(
                    mapping::import_error(flipped_anki::ImportErrorCode::UploadTooLarge),
                )),
            }));
        }
        let import_permit = self
            .application
            .reserve_import()
            .map_err(|_| Status::resource_exhausted("upload/import capacity exhausted"))?;
        let upload = tempfile::NamedTempFile::new()
            .map_err(|_| Status::internal("temporary upload creation failed"))?;
        let upload_file = upload
            .reopen()
            .map_err(|_| Status::internal("temporary upload open failed"))?;
        let mut upload_file = tokio::fs::File::from_std(upload_file);
        let mut received = 0_u64;
        while let Some(chunk) = stream.message().await? {
            match chunk.chunk {
                Some(pb::upload_chunk::Chunk::Data(data)) => {
                    received = received.saturating_add(data.len() as u64);
                    if received > self.application.max_upload_bytes() {
                        return Ok(Response::new(pb::CreateSessionResponse {
                            result: Some(pb::create_session_response::Result::Error(
                                mapping::import_error(
                                    flipped_anki::ImportErrorCode::UploadTooLarge,
                                ),
                            )),
                        }));
                    }
                    upload_file
                        .write_all(&data)
                        .await
                        .map_err(|_| Status::internal("temporary upload write failed"))?;
                }
                _ => return Err(Status::invalid_argument("upload metadata must occur once")),
            }
        }
        if received != metadata.declared_size_bytes {
            return Err(Status::invalid_argument(
                "declared upload size does not match body",
            ));
        }
        upload_file
            .flush()
            .await
            .map_err(|_| Status::internal("temporary upload flush failed"))?;
        drop(upload_file);
        let result = scope_trace(
            trace_context,
            self.application.create_session_from_file_reserved(
                upload.path().to_path_buf(),
                metadata.package_extension,
                SystemTime::now(),
                import_permit,
            ),
        )
        .await;
        let response = match result {
            Ok(created) => pb::CreateSessionResponse {
                result: Some(pb::create_session_response::Result::Success(
                    pb::CreateSessionSuccess {
                        session_id: created.session_id,
                        test_taker_access_token: created.test_taker_access_token,
                        examiner_invitation: created.examiner_invitation,
                        expires_at: mapping::timestamp(created.expires_at),
                        card_count: created.card_count,
                        revision: created.revision,
                        initial_snapshot: Some(created.initial_snapshot.into()),
                    },
                )),
            },
            Err(ServerError::Import(code)) => pb::CreateSessionResponse {
                result: Some(pb::create_session_response::Result::Error(
                    mapping::import_error(code),
                )),
            },
            Err(ServerError::ResourceExhausted) => {
                return Err(Status::resource_exhausted("session capacity exhausted"));
            }
            Err(_) => return Err(Status::internal("session creation failed")),
        };
        Ok(Response::new(response))
    }

    async fn get_test_taker_session_snapshot(
        &self,
        request: Request<pb::SessionSnapshotRequest>,
    ) -> Result<Response<pb::TestTakerSnapshotResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let result = scope_trace(
            trace_context,
            self.application
                .test_taker_snapshot(&request.session_id, &claims, SystemTime::now()),
        )
        .await;
        Ok(Response::new(match result {
            Ok(snapshot) => pb::TestTakerSnapshotResponse {
                result: Some(pb::test_taker_snapshot_response::Result::Success(
                    pb::TestTakerSnapshotSuccess {
                        snapshot: Some(snapshot.into()),
                    },
                )),
            },
            Err(error) => pb::TestTakerSnapshotResponse {
                result: Some(pb::test_taker_snapshot_response::Result::Error(
                    mapping::session_error(error.code, error.current_revision),
                )),
            },
        }))
    }

    async fn get_examiner_session_snapshot(
        &self,
        request: Request<pb::SessionSnapshotRequest>,
    ) -> Result<Response<pb::ExaminerSnapshotResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let result = scope_trace(
            trace_context,
            self.application
                .examiner_snapshot(&request.session_id, &claims, SystemTime::now()),
        )
        .await;
        Ok(Response::new(match result {
            Ok(snapshot) => pb::ExaminerSnapshotResponse {
                result: Some(pb::examiner_snapshot_response::Result::Success(
                    pb::ExaminerSnapshotSuccess {
                        snapshot: Some(snapshot.into()),
                    },
                )),
            },
            Err(error) => pb::ExaminerSnapshotResponse {
                result: Some(pb::examiner_snapshot_response::Result::Error(
                    mapping::session_error(error.code, error.current_revision),
                )),
            },
        }))
    }

    async fn start_session(
        &self,
        request: Request<pb::SessionCommandRequest>,
    ) -> Result<Response<pb::StartSessionResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let result = scope_trace(
            trace_context,
            command(&self.application, &request, &claims, Operation::Start),
        )
        .await;
        Ok(Response::new(match result {
            Ok(snapshot) => pb::StartSessionResponse {
                result: Some(pb::start_session_response::Result::Success(
                    pb::StartSessionSuccess {
                        snapshot: Some(snapshot.into()),
                    },
                )),
            },
            Err(error) => pb::StartSessionResponse {
                result: Some(pb::start_session_response::Result::Error(
                    mapping::session_error(error.code, error.current_revision),
                )),
            },
        }))
    }

    async fn advance_session(
        &self,
        request: Request<pb::SessionCommandRequest>,
    ) -> Result<Response<pb::AdvanceSessionResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let result = scope_trace(
            trace_context,
            command(&self.application, &request, &claims, Operation::Advance),
        )
        .await;
        Ok(Response::new(match result {
            Ok(snapshot) => pb::AdvanceSessionResponse {
                result: Some(pb::advance_session_response::Result::Success(
                    pb::AdvanceSessionSuccess {
                        snapshot: Some(snapshot.into()),
                    },
                )),
            },
            Err(error) => pb::AdvanceSessionResponse {
                result: Some(pb::advance_session_response::Result::Error(
                    mapping::session_error(error.code, error.current_revision),
                )),
            },
        }))
    }

    async fn end_session(
        &self,
        request: Request<pb::SessionCommandRequest>,
    ) -> Result<Response<pb::EndSessionResponse>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let result = scope_trace(
            trace_context,
            command(&self.application, &request, &claims, Operation::End),
        )
        .await;
        Ok(Response::new(match result {
            Ok(snapshot) => pb::EndSessionResponse {
                result: Some(pb::end_session_response::Result::Success(
                    pb::EndSessionSuccess {
                        snapshot: Some(snapshot.into()),
                    },
                )),
            },
            Err(error) => pb::EndSessionResponse {
                result: Some(pb::end_session_response::Result::Error(
                    mapping::session_error(error.code, error.current_revision),
                )),
            },
        }))
    }

    type WatchTestTakerSessionStream =
        Pin<Box<dyn Stream<Item = Result<pb::TestTakerWatchResponse, Status>> + Send + 'static>>;

    async fn watch_test_taker_session(
        &self,
        request: Request<pb::WatchSessionRequest>,
    ) -> Result<Response<Self::WatchTestTakerSessionStream>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let opened = scope_trace(
            trace_context.clone(),
            self.application.watch_test_taker(
                &request.session_id,
                &claims,
                request.after_revision,
                SystemTime::now(),
            ),
        )
        .await;
        let (watch_id, mut data, mut control) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                if let Some(item) = test_taker_stream_error(error) {
                    let stream = try_stream! { yield mapping::test_taker_watch(item); };
                    return Ok(Response::new(Box::pin(stream)));
                }
                return Err(stream_open_status(error));
            }
        };
        let guard = WatchGuard::new(
            Arc::clone(&self.application),
            request.session_id,
            AccessRole::TestTaker,
            watch_id,
            trace_context,
        );
        let stream = try_stream! {
            let _guard = guard;
            let mut data_open = true;
            let mut control_open = true;
            while let Some(item) = next_watch_item(
                &mut data,
                &mut control,
                &mut data_open,
                &mut control_open,
            )
            .await
            {
                let terminal = test_taker_item_is_terminal(&item);
                yield mapping::test_taker_watch(item);
                if terminal {
                    break;
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }

    type WatchExaminerSessionStream =
        Pin<Box<dyn Stream<Item = Result<pb::ExaminerWatchResponse, Status>> + Send + 'static>>;

    async fn watch_examiner_session(
        &self,
        request: Request<pb::WatchSessionRequest>,
    ) -> Result<Response<Self::WatchExaminerSessionStream>, Status> {
        let _request_event = self
            .application
            .request_event(crate::observability::ServiceEventName::GrpcRequestCompleted);
        let claims = self.authenticate(&request)?;
        let trace_context = TraceContext::extract_metadata(request.metadata());
        let request = request.into_inner();
        let opened = scope_trace(
            trace_context.clone(),
            self.application.watch_examiner(
                &request.session_id,
                &claims,
                request.after_revision,
                SystemTime::now(),
            ),
        )
        .await;
        let (watch_id, mut data, mut control) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                if let Some(item) = examiner_stream_error(error) {
                    let stream = try_stream! { yield mapping::examiner_watch(item); };
                    return Ok(Response::new(Box::pin(stream)));
                }
                return Err(stream_open_status(error));
            }
        };
        let guard = WatchGuard::new(
            Arc::clone(&self.application),
            request.session_id,
            AccessRole::Examiner,
            watch_id,
            trace_context,
        );
        let stream = try_stream! {
            let _guard = guard;
            let mut data_open = true;
            let mut control_open = true;
            while let Some(item) = next_watch_item(
                &mut data,
                &mut control,
                &mut data_open,
                &mut control_open,
            )
            .await
            {
                let terminal = examiner_item_is_terminal(&item);
                yield mapping::examiner_watch(item);
                if terminal {
                    break;
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }
}

async fn next_watch_item<T>(
    data: &mut tokio::sync::mpsc::Receiver<T>,
    control: &mut tokio::sync::mpsc::Receiver<T>,
    data_open: &mut bool,
    control_open: &mut bool,
) -> Option<T> {
    while *data_open || *control_open {
        tokio::select! {
            biased;
            item = control.recv(), if *control_open => match item {
                Some(item) => return Some(item),
                None => *control_open = false,
            },
            item = data.recv(), if *data_open => match item {
                Some(item) => return Some(item),
                None => *data_open = false,
            },
        }
    }
    None
}

fn test_taker_item_is_terminal(item: &crate::events::TestTakerWatchItem) -> bool {
    matches!(
        item,
        crate::events::TestTakerWatchItem::Error(..)
            | crate::events::TestTakerWatchItem::Event(crate::events::VersionedEvent {
                payload: crate::events::TestTakerEventPayload::Ended(
                    ApplicationStatus::Terminated | ApplicationStatus::Expired
                ),
                ..
            })
    )
}

fn examiner_item_is_terminal(item: &crate::events::ExaminerWatchItem) -> bool {
    matches!(
        item,
        crate::events::ExaminerWatchItem::Error(..)
            | crate::events::ExaminerWatchItem::Event(crate::events::VersionedEvent {
                payload: crate::events::ExaminerEventPayload::Ended(
                    ApplicationStatus::Terminated | ApplicationStatus::Expired
                ),
                ..
            })
    )
}

async fn command(
    application: &Application,
    request: &pb::SessionCommandRequest,
    claims: &AccessClaims,
    operation: Operation,
) -> Result<crate::events::ExaminerSnapshot, SessionApplicationError> {
    application
        .command(
            &request.session_id,
            claims,
            &request.command_id,
            operation,
            SystemTime::now(),
        )
        .await
}

fn test_taker_stream_error(
    error: SessionApplicationError,
) -> Option<crate::events::TestTakerWatchItem> {
    typed_stream_error(error)
        .map(|code| crate::events::TestTakerWatchItem::Error(code, error.current_revision))
}

fn examiner_stream_error(
    error: SessionApplicationError,
) -> Option<crate::events::ExaminerWatchItem> {
    typed_stream_error(error)
        .map(|code| crate::events::ExaminerWatchItem::Error(code, error.current_revision))
}

fn typed_stream_error(error: SessionApplicationError) -> Option<StreamErrorCode> {
    match error.code {
        SessionErrorCode::InvalidCursor => Some(StreamErrorCode::InvalidCursor),
        SessionErrorCode::SessionExpired => Some(StreamErrorCode::SessionExpired),
        SessionErrorCode::CredentialVersionMismatch => {
            Some(StreamErrorCode::CredentialVersionMismatch)
        }
        _ => None,
    }
}

fn stream_open_status(error: SessionApplicationError) -> Status {
    match error.code {
        SessionErrorCode::Unauthenticated => Status::unauthenticated("invalid credential"),
        SessionErrorCode::NotFound => Status::not_found("session not found"),
        SessionErrorCode::RoleForbidden => Status::permission_denied("role forbidden"),
        SessionErrorCode::ResourceExhausted => {
            Status::resource_exhausted("watch capacity exhausted")
        }
        _ => Status::failed_precondition("stream rejected"),
    }
}

struct WatchGuard {
    application: Arc<Application>,
    session_id: String,
    role: AccessRole,
    watch_id: Uuid,
    trace_context: TraceContext,
}

impl WatchGuard {
    fn new(
        application: Arc<Application>,
        session_id: String,
        role: AccessRole,
        watch_id: Uuid,
        trace_context: TraceContext,
    ) -> Self {
        Self {
            application,
            session_id,
            role,
            watch_id,
            trace_context,
        }
    }
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        let application = Arc::clone(&self.application);
        let session_id = self.session_id.clone();
        let role = self.role;
        let watch_id = self.watch_id;
        let trace_context = self.trace_context.clone();
        tokio::spawn(async move {
            scope_trace(
                trace_context,
                application.unregister_watch(&session_id, role, watch_id, SystemTime::now()),
            )
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::events::{
        ApplicationStatus, ExaminerEventPayload, ExaminerWatchItem, TestTakerEventPayload,
        TestTakerWatchItem, VersionedEvent,
    };

    use super::*;

    #[test]
    fn watch_capacity_rejection_maps_to_resource_exhausted() {
        let status = stream_open_status(SessionApplicationError::new(
            SessionErrorCode::ResourceExhausted,
            7,
        ));
        assert_eq!(status.code(), tonic::Code::ResourceExhausted);
    }

    #[test]
    fn only_revoked_end_states_are_terminal() {
        let item = |status| {
            ExaminerWatchItem::Event(VersionedEvent {
                revision: 8,
                occurred_at: SystemTime::UNIX_EPOCH,
                payload: ExaminerEventPayload::Ended(status),
            })
        };
        assert!(!examiner_item_is_terminal(&item(
            ApplicationStatus::Completed
        )));
        assert!(examiner_item_is_terminal(&item(
            ApplicationStatus::Terminated
        )));
        assert!(examiner_item_is_terminal(&item(ApplicationStatus::Expired)));
    }

    #[tokio::test]
    async fn closed_control_channel_does_not_starve_terminal_data_event() {
        for status in [ApplicationStatus::Terminated, ApplicationStatus::Expired] {
            let (data_tx, mut data_rx) = tokio::sync::mpsc::channel(1);
            let (control_tx, mut control_rx) = tokio::sync::mpsc::channel(1);
            data_tx
                .send(TestTakerWatchItem::Event(VersionedEvent {
                    revision: 7,
                    occurred_at: SystemTime::UNIX_EPOCH,
                    payload: TestTakerEventPayload::Ended(status),
                }))
                .await
                .expect("terminal event is queued");
            drop(data_tx);
            drop(control_tx);

            let task = tokio::spawn(async move {
                let mut data_open = true;
                let mut control_open = true;
                let item = next_watch_item(
                    &mut data_rx,
                    &mut control_rx,
                    &mut data_open,
                    &mut control_open,
                )
                .await
                .expect("queued data survives control closure");
                assert!(test_taker_item_is_terminal(&item));
            });
            tokio::time::timeout(Duration::from_millis(100), task)
                .await
                .expect("stream task exits instead of hot-looping")
                .expect("stream task succeeds");
        }
    }
}
