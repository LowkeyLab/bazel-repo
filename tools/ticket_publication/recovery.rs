//! Explicit local decisions. Recovery never mutates GitHub state.
use crate::{
    github,
    plan::{Batch, IssueNumber, TicketKey, TicketStatus},
    store::Store,
};
use anyhow::{Context, Result, ensure};

pub fn adopt(
    batch: &mut Batch,
    ticket: &TicketKey,
    issue: &IssueNumber,
    store: &Store,
) -> Result<()> {
    let index = batch
        .tickets
        .iter()
        .position(|p| &p.key == ticket)
        .context("unknown ticket key")?;
    let remote = github::read(issue)?;
    remote.check_content(
        &batch.plan.tickets[index].title,
        &format!(
            "{}\n\n{}",
            batch.plan.tickets[index].body, batch.tickets[index].marker
        ),
    )?;
    if let TicketStatus::Created(mapping) = &batch.tickets[index].status {
        ensure!(
            *mapping == remote.mapping(),
            "adoption conflicts with the saved mapping"
        );
    }
    batch.tickets[index].status = TicketStatus::Created(remote.mapping());
    store.save(batch)
}

pub fn authorize_retry(batch: &mut Batch, ticket: &TicketKey, store: &Store) -> Result<()> {
    let progress = batch
        .tickets
        .iter_mut()
        .find(|p| &p.key == ticket)
        .context("unknown ticket key")?;
    ensure!(
        matches!(
            progress.status,
            TicketStatus::Creating | TicketStatus::RetryAuthorized
        ),
        "ticket has no uncertain creation to retry"
    );
    progress.status = TicketStatus::RetryAuthorized;
    store.save(batch)
}

pub fn reconstruct(batch: &mut Batch, store: &Store) -> Result<()> {
    github::read(&batch.plan.parent)?;
    for index in 0..batch.tickets.len() {
        let found = github::find(&batch.tickets[index].marker)?;
        ensure!(
            found.len() <= 1,
            "multiple exact matches require explicit resolution"
        );
        batch.tickets[index].status = if let Some(remote) = found.first() {
            remote.check_content(
                &batch.plan.tickets[index].title,
                &format!(
                    "{}\n\n{}",
                    batch.plan.tickets[index].body, batch.tickets[index].marker
                ),
            )?;
            if let TicketStatus::Created(mapping) = &batch.tickets[index].status {
                ensure!(
                    *mapping == remote.mapping(),
                    "remote evidence conflicts with the exported mapping"
                );
            }
            TicketStatus::Created(remote.mapping())
        } else {
            ensure!(
                !matches!(batch.tickets[index].status, TicketStatus::Created(_)),
                "exported issue mapping is absent from remote evidence"
            );
            TicketStatus::Creating
        };
    }
    batch.check(&batch.id)?;
    store.save_new(batch)
}
