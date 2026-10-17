use book_smartz_domain::{CloudEventDocument, DomainError, Ranking, ReaderId};
use sqlx::{Postgres, Transaction};

use crate::{ApplicationEvent, Store, StoreError, application, ranking};

/// One reader-locked composition scope. Dropping it schedules rollback.
///
/// A scope accepts at most one ranking decision. Failed or cancelled appends
/// poison the scope; only an explicit successful commit establishes success.
/// Keep provider calls outside this scope and configure pool/lock timeouts.
pub struct CommandTransaction {
    transaction: Transaction<'static, Postgres>,
    reader: ReaderId,
    ranking: Ranking,
    appended: bool,
    ranking_documents: Vec<CloudEventDocument>,
    application_history: Vec<ApplicationEvent>,
    application_appended: bool,
    failed: bool,
}

impl Store {
    /// Locks a reader stream and loads its validated ranking before composition.
    ///
    /// # Errors
    /// Returns database failure or corrupt history. Acquisition is subject to caller timeouts.
    pub async fn begin_command(&self, reader: ReaderId) -> Result<CommandTransaction, StoreError> {
        let mut transaction = ranking::locked_transaction(&self.pool, reader).await?;
        let history = ranking::history_in_transaction(&mut transaction, reader).await?;
        let application_history = application::in_transaction(&mut transaction, reader).await?;
        Ok(CommandTransaction {
            transaction,
            reader,
            ranking: history.ranking,
            appended: false,
            ranking_documents: history.documents,
            application_history,
            application_appended: false,
            failed: false,
        })
    }
}

impl CommandTransaction {
    /// Returns the locked ranking, including any staged decision.
    #[must_use]
    pub const fn ranking(&self) -> &Ranking {
        &self.ranking
    }

    /// Returns the complete locked ranking envelopes, including any staged decision.
    #[must_use]
    pub fn ranking_documents(&self) -> &[CloudEventDocument] {
        &self.ranking_documents
    }

    /// Returns the locked application facts, including any staged fact.
    #[must_use]
    pub fn application_history(&self) -> &[ApplicationEvent] {
        &self.application_history
    }

    /// Stages a complete envelope after expected-revision and strict replay validation.
    ///
    /// # Errors
    /// Rejects stale revisions, invalid transitions, repeated appends, or database failure.
    /// Any error makes this scope uncommittable, including earlier staged writes.
    pub async fn append_ranking(
        &mut self,
        document: &CloudEventDocument,
        expected_revision: u64,
    ) -> Result<(), StoreError> {
        if self.failed || self.appended {
            self.failed = true;
            return Err(StoreError::InvalidComposition);
        }
        self.failed = true;
        let actual = self.ranking.projection().revision();
        if actual != expected_revision {
            return Err(DomainError::StaleRevision {
                expected: expected_revision,
                actual,
            }
            .into());
        }
        let mut history = self.ranking.history().to_vec();
        history.push(document.event().clone());
        let next = Ranking::from_history(self.reader, history)
            .map_err(|error| DomainError::InvalidTransition(*error.reason()))?;
        ranking::insert_document(&mut self.transaction, document).await?;
        self.ranking = next;
        self.ranking_documents.push(document.clone());
        self.appended = true;
        self.failed = false;
        Ok(())
    }

    /// Stages one application fact without advancing the ranking revision.
    ///
    /// # Errors
    /// Rejects another reader, duplicate/gapped sequences, a second append, or database failure.
    /// Any error or cancellation poisons the entire composition scope.
    pub async fn append_application(&mut self, event: &ApplicationEvent) -> Result<(), StoreError> {
        if self.failed || self.application_appended {
            self.failed = true;
            return Err(StoreError::InvalidComposition);
        }
        self.failed = true;
        if event.reader() != self.reader
            || self
                .application_history
                .last()
                .map_or(Some(1), |previous| previous.sequence().checked_add(1))
                != Some(event.sequence())
        {
            return Err(StoreError::InvalidComposition);
        }
        application::insert(&mut self.transaction, event).await?;
        self.application_history.push(event.clone());
        self.application_appended = true;
        self.failed = false;
        Ok(())
    }

    /// Commits all staged facts. Cancellation or a lost acknowledgement requires reconciliation.
    ///
    /// # Errors
    /// Returns an invalid scope, definite database rejection, or uncertain commit.
    pub async fn commit(self) -> Result<(), StoreError> {
        if self.failed {
            return Err(StoreError::InvalidComposition);
        }
        self.transaction
            .commit()
            .await
            .map_err(StoreError::from_commit)
    }
}
