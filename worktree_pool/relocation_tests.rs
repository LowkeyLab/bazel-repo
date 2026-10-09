//! Real-storage fault setup; assertions cross public workflow/Store interfaces.
use std::{fs, os::unix::fs::PermissionsExt};

use googletest::{assert_that, matchers::eq};
use redb::{ReadableTable, TableDefinition};
use serde_json::Value;

use crate::{
    catalog,
    error::PoolError,
    paths::Paths,
    relocation::{self, RelocationCheckpoint},
    store::Store,
};

#[googletest::test]
fn relocation_intent_rejects_a_changed_source_even_with_identical_valid_identity_and_revision() {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        catalog: root.path().join("data/catalog.redb"),
        state: root.path().join("state"),
        json: true,
    };
    let initial = catalog::initialize(&paths).unwrap();
    let original = fs::read(&paths.catalog).unwrap();
    let destination = root.path().join("destination");
    let interruption = std::panic::catch_unwind(|| {
        relocation::relocate_observed(&paths, &destination, |checkpoint| {
            if checkpoint == RelocationCheckpoint::IntentRecorded {
                panic!("stop after durable intent");
            }
        })
        .unwrap();
    });
    assert_that!(interruption.is_err(), eq(true));
    let operation = catalog::inspect_authority(&paths)
        .unwrap()
        .relocation
        .unwrap();
    let database = redb::Database::open(&paths.catalog).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut events = transaction
            .open_table(TableDefinition::<u64, &str>::new("events"))
            .unwrap();
        let mut first: Value =
            serde_json::from_str(events.get(1).unwrap().unwrap().value()).unwrap();
        // Recording time does not define replay order or change the projected state.
        first["event"]["time"] = serde_json::json!("2000-01-01T00:00:00Z");
        events
            .insert(1, serde_json::to_string(&first).unwrap().as_str())
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(database);
    assert_that!(
        Store::inspect_read_only(&paths.catalog, initial.catalog_id).unwrap(),
        eq(&initial)
    );
    let changed = fs::read(&paths.catalog).unwrap();
    assert_that!(changed != original, eq(true));
    let locator = fs::read(paths.state.join("active.json")).unwrap();
    assert_that!(
        matches!(
            relocation::resume(&paths, operation.operation_id),
            Err(PoolError::Conflict)
        ),
        eq(true)
    );
    assert_that!(destination.exists(), eq(false));
    assert_that!(fs::read(&paths.catalog).unwrap(), eq(&changed));
    assert_that!(
        fs::read(paths.state.join("active.json")).unwrap(),
        eq(&locator)
    );
    // Manual reconciliation can restore exact bytes without reusing filesystem inode identity.
    let preserved = root.path().join("preserved-changed-source");
    fs::rename(&paths.catalog, &preserved).unwrap();
    fs::write(&paths.catalog, &original).unwrap();
    fs::set_permissions(&paths.catalog, fs::Permissions::from_mode(0o600)).unwrap();
    let completed = relocation::resume(&paths, operation.operation_id).unwrap();
    assert_that!(completed.operation_id, eq(operation.operation_id));
    assert_that!(
        fs::read(destination.join("catalog.redb")).unwrap(),
        eq(&original)
    );
    assert_that!(fs::read(preserved).unwrap(), eq(&changed));
}
