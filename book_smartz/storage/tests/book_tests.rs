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
    let recorded = observations.0.lock().unwrap();
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
    let winner = if a.is_ok() { second } else { work_conflict };
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
