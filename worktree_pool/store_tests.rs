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
