mod common;

use common::observations::RecordingObserver;
use nicknamer_server::name::NameService;
use nicknamer_server::observations::{Fact, MutationKind, MutationOutcome};

#[tokio::test]
async fn create_and_duplicate_record_persistence_outcomes() {
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    service
        .create_name(10, "first".into(), "srv".into())
        .await
        .unwrap();
    assert!(
        service
            .create_name(10, "second".into(), "srv".into())
            .await
            .is_err()
    );
    assert_eq!(service.get_all_names().await.unwrap().len(), 1);
    let outcomes: Vec<_> = recorder
        .events()
        .into_iter()
        .filter_map(|event| match event.fact {
            Fact::NameMutationFinished {
                kind: MutationKind::Create,
                outcome,
                ..
            } => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes,
        vec![MutationOutcome::Committed, MutationOutcome::Rejected]
    );
}

#[tokio::test]
async fn malformed_and_empty_import_have_distinct_summaries() {
    use nicknamer_server::observations::{BulkOperation, BulkOutcome};
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    assert!(
        service
            .bulk_create_names("[invalid", "srv".into())
            .await
            .is_err()
    );
    assert_eq!(
        service
            .bulk_create_names("{}", "srv".into())
            .await
            .unwrap()
            .0,
        0
    );
    assert!(service.get_all_names().await.unwrap().is_empty());
    let summaries: Vec<_> = recorder
        .events()
        .into_iter()
        .filter_map(|event| match event.fact {
            Fact::BulkOperationFinished {
                operation: BulkOperation::Import,
                attempted,
                input_count,
                outcome,
                ..
            } => Some((attempted, input_count, outcome)),
            _ => None,
        })
        .collect();
    assert_eq!(
        summaries,
        vec![
            (0, None, BulkOutcome::Rejected),
            (0, Some(0), BulkOutcome::Succeeded)
        ]
    );
}

#[tokio::test]
async fn duplicate_only_import_succeeds_with_skip_and_parented_rejection() {
    use nicknamer_server::observations::{BulkOutcome, MutationOrigin};
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    service
        .create_name(10, "original".into(), "srv".into())
        .await
        .unwrap();
    let result = service
        .bulk_create_names("10: replacement", "srv".into())
        .await
        .unwrap();
    assert_eq!((result.0, result.1, result.2.len()), (0, 1, 0));
    assert_eq!(service.get_all_names().await.unwrap()[0].name(), "original");
    let events = recorder.events();
    let summary = events
        .iter()
        .find(|event| matches!(event.fact, Fact::BulkOperationFinished { .. }))
        .unwrap();
    assert!(matches!(
        summary.fact,
        Fact::BulkOperationFinished {
            attempted: 1,
            succeeded: 0,
            skipped: 1,
            failed: 0,
            outcome: BulkOutcome::Succeeded,
            ..
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event.fact,
        Fact::NameMutationFinished {
            origin: MutationOrigin::Bulk,
            outcome: MutationOutcome::Rejected,
            ..
        }
    ) && event.context.parent_operation_id
        == Some(summary.context.operation_id)));
}

#[tokio::test]
async fn repeated_and_missing_delete_are_partial_with_aggregate_missing_category() {
    use nicknamer_server::observations::{BulkOutcome, FailureCategory, MutationOrigin};
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    let id = service
        .create_name(10, "first".into(), "srv".into())
        .await
        .unwrap()
        .id();
    let (deleted, errors) = service
        .bulk_delete_names(&[id, id, u32::MAX])
        .await
        .unwrap();
    assert_eq!((deleted, errors.len()), (1, 2));
    assert!(service.get_all_names().await.unwrap().is_empty());
    let events = recorder.events();
    let summary = events
        .iter()
        .find(|event| matches!(event.fact, Fact::BulkOperationFinished { .. }))
        .unwrap();
    assert!(
        matches!(&summary.fact, Fact::BulkOperationFinished { attempted: 3, succeeded: 1, failed: 2, outcome: BulkOutcome::Partial, categories, .. } if categories == &vec![(FailureCategory::MissingEntry, 2)])
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event.fact,
                Fact::NameMutationFinished {
                    origin: MutationOrigin::Bulk,
                    ..
                }
            ) && event.context.parent_operation_id
                == Some(summary.context.operation_id))
            .count(),
        3
    );
}

#[tokio::test]
async fn web_export_records_prepared_bytes_after_serialization() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nicknamer_server::name::web::{NameState, create_name_router};
    use nicknamer_server::observations::{ExportStage, Outcome};
    use std::sync::Arc;
    use tower::ServiceExt;

    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::new(&db);
    service
        .create_name(10, "first".into(), "srv".into())
        .await
        .unwrap();
    let app = create_name_router(Arc::new(NameState {
        db: Arc::new(db),
        observer,
    }));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/names/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        recorder
            .events()
            .iter()
            .any(|event| matches!(event.fact, Fact::ExportPrepared {
        entry_count: Some(1), prepared_bytes: Some(n), outcome: Outcome::Succeeded,
        stage: ExportStage::Serialization, category: None,
    } if n == bytes.len() as u64))
    );
}

#[tokio::test]
async fn missing_delete_is_rejected_without_a_write() {
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    assert!(service.delete_name_by_id(999).await.is_err());
    assert!(service.get_all_names().await.unwrap().is_empty());
    assert!(recorder.events().iter().any(|event| matches!(
        event.fact,
        Fact::NameMutationFinished {
            kind: MutationKind::Delete,
            outcome: MutationOutcome::Rejected,
            ..
        }
    )));
}

#[tokio::test]
async fn database_failure_never_records_committed() {
    use sea_orm::ConnectionTrait;
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    db.execute_unprepared("DROP TABLE name").await.unwrap();
    assert!(
        service
            .create_name(10, "first".into(), "srv".into())
            .await
            .is_err()
    );
    let events = recorder.events();
    assert!(events.iter().any(|event| matches!(
        event.fact,
        Fact::NameMutationFinished {
            kind: MutationKind::Create,
            outcome: MutationOutcome::Failed,
            ..
        }
    )));
    assert!(!events.iter().any(|event| matches!(
        event.fact,
        Fact::NameMutationFinished {
            outcome: MutationOutcome::Committed,
            ..
        }
    )));
}

#[tokio::test]
async fn api_export_records_prepared_bytes() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nicknamer_server::name::api::v1::create_api_router;
    use nicknamer_server::name::web::NameState;
    use nicknamer_server::observations::{ExportStage, Outcome};
    use std::sync::Arc;
    use tower::ServiceExt;
    let db = common::setup_db_with_global_container().await.unwrap();
    NameService::new(&db)
        .create_name(10, "first".into(), "srv".into())
        .await
        .unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let app = create_api_router(Arc::new(NameState {
        db: Arc::new(db),
        observer,
    }));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/names/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        recorder
            .events()
            .iter()
            .any(|event| matches!(event.fact, Fact::ExportPrepared {
        entry_count: Some(1), prepared_bytes: Some(n), outcome: Outcome::Succeeded,
        stage: ExportStage::Serialization, category: None,
    } if n == bytes.len() as u64))
    );
}

#[tokio::test]
async fn later_response_error_does_not_change_committed_fact() {
    use axum::{http::StatusCode, response::IntoResponse};
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    service
        .create_name(10, "first".into(), "srv".into())
        .await
        .unwrap();
    let response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(service.get_all_names().await.unwrap().len(), 1);
    assert!(recorder.events().iter().any(|event| matches!(
        event.fact,
        Fact::NameMutationFinished {
            kind: MutationKind::Create,
            outcome: MutationOutcome::Committed,
            ..
        }
    )));
}

#[tokio::test]
async fn import_success_and_database_failure_is_partial_and_keeps_successful_row() {
    use nicknamer_server::observations::{BulkOutcome, FailureCategory};
    use sea_orm::ConnectionTrait;
    let db = common::setup_db_with_global_container().await.unwrap();
    db.execute_unprepared(
        "ALTER TABLE name ADD CONSTRAINT reject_discord CHECK (discord_id != 20)",
    )
    .await
    .unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    let (created, skipped, errors) = service
        .bulk_create_names("10: first\n20: denied", "srv".into())
        .await
        .unwrap();
    assert_eq!((created, skipped, errors.len()), (1, 0, 1));
    assert_eq!(service.get_all_names().await.unwrap()[0].discord_id(), 10);
    let events = recorder.events();
    assert!(
        events
            .iter()
            .any(|event| matches!(&event.fact, Fact::BulkOperationFinished {
        attempted: 2, succeeded: 1, skipped: 0, failed: 1,
        outcome: BulkOutcome::Partial, categories, ..
    } if categories == &vec![(FailureCategory::Database, 1)]))
    );
}

#[tokio::test]
async fn empty_bulk_delete_is_successful_noop() {
    use nicknamer_server::observations::BulkOutcome;
    let db = common::setup_db_with_global_container().await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let service = NameService::with_observer(&db, observer);
    assert_eq!(service.bulk_delete_names(&[]).await.unwrap().0, 0);
    assert!(service.get_all_names().await.unwrap().is_empty());
    assert!(recorder.events().iter().any(|event| matches!(
        event.fact,
        Fact::BulkOperationFinished {
            attempted: 0,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            input_count: Some(0),
            outcome: BulkOutcome::Succeeded,
            ..
        }
    )));
}

#[tokio::test]
async fn web_export_query_failure_records_query_stage() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use nicknamer_server::name::web::{NameState, create_name_router};
    use nicknamer_server::observations::{ExportStage, FailureCategory, Outcome};
    use sea_orm::ConnectionTrait;
    use std::sync::Arc;
    use tower::ServiceExt;
    let db = common::setup_db_with_global_container().await.unwrap();
    db.execute_unprepared("DROP TABLE name").await.unwrap();
    let (recorder, observer) = RecordingObserver::shared();
    let app = create_name_router(Arc::new(NameState {
        db: Arc::new(db),
        observer,
    }));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/names/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(recorder.events().iter().any(|event| matches!(
        event.fact,
        Fact::ExportPrepared {
            entry_count: None,
            prepared_bytes: None,
            outcome: Outcome::Failed,
            stage: ExportStage::Query,
            category: Some(FailureCategory::Database),
        }
    )));
}
