use googletest::{assert_that, matchers::eq};

use crate::{domain::CatalogId, store::Store};

#[googletest::test]
fn initialized_catalog_reopens_with_its_history_and_revision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let id = CatalogId::new();
    let initialized = Store::create_with_id(&path, id).unwrap();
    let reopened = Store::open(&path, id).unwrap();
    assert_that!(reopened.projection().unwrap(), eq(&initialized));
    assert_that!(reopened.events().unwrap().len(), eq(1));
    assert_that!(initialized.revision, eq(1));
}

#[googletest::test]
fn losing_commit_result_requires_inspection_and_cannot_repeat_initialization() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let id = CatalogId::new();
    assert_that!(
        matches!(
            Store::create_with_lost_commit_result(&path, id),
            Err(crate::error::PoolError::CommitUnknown)
        ),
        eq(true)
    );
    {
        let reopened = Store::open(&path, id).unwrap();
        assert_that!(reopened.projection().unwrap().revision, eq(1));
        assert_that!(reopened.events().unwrap().len(), eq(1));
    }
    assert_that!(Store::create_with_id(&path, id).is_err(), eq(true));
    assert_that!(
        Store::open(&path, id).unwrap().events().unwrap().len(),
        eq(1)
    );
}

#[derive(Debug)]
struct SyncFailureBackend {
    inner: redb::backends::FileBackend,
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl redb::StorageBackend for SyncFailureBackend {
    fn len(&self) -> std::io::Result<u64> {
        redb::StorageBackend::len(&self.inner)
    }
    fn read(&self, offset: u64, out: &mut [u8]) -> std::io::Result<()> {
        redb::StorageBackend::read(&self.inner, offset, out)
    }
    fn set_len(&self, len: u64) -> std::io::Result<()> {
        redb::StorageBackend::set_len(&self.inner, len)
    }
    fn sync_data(&self) -> std::io::Result<()> {
        if self.armed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(std::io::Error::other("injected sync failure"));
        }
        redb::StorageBackend::sync_data(&self.inner)
    }
    fn write(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        redb::StorageBackend::write(&self.inner, offset, data)
    }
}

#[googletest::test]
fn real_redb_commit_io_failure_is_unknown_and_never_reinitializes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let backend = SyncFailureBackend {
        inner: redb::backends::FileBackend::new(file).unwrap(),
        armed: armed.clone(),
    };
    let id = CatalogId::new();
    let result = Store::create_with_fault_backend(id, backend, || {
        armed.store(true, std::sync::atomic::Ordering::SeqCst)
    });
    assert_that!(
        matches!(result, Err(crate::error::PoolError::CommitUnknown)),
        eq(true)
    );
    assert_that!(Store::create_with_id(&path, id).is_err(), eq(true));
    // A reopened database may contain either the complete commit or no supported catalog.
    // Both outcomes preserve the authority barrier; partial catalog state is never accepted.
    if let Ok(reopened) = Store::open(&path, id) {
        assert_that!(reopened.projection().unwrap().revision, eq(1));
        assert_that!(reopened.events().unwrap().len(), eq(1));
    }
}

#[googletest::test]
fn every_nonzero_expected_initial_revision_is_rejected() {
    use crate::domain::{CatalogInitialized, DomainEvent, reduce};
    use proptest::test_runner::{TestCaseError, TestRunner};
    let event = DomainEvent::CatalogInitialized(CatalogInitialized {
        catalog_id: CatalogId::new(),
        store_version: 1,
        projection_version: 1,
    });
    TestRunner::default()
        .run(&(1_u64..=u64::MAX), |revision| {
            googletest::verify_that!(
                matches!(
                    reduce(None, revision, &event),
                    Err(crate::error::PoolError::Conflict)
                ),
                eq(true)
            )
            .map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .unwrap();
    let initial = reduce(None, 0, &event).unwrap();
    assert_that!(
        matches!(
            reduce(Some(&initial), 1, &event),
            Err(crate::error::PoolError::Conflict)
        ),
        eq(true)
    );
}

#[googletest::test]
fn authoritative_profile_rejects_wrong_identity_versions_and_extension_types() {
    use crate::domain::{decode_event, initialized_event};
    let id = CatalogId::new();
    let valid = initialized_event(id).unwrap();
    for (field, bad_value) in [
        (
            "source",
            serde_json::json!("urn:uuid:00000000-0000-0000-0000-000000000000"),
        ),
        ("id", serde_json::json!("not-an-id")),
        ("subject", serde_json::json!("another/catalog")),
        ("time", serde_json::json!("2026-10-08T12:00:00+01:00")),
        ("datacontenttype", serde_json::json!("text/plain")),
        ("operationid", serde_json::json!(42)),
        ("causationid", serde_json::json!(false)),
        ("OperationId", serde_json::json!("bad-extension-name")),
        (
            "type",
            serde_json::json!("io.lowkeylab.worktreepool.catalog.initialized.v2"),
        ),
        ("specversion", serde_json::json!("0.3")),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = bad_value;
        assert_that!(decode_event(&invalid, id).is_err(), eq(true));
    }
    assert_that!(decode_event(&valid, CatalogId::new()).is_err(), eq(true));
}

#[googletest::test]
fn corrupt_history_never_becomes_an_empty_replacement() {
    use redb::{Database, ReadableTable, TableDefinition};
    let table_definition: TableDefinition<u64, &str> = TableDefinition::new("events");
    for corruption in [
        "position",
        "expected_revision",
        "event_type",
        "event_version",
        "duplicate",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.redb");
        let id = CatalogId::new();
        Store::create_with_id(&path, id).unwrap();
        {
            // Prepare a damaged authoritative fixture; observations remain at Store's public boundary.
            let database = Database::open(&path).unwrap();
            let transaction = database.begin_write().unwrap();
            {
                let mut table = transaction.open_table(table_definition).unwrap();
                let mut record: serde_json::Value =
                    serde_json::from_str(table.get(1).unwrap().unwrap().value()).unwrap();
                match corruption {
                    "position" => record["position"] = serde_json::json!(2),
                    "expected_revision" => record["expected_revision"] = serde_json::json!(99),
                    "event_type" => {
                        record["event"]["type"] =
                            serde_json::json!("io.lowkeylab.worktreepool.unknown.v1")
                    }
                    "event_version" => {
                        record["event"]["data"]["store_version"] = serde_json::json!(2)
                    }
                    "duplicate" => {
                        record["position"] = serde_json::json!(2);
                        record["expected_revision"] = serde_json::json!(1);
                    }
                    _ => unreachable!(),
                }
                table
                    .insert(
                        if corruption == "duplicate" { 2 } else { 1 },
                        serde_json::to_string(&record).unwrap().as_str(),
                    )
                    .unwrap();
            }
            transaction.commit().unwrap();
        }
        assert_that!(Store::open(&path, id).is_err(), eq(true));
        let before = std::fs::read(&path).unwrap();
        assert_that!(
            Store::create_with_id(&path, CatalogId::new()).is_err(),
            eq(true)
        );
        assert_that!(std::fs::read(&path).unwrap(), eq(&before));
        assert_that!(Store::open(&path, id).is_err(), eq(true));
    }
}

#[googletest::test]
fn resource_append_sync_failure_reopens_as_complete_old_or_new_history() {
    use crate::{
        domain::management_event,
        management::{ManagementEvent, Repository, RepositoryId},
        paths::EncodedPath,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let catalog_id = CatalogId::new();
    Store::create_with_id(&path, catalog_id).unwrap();
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let backend = SyncFailureBackend {
        inner: redb::backends::FileBackend::new(file).unwrap(),
        armed: armed.clone(),
    };
    let repository_id = RepositoryId::new();
    let event = management_event(
        catalog_id,
        &ManagementEvent::RepositoryRegistered(Repository {
            repository_id,
            common_directory: EncodedPath::from_path(directory.path()),
            context_path: EncodedPath::from_path(directory.path()),
            capacity: 4,
            revision: 0,
        }),
    )
    .unwrap();
    let store = Store::open_with_fault_backend(catalog_id, backend).unwrap();
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_that!(
        matches!(
            store.append(1, event),
            Err(crate::error::PoolError::CommitUnknown)
        ),
        eq(true)
    );
    drop(store);
    let reopened = Store::open(&path, catalog_id).unwrap();
    let state = reopened.projection().unwrap();
    assert_that!(matches!(state.revision, 1 | 2), eq(true));
    assert_that!(reopened.events().unwrap().len() as u64, eq(state.revision));
    assert_that!(state.repositories.len() as u64, eq(state.revision - 1));
    if state.revision == 2 {
        assert_that!(state.repositories[0].repository_id, eq(repository_id));
    }
}

#[googletest::test]
fn refresh_results_require_the_recorded_intent_cause_and_resource_subject() {
    use crate::{
        domain::{management_event, management_event_caused_by},
        management::{
            ManagementEvent, OperationId, RefreshFinished, RefreshStarted, Repository, RepositoryId,
        },
        paths::EncodedPath,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let catalog_id = CatalogId::new();
    Store::create_with_id(&path, catalog_id).unwrap();
    let store = Store::open(&path, catalog_id).unwrap();
    let repository_id = RepositoryId::new();
    store
        .append(
            1,
            management_event(
                catalog_id,
                &ManagementEvent::RepositoryRegistered(Repository {
                    repository_id,
                    common_directory: EncodedPath::from_path(directory.path()),
                    context_path: EncodedPath::from_path(directory.path()),
                    capacity: 4,
                    revision: 0,
                }),
            )
            .unwrap(),
        )
        .unwrap();
    let operation_id = OperationId::new();
    let started = management_event(
        catalog_id,
        &ManagementEvent::RefreshStarted(RefreshStarted {
            operation_id,
            repository_id,
        }),
    )
    .unwrap();
    let cause = started["id"].as_str().unwrap().to_owned();
    store.append(0, started).unwrap();
    let finished = ManagementEvent::RefreshFinished(RefreshFinished {
        operation_id,
        repository_id,
        resolved_commit: Some("0123456789012345678901234567890123456789".into()),
    });
    let wrong_cause = CatalogId::new().to_string();
    assert_that!(
        store
            .append(
                1,
                management_event_caused_by(catalog_id, &finished, &wrong_cause).unwrap()
            )
            .is_err(),
        eq(true)
    );
    let mut wrong_subject = management_event_caused_by(catalog_id, &finished, &cause).unwrap();
    wrong_subject["subject"] = serde_json::json!(format!("catalogs/{catalog_id}"));
    assert_that!(store.append(1, wrong_subject).is_err(), eq(true));
    assert_that!(store.projection().unwrap().revision, eq(3));
    let complete = store
        .append(
            1,
            management_event_caused_by(catalog_id, &finished, &cause).unwrap(),
        )
        .unwrap();
    assert_that!(
        complete.operations[0].state,
        eq(crate::management::RefreshState::Completed)
    );
    assert_that!(complete.repositories[0].revision, eq(2));
}

#[googletest::test]
fn original_catalog_projection_shape_reopens_without_rewriting_its_history() {
    use redb::{Database, ReadableTable, TableDefinition};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let id = CatalogId::new();
    Store::create_with_id(&path, id).unwrap();
    let before = Store::open(&path, id).unwrap().events().unwrap();
    {
        let database = Database::open(&path).unwrap();
        let transaction = database.begin_write().unwrap();
        {
            let mut state = transaction
                .open_table(TableDefinition::<&str, &str>::new("state"))
                .unwrap();
            let mut original: serde_json::Value =
                serde_json::from_str(state.get("catalog").unwrap().unwrap().value()).unwrap();
            let original = original.as_object_mut().unwrap();
            for field in [
                "repositories",
                "worktrees",
                "operations",
                "assignments",
                "acquisitions",
                "withheld_worktrees",
                "releases",
            ] {
                original.remove(field);
            }
            state
                .insert("catalog", serde_json::to_string(original).unwrap().as_str())
                .unwrap();
        }
        transaction.commit().unwrap();
    }
    let reopened = Store::open(&path, id).unwrap();
    assert_that!(reopened.projection().unwrap().revision, eq(1));
    assert_that!(
        reopened.projection().unwrap().repositories.is_empty(),
        eq(true)
    );
    assert_that!(reopened.events().unwrap(), eq(&before));
}

fn append_release_fixture_fact(
    store: &Store,
    event: crate::management::ManagementEvent,
    cause: Option<&str>,
) -> String {
    use crate::domain::{decode_event, management_event, management_event_caused_by};
    let state = store.projection().unwrap();
    let wire = if let Some(cause) = cause {
        management_event_caused_by(state.catalog_id, &event, cause).unwrap()
    } else {
        management_event(state.catalog_id, &event).unwrap()
    };
    let id = wire["id"].as_str().unwrap().to_owned();
    store
        .append(
            decode_event(&wire, state.catalog_id)
                .unwrap()
                .expected_revision(&state)
                .unwrap(),
            wire,
        )
        .unwrap();
    id
}
fn seed_branch_release(store: &Store, path: &std::path::Path) -> crate::release::ReleaseOperation {
    use crate::{
        acquisition::{AcquisitionEvent, Assignment, AssignmentState},
        management::{
            AssignmentHandle, ManagementEvent, OperationId, Repository, RepositoryId, Worktree,
            WorktreeId,
        },
        paths::EncodedPath,
        release::{ReleaseEvent, ReleaseOperation, ReleaseState},
    };
    let repository_id = RepositoryId::new();
    let worktree_id = WorktreeId::new();
    let acquisition = OperationId::new();
    let handle = AssignmentHandle::new();
    append_release_fixture_fact(
        store,
        ManagementEvent::RepositoryRegistered(Repository {
            repository_id,
            common_directory: EncodedPath::from_path(path),
            context_path: EncodedPath::from_path(path),
            capacity: 4,
            revision: 0,
        }),
        None,
    );
    append_release_fixture_fact(
        store,
        ManagementEvent::WorktreeRegistered(Worktree {
            repository_id,
            worktree_id,
            path: EncodedPath::from_path(path),
            git_directory: EncodedPath::from_path(&path.join(".git")),
            last_release_position: None,
        }),
        None,
    );
    let cause = append_release_fixture_fact(
        store,
        ManagementEvent::Acquisition(AcquisitionEvent::Reserved(Assignment {
            assignment_handle: handle,
            operation_id: acquisition,
            repository_id,
            worktree_id,
            path: EncodedPath::from_path(path),
            resolved_commit: "1".repeat(40),
            branch: None,
            state: AssignmentState::Preparing,
        })),
        None,
    );
    let cause = append_release_fixture_fact(
        store,
        ManagementEvent::Acquisition(AcquisitionEvent::CheckoutIntended {
            operation_id: acquisition,
            repository_id,
            worktree_id,
        }),
        Some(&cause),
    );
    append_release_fixture_fact(
        store,
        ManagementEvent::Acquisition(AcquisitionEvent::Finished {
            operation_id: acquisition,
            repository_id,
            worktree_id,
            successful: true,
        }),
        Some(&cause),
    );
    let operation_id = OperationId::new();
    let mut op = ReleaseOperation {
        operation_id,
        repository_id,
        worktree_id,
        assignment_handle: handle,
        state: ReleaseState::Intended,
        tip: "1".repeat(40),
        branch: Some("refs/heads/caller".into()),
        preservation_reference: None,
        intent_event_id: String::new(),
        last_checkpoint: "release_intended".into(),
    };
    op.intent_event_id = append_release_fixture_fact(
        store,
        ManagementEvent::Release(ReleaseEvent::Started(op.clone())),
        None,
    );
    op
}
#[googletest::test]
fn release_completion_sync_failure_reopens_with_atomic_ownership_and_recency() {
    use crate::{
        acquisition::AssignmentState,
        domain::{decode_event, management_event_caused_by},
        management::ManagementEvent,
        release::{ReleaseEvent, ReleaseState},
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let catalog_id = CatalogId::new();
    Store::create_with_id(&path, catalog_id).unwrap();
    let store = Store::open(&path, catalog_id).unwrap();
    let operation = seed_branch_release(&store, directory.path());
    let before = store.projection().unwrap();
    let wire = management_event_caused_by(
        catalog_id,
        &ManagementEvent::Release(ReleaseEvent::Finished {
            operation_id: operation.operation_id,
            repository_id: operation.repository_id,
            worktree_id: operation.worktree_id,
            successful: true,
        }),
        &operation.intent_event_id,
    )
    .unwrap();
    let expected = decode_event(&wire, catalog_id)
        .unwrap()
        .expected_revision(&before)
        .unwrap();
    drop(store);
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let backend = SyncFailureBackend {
        inner: redb::backends::FileBackend::new(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap(),
        )
        .unwrap(),
        armed: armed.clone(),
    };
    let store = Store::open_with_fault_backend(catalog_id, backend).unwrap();
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_that!(
        matches!(
            store.append(expected, wire),
            Err(crate::error::PoolError::CommitUnknown)
        ),
        eq(true)
    );
    drop(store);
    let reopened = Store::open(&path, catalog_id).unwrap();
    let state = reopened.projection().unwrap();
    assert_that!(
        state.revision == before.revision || state.revision == before.revision + 1,
        eq(true)
    );
    assert_that!(reopened.events().unwrap().len() as u64, eq(state.revision));
    if state.revision == before.revision {
        assert_that!(state, eq(&before));
    } else {
        assert_that!(state.assignments[0].state, eq(AssignmentState::Released));
        assert_that!(state.releases[0].state, eq(ReleaseState::Completed));
        assert_that!(
            state.worktrees[0].last_release_position,
            eq(Some(state.revision))
        );
    }
}
