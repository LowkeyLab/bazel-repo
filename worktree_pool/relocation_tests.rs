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
    let original = fs::read(&paths.catalog).unwrap();
    let intended = Store::inspect_read_only(&paths.catalog, initial.catalog_id).unwrap();
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
        eq(&intended)
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
    assert_that!(fs::read(&paths.catalog).unwrap(), eq(&original));
    let events =
        Store::events_read_only(&destination.join("catalog.redb"), initial.catalog_id).unwrap();
    assert_that!(events.len(), eq(3));
    assert_that!(
        events[2]["causationid"].as_str(),
        eq(events[1]["id"].as_str())
    );
    assert_that!(fs::read(preserved).unwrap(), eq(&changed));
}

#[googletest::test]
fn exact_relocation_recovers_unclean_semantic_commit_with_damaged_derived_state() {
    for checkpoint in [
        RelocationCheckpoint::StartedCommitted,
        RelocationCheckpoint::CompletionCommitted,
    ] {
        for repair_checkpoint in [
            None,
            Some(RelocationCheckpoint::RepairIntentRecorded),
            Some(RelocationCheckpoint::RepairStoreValidated),
            Some(RelocationCheckpoint::RepairRebuilt),
            Some(RelocationCheckpoint::RepairCompleted),
        ] {
            verify_unclean_relocation_recovery(checkpoint, repair_checkpoint);
        }
    }
}

fn verify_unclean_relocation_recovery(
    checkpoint: RelocationCheckpoint,
    repair_checkpoint: Option<RelocationCheckpoint>,
) {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        catalog: root.path().join("data/catalog.redb"),
        state: root.path().join("state"),
        json: true,
    };
    let initial = catalog::initialize(&paths).unwrap();
    let destination = root.path().join("destination");
    let interrupted = std::panic::catch_unwind(|| {
        relocation::relocate_observed(&paths, &destination, |observed| {
            if observed == checkpoint {
                panic!("interrupt semantic commit before technical publication");
            }
        })
        .unwrap();
    });
    assert_that!(interrupted.is_err(), eq(true));
    let locator: Value =
        serde_json::from_slice(&fs::read(paths.state.join("active.json")).unwrap()).unwrap();
    let id = serde_json::from_value(locator["relocation"]["operation_id"].clone()).unwrap();
    let selected = if checkpoint == RelocationCheckpoint::StartedCommitted {
        paths.catalog.clone()
    } else {
        destination.join("catalog.redb")
    };
    {
        let database = redb::Database::open(&selected).unwrap();
        let transaction = database.begin_write().unwrap();
        transaction
            .open_table(TableDefinition::<&str, &str>::new("state"))
            .unwrap()
            .insert("catalog", "damaged derived projection")
            .unwrap();
        transaction.commit().unwrap();
    }
    catalog::recovery_tests::crash_catalog_writer(&selected);
    assert_that!(
        matches!(
            redb::ReadOnlyDatabase::open(&selected),
            Err(redb::DatabaseError::RepairAborted)
        ),
        eq(true)
    );
    let bytes = fs::read(&selected).unwrap();
    let journal = fs::read(paths.state.join("active.json")).unwrap();
    assert_that!(catalog::open(&paths).is_err(), eq(true));
    assert_that!(catalog::rebuild(&paths).is_err(), eq(true));
    assert_that!(
        catalog::inspect_active_authority(&paths)
            .unwrap()
            .mutations_available(),
        eq(false)
    );
    assert_that!(fs::read(&selected).unwrap(), eq(&bytes));
    assert_that!(
        fs::read(paths.state.join("active.json")).unwrap(),
        eq(&journal)
    );
    if let Some(boundary) = repair_checkpoint {
        let interrupted = std::panic::catch_unwind(|| {
            relocation::resume_observed(&paths, id, |observed| {
                if observed == boundary {
                    panic!("interrupt recorded physical repair");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let journal_path = paths.state.join("active.json");
        let original = fs::read(&journal_path).unwrap();
        if boundary == RelocationCheckpoint::RepairIntentRecorded {
            let mut forged: Value = serde_json::from_slice(&original).unwrap();
            forged["recovery"]["relocation_repair"]["history"]["started"]["time"] =
                serde_json::json!("2000-01-01T00:00:00Z");
            forged["recovery_history"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap()["relocation_repair"]["history"]["started"]["time"] =
                serde_json::json!("2000-01-01T00:00:00Z");
            let corrupted = serde_json::to_vec(&forged).unwrap();
            fs::write(&journal_path, &corrupted).unwrap();
            let bytes = fs::read(&selected).unwrap();
            assert_that!(
                matches!(relocation::resume(&paths, id), Err(PoolError::Corrupt)),
                eq(true)
            );
            assert_that!(fs::read(&selected).unwrap(), eq(&bytes));
            assert_that!(fs::read(&journal_path).unwrap(), eq(&corrupted));
            fs::write(&journal_path, &original).unwrap();
        }
        if boundary == RelocationCheckpoint::RepairRebuilt {
            catalog::recovery_tests::crash_catalog_writer(&selected);
        }
        let bytes = fs::read(&selected).unwrap();
        assert_that!(catalog::open(&paths).is_err(), eq(true));
        assert_that!(catalog::rebuild(&paths).is_err(), eq(true));
        assert_that!(fs::read(&selected).unwrap(), eq(&bytes));
        assert_that!(fs::read(&journal_path).unwrap(), eq(&original));
    }
    let completed = relocation::resume(&paths, id).unwrap();
    assert_that!(completed.operation_id, eq(id));
    let active = Paths {
        catalog: destination.join("catalog.redb"),
        ..paths.clone()
    };
    let state = catalog::inspect_projection(&active).unwrap();
    assert_that!(state.catalog_id, eq(initial.catalog_id));
    assert_that!(state.revision, eq(initial.revision + 2));
    assert_that!(
        state.relocations[0].state,
        eq(crate::relocation_events::RelocationState::Completed)
    );
    assert_that!(
        Store::events_read_only(&active.catalog, initial.catalog_id)
            .unwrap()
            .len(),
        eq(3)
    );
    let authority = catalog::inspect_authority(&active).unwrap();
    assert_that!(authority.mutations_available(), eq(true));
    assert_that!(authority.recovery_history.len(), eq(2));
    assert_that!(authority.recovery_history[0].operation_id == id, eq(false));
    assert_that!(
        authority.recovery_history[0].operation_id,
        eq(authority.recovery_history[1].operation_id)
    );
    assert_that!(
        authority.recovery_history[1].checkpoint.as_str(),
        eq("completed")
    );
}

#[googletest::test]
fn relocation_semantic_sync_failures_inspect_fixed_identity_before_exact_retry() {
    for checkpoint in [
        RelocationCheckpoint::JournalRecorded,
        RelocationCheckpoint::CompletionRecorded,
    ] {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initial = catalog::initialize(&paths).unwrap();
        let destination = root.path().join("destination");
        let interruption = std::panic::catch_unwind(|| {
            relocation::relocate_observed(&paths, &destination, |c| {
                if c == checkpoint {
                    panic!("interrupt before fixed semantic append");
                }
            })
            .unwrap();
        });
        assert_that!(interruption.is_err(), eq(true));
        let journal: Value =
            serde_json::from_slice(&fs::read(paths.state.join("active.json")).unwrap()).unwrap();
        let id = serde_json::from_value(journal["relocation"]["operation_id"].clone()).unwrap();
        let (selected, event, revision) = if checkpoint == RelocationCheckpoint::JournalRecorded {
            (
                paths.catalog.clone(),
                journal["relocation"]["intent_event"].clone(),
                initial.revision,
            )
        } else {
            (
                destination.join("catalog.redb"),
                journal["relocation"]["completion_event"].clone(),
                initial.revision + 1,
            )
        };
        let original = fs::read(&selected).unwrap();
        let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&selected)
            .unwrap();
        let backend = crate::store_tests::SyncFailureBackend {
            inner: redb::backends::FileBackend::new(file).unwrap(),
            armed: armed.clone(),
        };
        let store = Store::open_with_fault_backend(initial.catalog_id, backend).unwrap();
        let failed = store.append_batch_observed(revision, vec![event.clone()], || {
            armed.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        assert_that!(matches!(failed, Err(PoolError::CommitUnknown)), eq(true));
        armed.store(false, std::sync::atomic::Ordering::SeqCst);
        drop(store);
        let after_failure = fs::read(&selected).unwrap();
        let completed = match relocation::resume(&paths, id) {
            Ok(completed) => completed,
            Err(_) => {
                // Unknown physical bytes without accepted evidence require explicit manual restoration.
                let preserved = selected.with_extension("preserved-unknown");
                fs::rename(&selected, &preserved).unwrap();
                fs::write(&selected, &original).unwrap();
                fs::set_permissions(&selected, fs::Permissions::from_mode(0o600)).unwrap();
                assert_that!(fs::read(&preserved).unwrap(), eq(&after_failure));
                relocation::resume(&paths, id).unwrap()
            }
        };
        assert_that!(completed.operation_id, eq(id));
        let events =
            Store::events_read_only(&destination.join("catalog.redb"), initial.catalog_id).unwrap();
        assert_that!(events.len(), eq(3));
        assert_that!(
            events.iter().filter(|e| e["id"] == event["id"]).count(),
            eq(1)
        );
        assert_that!(
            events[2]["causationid"].as_str(),
            eq(events[1]["id"].as_str())
        );
    }
}

#[googletest::test]
fn relocation_explicit_repair_refuses_whitespace_only_replacement_of_prefix_or_fixed_records() {
    for (checkpoint, position) in [
        (RelocationCheckpoint::StartedCommitted, 1),
        (RelocationCheckpoint::StartedCommitted, 2),
        (RelocationCheckpoint::CompletionCommitted, 3),
    ] {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initial = catalog::initialize(&paths).unwrap();
        let destination = root.path().join("destination");
        assert_that!(
            std::panic::catch_unwind(|| relocation::relocate_observed(
                &paths,
                &destination,
                |observed| {
                    if observed == checkpoint {
                        panic!("interrupt semantic append");
                    }
                }
            )
            .unwrap())
            .is_err(),
            eq(true)
        );
        let journal: Value =
            serde_json::from_slice(&fs::read(paths.state.join("active.json")).unwrap()).unwrap();
        let id = serde_json::from_value(journal["relocation"]["operation_id"].clone()).unwrap();
        let selected = if checkpoint == RelocationCheckpoint::CompletionCommitted {
            destination.join("catalog.redb")
        } else {
            paths.catalog.clone()
        };
        {
            let database = redb::Database::open(&selected).unwrap();
            let transaction = database.begin_write().unwrap();
            {
                let mut records = transaction
                    .open_table(TableDefinition::<u64, &str>::new("events"))
                    .unwrap();
                let prior = records.get(position).unwrap().unwrap().value().to_owned();
                records
                    .insert(position, format!("\n {prior}").as_str())
                    .unwrap();
            }
            transaction.commit().unwrap();
        }
        assert_that!(
            Store::inspect_read_only(&selected, initial.catalog_id).is_ok(),
            eq(true)
        );
        catalog::recovery_tests::crash_catalog_writer(&selected);
        let result = relocation::resume(&paths, id);
        assert_that!(result.is_ok(), eq(false));
        assert_that!(matches!(result, Err(PoolError::Conflict)), eq(true));
        let authority = catalog::inspect_active_authority(&paths).unwrap();
        assert_that!(authority.mutations_available(), eq(false));
        assert_that!(
            authority
                .recovery_history
                .last()
                .unwrap()
                .checkpoint
                .as_str(),
            eq("intent_recorded")
        );
        let after_explicit_physical_repair = fs::read(&selected).unwrap();
        let after_journal = fs::read(paths.state.join("active.json")).unwrap();
        assert_that!(catalog::open(&paths).is_err(), eq(true));
        assert_that!(
            fs::read(&selected).unwrap(),
            eq(&after_explicit_physical_repair)
        );
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap(),
            eq(&after_journal)
        );
    }
}
