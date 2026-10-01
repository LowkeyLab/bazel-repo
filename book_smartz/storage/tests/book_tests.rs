use super::support::{Fixture, Recorder};
use book_smartz_domain::{Book, BookId, IdentityError, OpenLibraryWorkId};
use book_smartz_storage::{Operation, OperationContext, Outcome, Registration, Store, StoreError};
use googletest::{assert_that, matchers::eq};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use uuid::Uuid;

fn book(id: u128, work: &str) -> Book {
    Book::new(
        BookId::new(Uuid::from_u128(id)),
        OpenLibraryWorkId::try_from(work).unwrap(),
    )
}

#[googletest::test]
#[tokio::test]
async fn registration_preserves_both_identity_directions() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let context = OperationContext::default();
    store.migrate(context).await.unwrap();
    let first = book(1, "OL1W");
    assert_that!(
        store.register_book(&first, context).await.unwrap(),
        eq(Registration::Created)
    );
    assert_that!(
        store.register_book(&first, context).await.unwrap(),
        eq(Registration::AlreadyPresent)
    );
    assert_that!(
        store.book_by_id(first.id(), context).await.unwrap(),
        eq(&Some(first.clone()))
    );
    assert_that!(
        store
            .book_by_work_id(first.work_id(), context)
            .await
            .unwrap(),
        eq(&Some(first.clone()))
    );
    assert_that!(
        store
            .book_by_id(BookId::new(Uuid::from_u128(9)), context)
            .await
            .unwrap(),
        eq(&None)
    );
    assert_that!(
        store
            .book_by_work_id(&OpenLibraryWorkId::try_from("OL9W").unwrap(), context)
            .await
            .unwrap(),
        eq(&None)
    );
    assert_that!(
        matches!(
            store.register_book(&book(1, "OL2W"), context).await,
            Err(StoreError::Identity(IdentityError::BookIdConflict))
        ),
        eq(true)
    );
    assert_that!(
        matches!(
            store.register_book(&book(2, "OL1W"), context).await,
            Err(StoreError::Identity(IdentityError::WorkIdConflict))
        ),
        eq(true)
    );
    assert_that!(
        store.book_by_id(first.id(), context).await.unwrap(),
        eq(&Some(first.clone()))
    );
    assert_that!(
        store
            .book_by_work_id(first.work_id(), context)
            .await
            .unwrap(),
        eq(&Some(first.clone()))
    );
    assert_that!(
        store
            .book_by_id(BookId::new(Uuid::from_u128(2)), context)
            .await
            .unwrap(),
        eq(&None)
    );
    assert_that!(
        store
            .book_by_work_id(&OpenLibraryWorkId::try_from("OL2W").unwrap(), context)
            .await
            .unwrap(),
        eq(&None)
    );
    let recorded = observations.0.lock().unwrap().clone();
    let registration: Vec<_> = recorded
        .iter()
        .filter(|entry| entry.operation == Operation::Registration)
        .collect();
    assert_that!(registration.len(), eq(4));
    assert_that!(
        registration
            .iter()
            .filter(|entry| entry.outcome == Outcome::Created)
            .count(),
        eq(1)
    );
    assert_that!(
        registration
            .iter()
            .filter(|entry| entry.outcome == Outcome::AlreadyPresent)
            .count(),
        eq(1)
    );
    assert_that!(
        recorded
            .iter()
            .filter(|entry| entry.operation == Operation::BookById)
            .count(),
        eq(4)
    );
    assert_that!(
        recorded
            .iter()
            .filter(|entry| entry.operation == Operation::BookByWorkId)
            .count(),
        eq(4)
    );
    drop(recorded);
    let options = fixture.pool().connect_options().as_ref().clone();
    fixture.pool().close().await;
    let reopened = PgPoolOptions::new().connect_with(options).await.unwrap();
    let reconnected = Store::new(reopened.clone(), observations);
    assert_that!(
        reconnected.book_by_id(first.id(), context).await.unwrap(),
        eq(&Some(first.clone()))
    );
    assert_that!(
        reconnected
            .book_by_work_id(first.work_id(), context)
            .await
            .unwrap(),
        eq(&Some(first))
    );
    reopened.close().await;
}

#[googletest::test]
#[tokio::test]
async fn concurrent_registration_is_idempotent() {
    let fixture = Fixture::new().await;
    let store = fixture.store(Arc::new(Recorder::default()));
    let context = OperationContext::default();
    store.migrate(context).await.unwrap();
    let first = book(1, "OL1W");
    let (a, b) = tokio::join!(
        store.register_book(&first, context),
        store.register_book(&first, context)
    );
    let results = [a.unwrap(), b.unwrap()];
    assert_that!(
        results
            .iter()
            .filter(|result| **result == Registration::Created)
            .count(),
        eq(1)
    );
    assert_that!(
        results
            .iter()
            .filter(|result| **result == Registration::AlreadyPresent)
            .count(),
        eq(1)
    );
    let second = book(2, "OL2W");
    let work_conflict = book(3, "OL2W");
    let (a, b) = tokio::join!(
        store.register_book(&second, context),
        store.register_book(&work_conflict, context)
    );
    assert_that!(
        [a.is_ok(), b.is_ok()]
            .iter()
            .filter(|success| **success)
            .count(),
        eq(1)
    );
    let (winner, loser, rejected) = if a.is_ok() {
        (second, work_conflict, b)
    } else {
        (work_conflict, second, a)
    };
    assert_that!(
        matches!(
            rejected,
            Err(StoreError::Identity(IdentityError::WorkIdConflict))
        ),
        eq(true)
    );
    assert_that!(
        store.book_by_id(loser.id(), context).await.unwrap(),
        eq(&None)
    );
    assert_that!(
        store
            .book_by_work_id(winner.work_id(), context)
            .await
            .unwrap(),
        eq(&Some(winner))
    );
    assert_that!(
        matches!(
            store.register_book(&book(1, "OL2W"), context).await,
            Err(StoreError::Identity(IdentityError::BookIdConflict))
        ),
        eq(true)
    );
}

#[googletest::test]
#[tokio::test]
async fn overlapping_registration_preserves_identity_contract_with_stronger_pool_defaults() {
    let fixture = Fixture::new().await;
    let context = OperationContext::default();
    fixture
        .store(Arc::new(Recorder::default()))
        .migrate(context)
        .await
        .unwrap();
    for isolation in ["repeatable read", "serializable"] {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .after_connect(move |connection, _| {
                Box::pin(async move {
                    sqlx::query(if isolation == "repeatable read" {
                        "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL REPEATABLE READ"
                    } else {
                        "SET SESSION CHARACTERISTICS AS TRANSACTION ISOLATION LEVEL SERIALIZABLE"
                    })
                    .execute(connection)
                    .await?;
                    Ok(())
                })
            })
            .connect_with(fixture.pool().connect_options().as_ref().clone())
            .await
            .unwrap();
        let (configured,): (String,) = sqlx::query_as("SHOW default_transaction_isolation")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_that!(configured.as_str(), eq(isolation));
        let store = Store::new(pool.clone(), Arc::new(Recorder::default()));
        for candidate in [book(1, "OL1W"), book(2, "OL1W"), book(1, "OL2W")] {
            let winner = book(1, "OL1W");
            let mut holder = fixture.pool().begin().await.unwrap();
            sqlx::query("INSERT INTO book_smartz.books (id, work_id) VALUES ($1, $2)")
                .bind(winner.id().as_uuid())
                .bind(winner.work_id().as_str())
                .execute(&mut *holder)
                .await
                .unwrap();
            let competing_store = store.clone();
            let competing_book = candidate.clone();
            let registration = tokio::spawn(async move {
                competing_store
                    .register_book(&competing_book, context)
                    .await
            });
            // The INSERT has acquired its snapshot and waits for our uncommitted
            // mapping. Commit only after observing that actual database overlap.
            tokio::time::timeout(std::time::Duration::from_secs(20), async {
                loop {
                    let (waiting,): (i64,) = sqlx::query_as(
                        "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE 'INSERT INTO book_smartz.books%ON CONFLICT%'",
                    ).fetch_one(fixture.pool()).await.unwrap();
                    if waiting == 1 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            holder.commit().await.unwrap();
            let result = registration.await.unwrap();
            if candidate == winner {
                assert_that!(result.unwrap(), eq(Registration::AlreadyPresent));
            } else if candidate.id() == winner.id() {
                assert_that!(
                    matches!(
                        result,
                        Err(StoreError::Identity(IdentityError::BookIdConflict))
                    ),
                    eq(true)
                );
                assert_that!(
                    store
                        .book_by_work_id(candidate.work_id(), context)
                        .await
                        .unwrap(),
                    eq(&None)
                );
            } else {
                assert_that!(
                    matches!(
                        result,
                        Err(StoreError::Identity(IdentityError::WorkIdConflict))
                    ),
                    eq(true)
                );
                assert_that!(
                    store.book_by_id(candidate.id(), context).await.unwrap(),
                    eq(&None)
                );
            }
            assert_that!(
                store.book_by_id(winner.id(), context).await.unwrap(),
                eq(&Some(winner.clone()))
            );
            assert_that!(
                store
                    .book_by_work_id(winner.work_id(), context)
                    .await
                    .unwrap(),
                eq(&Some(winner))
            );
            sqlx::query("DELETE FROM book_smartz.books")
                .execute(fixture.pool())
                .await
                .unwrap();
        }
        let (unchanged,): (String,) = sqlx::query_as("SHOW default_transaction_isolation")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_that!(unchanged.as_str(), eq(isolation));
        pool.close().await;
    }
}
