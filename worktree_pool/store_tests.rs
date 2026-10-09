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
pub(crate) struct SyncFailureBackend {
    pub(crate) inner: redb::backends::FileBackend,
    pub(crate) armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
                "creations",
                "catalog_stream_revision",
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
    seed_release(store, path, false)
}

fn seed_release(
    store: &Store,
    path: &std::path::Path,
    detached: bool,
) -> crate::release::ReleaseOperation {
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
        branch: (!detached).then(|| "refs/heads/caller".into()),
        preservation_reference: detached.then(|| format!("refs/worktree-pool/{operation_id}")),
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

fn creation_batch(store: &Store, root: &std::path::Path) -> Vec<serde_json::Value> {
    use crate::{
        acquisition::{AcquisitionEvent, Assignment, AssignmentState},
        creation::CreationEvent,
        domain::management_event,
        management::{AssignmentHandle, ManagementEvent, OperationId, Worktree, WorktreeId},
        paths::EncodedPath,
    };
    let state = store.projection().unwrap();
    let repository_id = state.repositories[0].repository_id;
    let worktree_id = WorktreeId::new();
    let worktree = Worktree {
        worktree_id,
        repository_id,
        path: EncodedPath::from_path(&root.join("created")),
        git_directory: EncodedPath::from_path(&root.join("admin")),
        last_release_position: None,
    };
    let assignment = Assignment {
        assignment_handle: AssignmentHandle::new(),
        operation_id: OperationId::new(),
        repository_id,
        worktree_id,
        path: worktree.path.clone(),
        resolved_commit: "1".repeat(40),
        branch: None,
        state: AssignmentState::Preparing,
    };
    vec![
        management_event(
            state.catalog_id,
            &ManagementEvent::Creation(CreationEvent::Registered {
                operation_id: OperationId::new(),
                worktree: Box::new(worktree),
                assignment: Box::new(assignment.clone()),
            }),
        )
        .unwrap(),
        management_event(
            state.catalog_id,
            &ManagementEvent::Acquisition(AcquisitionEvent::Reserved(assignment)),
        )
        .unwrap(),
    ]
}

#[googletest::test]
fn creation_batch_sync_failure_never_exposes_uncounted_or_unowned_preparation() {
    use crate::{
        management::{ManagementEvent, Repository, RepositoryId, Worktree, WorktreeId},
        paths::EncodedPath,
    };
    for fail in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.redb");
        let id = CatalogId::new();
        Store::create_with_id(&path, id).unwrap();
        {
            let store = Store::open(&path, id).unwrap();
            let repository_id = RepositoryId::new();
            append_release_fixture_fact(
                &store,
                ManagementEvent::RepositoryRegistered(Repository {
                    repository_id,
                    common_directory: EncodedPath::from_path(directory.path()),
                    context_path: EncodedPath::from_path(directory.path()),
                    capacity: 4,
                    revision: 0,
                }),
                None,
            );
            // Immutable historical v1 registration remains on the repository stream.
            append_release_fixture_fact(
                &store,
                ManagementEvent::WorktreeRegistered(Worktree {
                    worktree_id: WorktreeId::new(),
                    repository_id,
                    path: EncodedPath::from_path(&directory.path().join("legacy")),
                    git_directory: EncodedPath::from_path(&directory.path().join("legacy-admin")),
                    last_release_position: None,
                }),
                None,
            );
        }
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
        let store = Store::open_with_fault_backend(id, backend).unwrap();
        let events = creation_batch(&store, directory.path());
        armed.store(fail, std::sync::atomic::Ordering::SeqCst);
        let result = store.append_batch(3, events);
        if fail {
            assert_that!(
                matches!(result, Err(crate::error::PoolError::CommitUnknown)),
                eq(true)
            );
        } else {
            assert_that!(result.is_ok(), eq(true));
        }
        drop(store);
        let store = Store::open(&path, id).unwrap();
        let state = store.projection().unwrap();
        assert_that!(matches!(state.revision, 3 | 5), eq(true));
        assert_that!(store.events().unwrap().len() as u64, eq(state.revision));
        let created = usize::from(state.revision == 5);
        assert_that!(state.worktrees.len(), eq(1 + created));
        assert_that!(state.assignments.len(), eq(created));
        assert_that!(state.creations.len(), eq(created));
        if !fail {
            assert_that!(state.catalog_stream_revision, eq(Some(3)));
            assert_that!(state.repositories[0].revision, eq(2));
            let repository_id = state.repositories[0].repository_id;
            append_release_fixture_fact(
                &store,
                ManagementEvent::WorktreeEnrolled(Worktree {
                    worktree_id: WorktreeId::new(),
                    repository_id,
                    path: EncodedPath::from_path(&directory.path().join("new")),
                    git_directory: EncodedPath::from_path(&directory.path().join("new-admin")),
                    last_release_position: None,
                }),
                None,
            );
            append_release_fixture_fact(
                &store,
                ManagementEvent::RepositoryRegistered(Repository {
                    repository_id: RepositoryId::new(),
                    common_directory: EncodedPath::from_path(&directory.path().join("another")),
                    context_path: EncodedPath::from_path(&directory.path().join("another")),
                    capacity: 4,
                    revision: 0,
                }),
                None,
            );
            let state = store.projection().unwrap();
            assert_that!(state.catalog_stream_revision, eq(Some(5)));
            assert_that!(state.repositories[0].revision, eq(2));
            assert_that!(state.repositories[1].revision, eq(0));
        }
    }
}

fn seed_preserved_release_recovery(
    store: &Store,
    root: &std::path::Path,
) -> crate::recovery::RecoveryOperation {
    use crate::{
        management::{ManagementEvent, OperationId},
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let target = seed_release(store, root, true);
    let mut operation = RecoveryOperation {
        operation_id: OperationId::new(),
        repository_id: target.repository_id,
        worktree_id: target.worktree_id,
        assignment_handle: Some(target.assignment_handle),
        state: RecoveryState::Intended,
        tip: Some(target.tip),
        branch: None,
        preservation_reference: target.preservation_reference,
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: Some(target.operation_id),
        disposition: RecoveryDisposition::Released,
        protected_reason: None,
    };
    let cause = append_release_fixture_fact(
        store,
        ManagementEvent::Recovery(RecoveryEvent::Started(operation.clone())),
        Some(&target.intent_event_id),
    );
    operation.intent_event_id = append_release_fixture_fact(
        store,
        ManagementEvent::Recovery(RecoveryEvent::Preserved {
            operation_id: operation.operation_id,
            repository_id: operation.repository_id,
            worktree_id: operation.worktree_id,
        }),
        Some(&cause),
    );
    operation.state = RecoveryState::Preserved;
    operation.last_checkpoint = "recovery_preserved".into();
    operation
}

#[googletest::test]
fn recovery_completion_sync_failure_keeps_history_ownership_recency_and_linked_release_atomic() {
    use crate::{
        acquisition::AssignmentState,
        domain::{decode_event, management_event_caused_by},
        error::PoolError,
        management::ManagementEvent,
        recovery::{RecoveryEvent, RecoveryState},
        release::ReleaseState,
    };
    for fail_sync in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalog.redb");
        let id = CatalogId::new();
        Store::create_with_id(&path, id).unwrap();
        let store = Store::open(&path, id).unwrap();
        let operation = seed_preserved_release_recovery(&store, directory.path());
        let before = store.projection().unwrap();
        let old_events = store.events().unwrap();
        let wire = management_event_caused_by(
            id,
            &ManagementEvent::Recovery(RecoveryEvent::Finished {
                operation_id: operation.operation_id,
                repository_id: operation.repository_id,
                worktree_id: operation.worktree_id,
                successful: true,
            }),
            &operation.intent_event_id,
        )
        .unwrap();
        let result_id = wire["id"].as_str().unwrap().to_owned();
        let expected_revision = decode_event(&wire, id)
            .unwrap()
            .expected_revision(&before)
            .unwrap();
        // Expected transitions come from the ownership/history contract, independently
        // of the reducer being exercised by append and reopen validation.
        let mut completed = before.clone();
        completed.revision += 1;
        completed.repositories[0].revision += 1;
        completed.assignments[0].state = AssignmentState::Released;
        completed.worktrees[0].last_release_position = Some(completed.revision);
        completed.releases[0].state = ReleaseState::Completed;
        completed.releases[0].last_checkpoint = "release_reconciled".into();
        completed.recoveries[0].state = RecoveryState::Completed;
        completed.recoveries[0].last_checkpoint = "recovery_committed".into();
        completed.recoveries[0]
            .intent_event_id
            .clone_from(&result_id);
        let mut completed_events = old_events.clone();
        completed_events.push(wire.clone());
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
        let store = Store::open_with_fault_backend(id, backend).unwrap();
        armed.store(fail_sync, std::sync::atomic::Ordering::SeqCst);
        let result = store.append(expected_revision, wire.clone());
        if fail_sync {
            assert_that!(matches!(result, Err(PoolError::CommitUnknown)), eq(true));
        } else {
            assert_that!(result.unwrap(), eq(&completed));
        }
        drop(store);

        let reopened = Store::open(&path, id).unwrap();
        let observed = reopened.projection().unwrap();
        let events = reopened.events().unwrap();
        if observed.revision == before.revision {
            assert_that!(fail_sync, eq(true));
            assert_that!(&observed, eq(&before));
            assert_that!(&events, eq(&old_events));
            assert_that!(
                events
                    .iter()
                    .any(|event| event["id"].as_str() == Some(&result_id)),
                eq(false)
            );
            assert_that!(
                reopened.append(expected_revision, wire.clone()).unwrap(),
                eq(&completed)
            );
        } else {
            assert_that!(&observed, eq(&completed));
            assert_that!(&events, eq(&completed_events));
            assert_that!(
                events
                    .iter()
                    .filter(|event| event["id"].as_str() == Some(&result_id))
                    .count(),
                eq(1)
            );
        }
        assert_that!(reopened.projection().unwrap(), eq(&completed));
        assert_that!(reopened.events().unwrap(), eq(&completed_events));
        assert_that!(
            matches!(
                reopened.append_batch(completed.revision, vec![wire.clone()]),
                Err(PoolError::Conflict)
            ),
            eq(true),
        );
        assert_that!(reopened.projection().unwrap(), eq(&completed));
        assert_that!(reopened.events().unwrap(), eq(&completed_events));
        drop(reopened);
        let final_store = Store::open(&path, id).unwrap();
        assert_that!(final_store.projection().unwrap(), eq(&completed));
        assert_that!(final_store.events().unwrap(), eq(&completed_events));
    }
}

#[googletest::test]
fn recovery_replay_rejects_forged_binding_cause_protection_and_unpreserved_release() {
    use crate::{
        domain::{decode_event, management_event_caused_by, reduce},
        management::{ManagementEvent, OperationId},
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.redb");
    let id = CatalogId::new();
    Store::create_with_id(&path, id).unwrap();
    let store = Store::open(&path, id).unwrap();
    let target = seed_release(&store, directory.path(), true);
    let before = store.projection().unwrap();
    let valid = RecoveryOperation {
        operation_id: OperationId::new(),
        repository_id: target.repository_id,
        worktree_id: target.worktree_id,
        assignment_handle: Some(target.assignment_handle),
        state: RecoveryState::Intended,
        tip: Some(target.tip),
        branch: None,
        preservation_reference: target.preservation_reference,
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: Some(target.operation_id),
        disposition: RecoveryDisposition::Released,
        protected_reason: None,
    };
    for invalidity in [
        "identity_alias",
        "cause",
        "target",
        "missing_owner",
        "unassigned_owner",
        "missing_protection",
        "preservation",
    ] {
        let mut invalid = valid.clone();
        let mut cause = target.intent_event_id.clone();
        match invalidity {
            "identity_alias" => invalid.operation_id = target.operation_id,
            "cause" => cause = uuid::Uuid::new_v4().to_string(),
            "target" => invalid.target_operation_id = Some(OperationId::new()),
            "missing_owner" => invalid.assignment_handle = None,
            "unassigned_owner" => invalid.disposition = RecoveryDisposition::Unassigned,
            "missing_protection" => {
                invalid.disposition = RecoveryDisposition::Withheld;
                invalid.preservation_reference = None;
            }
            "preservation" => invalid.preservation_reference = None,
            _ => unreachable!(),
        }
        let wire = management_event_caused_by(
            id,
            &ManagementEvent::Recovery(RecoveryEvent::Started(invalid)),
            &cause,
        )
        .unwrap();
        let fact = decode_event(&wire, id).unwrap();
        assert_that!(
            reduce(Some(&before), before.revision, &fact).is_err(),
            eq(true)
        );
    }
    let wire = management_event_caused_by(
        id,
        &ManagementEvent::Recovery(RecoveryEvent::Started(valid.clone())),
        &target.intent_event_id,
    )
    .unwrap();
    let started = reduce(
        Some(&before),
        before.revision,
        &decode_event(&wire, id).unwrap(),
    )
    .unwrap();
    let finish = management_event_caused_by(
        id,
        &ManagementEvent::Recovery(RecoveryEvent::Finished {
            operation_id: valid.operation_id,
            repository_id: valid.repository_id,
            worktree_id: valid.worktree_id,
            successful: true,
        }),
        wire["id"].as_str().unwrap(),
    )
    .unwrap();
    assert_that!(
        reduce(
            Some(&started),
            started.revision,
            &decode_event(&finish, id).unwrap()
        )
        .is_err(),
        eq(true)
    );
    assert_that!(store.projection().unwrap(), eq(&before));
}

#[googletest::test]
fn rebuild_sync_failure_never_exposes_partial_ownership_or_changes_immutable_history() {
    use crate::{catalog, paths::Paths, rebuild::RebuildCheckpoint};
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initial = catalog::initialize(&paths).unwrap();
        let (expected, history) = {
            let store = Store::open(&paths.catalog, initial.catalog_id).unwrap();
            seed_preserved_release_recovery(&store, root.path());
            (store.projection().unwrap(), store.events().unwrap())
        };
        {
            use redb::{ReadableTable, TableDefinition};
            let database = redb::Database::open(&paths.catalog).unwrap();
            let transaction = database.begin_write().unwrap();
            {
                let mut table = transaction
                    .open_table(TableDefinition::<&str, &str>::new("state"))
                    .unwrap();
                let mut state: serde_json::Value =
                    serde_json::from_str(table.get("catalog").unwrap().unwrap().value()).unwrap();
                state["assignments"] = serde_json::json!([]);
                table
                    .insert("catalog", serde_json::to_string(&state).unwrap().as_str())
                    .unwrap();
            }
            transaction.commit().unwrap();
        }
        let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let backend = SyncFailureBackend {
            inner: redb::backends::FileBackend::new(
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&paths.catalog)
                    .unwrap(),
            )
            .unwrap(),
            armed: armed.clone(),
        };
        let result = Store::rebuild_with_fault_backend(initial.catalog_id, backend, |checkpoint| {
            if checkpoint == RebuildCheckpoint::BeforeCommit {
                armed.store(fail, std::sync::atomic::Ordering::SeqCst);
            }
        });
        armed.store(false, std::sync::atomic::Ordering::SeqCst);
        if fail {
            assert_that!(
                matches!(result, Err(crate::error::PoolError::CommitUnknown)),
                eq(true)
            );
        } else {
            assert_that!(result.unwrap(), eq(&expected));
        }
        // Reopen and observe complete identity/revisions before any explicit new request.
        let observed = catalog::inspect_authority(&paths).unwrap();
        assert_that!(observed.catalog_id, eq(initial.catalog_id));
        if observed.store_state == "unreadable" {
            catalog::reconcile_authority(&paths).unwrap();
        }
        match Store::inspect_read_only(&paths.catalog, initial.catalog_id) {
            Ok(projection) => assert_that!(projection, eq(&expected)),
            Err(crate::error::PoolError::Corrupt) => {
                assert_that!(
                    catalog::inspect_authority(&paths).unwrap().store_state,
                    eq("rebuild_required")
                );
                assert_that!(catalog::open(&paths).is_err(), eq(true));
            }
            Err(error) => panic!("unexpected reopened state: {error:?}"),
        }
        assert_that!(
            Store::inspect_rebuild_history(&paths.catalog, initial.catalog_id).unwrap(),
            eq(&expected)
        );
        assert_that!(catalog::rebuild(&paths).unwrap(), eq(&expected));
        assert_that!(
            Store::events_read_only(&paths.catalog, initial.catalog_id).unwrap(),
            eq(&history)
        );
        let store = Store::open(&paths.catalog, initial.catalog_id).unwrap();
        assert_that!(
            store
                .append_batch(expected.revision, vec![history.last().unwrap().clone()])
                .is_err(),
            eq(true)
        );
        assert_that!(store.projection().unwrap(), eq(&expected));
        assert_that!(store.events().unwrap(), eq(&history));
    }
}

fn seed_retirement(
    store: &Store,
    root: &std::path::Path,
) -> crate::retirement::RetirementOperation {
    use crate::{
        management::{ManagementEvent, OperationId},
        release::ReleaseEvent,
        retirement::{RetirementOperation, RetirementState},
    };
    let release = seed_branch_release(store, root);
    append_release_fixture_fact(
        store,
        ManagementEvent::Release(ReleaseEvent::Finished {
            operation_id: release.operation_id,
            repository_id: release.repository_id,
            worktree_id: release.worktree_id,
            successful: true,
        }),
        Some(&release.intent_event_id),
    );
    RetirementOperation {
        operation_id: OperationId::new(),
        repository_id: release.repository_id,
        worktree_id: release.worktree_id,
        worktree: store.projection().unwrap().worktrees[0].clone(),
        reconciliation_id: release.operation_id,
        reconciliation_checkpoint: release.intent_event_id,
        state: RetirementState::Intended,
        intent_event_id: String::new(),
        last_checkpoint: "retirement_intended".into(),
    }
}

#[googletest::test]
fn retirement_commit_uncertainty_preserves_complete_history_and_exact_capacity_transition() {
    use crate::{
        domain::{decode_event, management_event_caused_by},
        error::PoolError,
        management::ManagementEvent,
        retirement::{RetirementEvent, RetirementState},
    };
    for phase in ["intent", "result"] {
        for fail_sync in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("catalog.redb");
            let id = CatalogId::new();
            Store::create_with_id(&path, id).unwrap();
            let store = Store::open(&path, id).unwrap();
            let mut operation = seed_retirement(&store, root.path());
            if phase == "result" {
                operation.intent_event_id = append_release_fixture_fact(
                    &store,
                    ManagementEvent::Retirement(RetirementEvent::Started(Box::new(
                        operation.clone(),
                    ))),
                    Some(&operation.reconciliation_checkpoint),
                );
            }
            let before = store.projection().unwrap();
            let old_events = store.events().unwrap();
            let event = if phase == "intent" {
                RetirementEvent::Started(Box::new(operation.clone()))
            } else {
                RetirementEvent::Finished {
                    operation_id: operation.operation_id,
                    repository_id: operation.repository_id,
                    worktree_id: operation.worktree_id,
                }
            };
            let cause = if phase == "intent" {
                &operation.reconciliation_checkpoint
            } else {
                &operation.intent_event_id
            };
            let wire =
                management_event_caused_by(id, &ManagementEvent::Retirement(event), cause).unwrap();
            let result_id = wire["id"].as_str().unwrap().to_owned();
            let revision = decode_event(&wire, id)
                .unwrap()
                .expected_revision(&before)
                .unwrap();
            let mut completed = before.clone();
            completed.revision += 1;
            if phase == "intent" {
                completed.repositories[0].revision += 1;
                let mut intended = operation.clone();
                intended.intent_event_id = result_id.clone();
                completed.retirements.push(intended);
            } else {
                completed.catalog_stream_revision = Some(
                    before
                        .catalog_stream_revision
                        .unwrap_or(1 + before.repositories.len() as u64)
                        + 1,
                );
                completed.worktrees.clear();
                completed.retirements[0].state = RetirementState::Completed;
                completed.retirements[0].intent_event_id = result_id.clone();
                completed.retirements[0].last_checkpoint = "retirement_committed".into();
            }
            let mut new_events = old_events.clone();
            new_events.push(wire.clone());
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
            let store = Store::open_with_fault_backend(id, backend).unwrap();
            let result = store.append_batch_observed(before.revision, vec![wire.clone()], || {
                armed.store(fail_sync, std::sync::atomic::Ordering::SeqCst);
            });
            if fail_sync {
                assert_that!(
                    matches!(&result, Err(PoolError::CommitUnknown)),
                    eq(true),
                    "phase={phase} result={result:?}"
                );
            } else {
                assert_that!(result.unwrap(), eq(&completed));
            }
            drop(store);
            let reopened = Store::open(&path, id).unwrap();
            let observed = reopened.projection().unwrap();
            if observed.revision == before.revision {
                assert_that!(fail_sync, eq(true));
                assert_that!(&observed, eq(&before));
                assert_that!(reopened.events().unwrap(), eq(&old_events));
                assert_that!(
                    reopened.append(revision, wire.clone()).unwrap(),
                    eq(&completed)
                );
            } else {
                assert_that!(&observed, eq(&completed));
            }
            assert_that!(reopened.events().unwrap(), eq(&new_events));
            assert_that!(
                matches!(
                    reopened.append_batch(completed.revision, vec![wire]),
                    Err(PoolError::Conflict)
                ),
                eq(true)
            );
            assert_that!(reopened.projection().unwrap(), eq(&completed));
            assert_that!(&completed.assignments, eq(&before.assignments));
            assert_that!(&completed.releases, eq(&before.releases));
            assert_that!(&completed.retirements[0].worktree, eq(&before.worktrees[0]));
        }
    }
}

#[googletest::test]
fn retirement_replay_rejects_forged_reconciliation_binding_cause_and_repeated_results() {
    use crate::{
        domain::{decode_event, management_event_caused_by, reduce},
        management::{ManagementEvent, OperationId, RepositoryId, WorktreeId},
        retirement::{RetirementEvent, RetirementState},
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("catalog.redb");
    let id = CatalogId::new();
    Store::create_with_id(&path, id).unwrap();
    let store = Store::open(&path, id).unwrap();
    let operation = seed_retirement(&store, root.path());
    let state = store.projection().unwrap();
    let events = store.events().unwrap();
    for fault in [
        "proof",
        "checkpoint",
        "repository",
        "worktree",
        "snapshot",
        "identity",
        "state",
        "cause",
    ] {
        let mut invalid = operation.clone();
        match fault {
            "proof" => invalid.reconciliation_id = OperationId::new(),
            "checkpoint" => invalid.reconciliation_checkpoint = uuid::Uuid::new_v4().to_string(),
            "repository" => invalid.repository_id = RepositoryId::new(),
            "worktree" => invalid.worktree_id = WorktreeId::new(),
            "snapshot" => invalid.worktree.last_release_position = None,
            "identity" => invalid.operation_id = invalid.reconciliation_id,
            "state" => invalid.state = RetirementState::Completed,
            _ => {}
        }
        let cause = if fault == "cause" {
            uuid::Uuid::new_v4().to_string()
        } else {
            operation.reconciliation_checkpoint.clone()
        };
        let wire = management_event_caused_by(
            id,
            &ManagementEvent::Retirement(RetirementEvent::Started(Box::new(invalid))),
            &cause,
        )
        .unwrap();
        let typed = decode_event(&wire, id).unwrap();
        assert_that!(
            reduce(Some(&state), state.revision, &typed).is_err(),
            eq(true)
        );
    }
    let started = append_release_fixture_fact(
        &store,
        ManagementEvent::Retirement(RetirementEvent::Started(Box::new(operation.clone()))),
        Some(&operation.reconciliation_checkpoint),
    );
    let result = management_event_caused_by(
        id,
        &ManagementEvent::Retirement(RetirementEvent::Finished {
            operation_id: operation.operation_id,
            repository_id: operation.repository_id,
            worktree_id: operation.worktree_id,
        }),
        &started,
    )
    .unwrap();
    let state = store.projection().unwrap();
    let typed = decode_event(&result, id).unwrap();
    let complete = reduce(Some(&state), state.revision, &typed).unwrap();
    assert_that!(complete.worktrees.is_empty(), eq(true));
    assert_that!(
        reduce(Some(&complete), complete.revision, &typed).is_err(),
        eq(true)
    );
    assert_that!(
        store.events().unwrap()[..events.len()].to_vec(),
        eq(&events)
    );
}
