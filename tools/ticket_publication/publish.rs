//! Creation transitions. A durable intent precedes each remote mutation.
use crate::github::Conflict;
use crate::{
    github,
    plan::{Batch, Blocker, TicketStatus},
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};

pub fn run(batch: &mut Batch, store: &Store) -> Result<()> {
    github::read(&batch.plan.parent)?;
    check_mappings(batch)?;
    loop {
        let next = batch
            .plan
            .tickets
            .iter()
            .enumerate()
            .find(|(index, ticket)| {
                !matches!(batch.tickets[*index].status, TicketStatus::Created(_))
                    && ticket.blockers.iter().all(|blocker| match blocker {
                        Blocker::Issue(_) => true,
                        Blocker::Ticket(key) => batch
                            .tickets
                            .iter()
                            .any(|p| p.key == *key && matches!(p.status, TicketStatus::Created(_))),
                    })
            })
            .map(|(index, _)| index);
        let Some(index) = next else {
            break;
        };
        let ticket = &batch.plan.tickets[index];
        let body = format!("{}\n\n{}", ticket.body, batch.tickets[index].marker);
        let created = if matches!(batch.tickets[index].status, TicketStatus::Creating) {
            let mut found = github::find(&batch.tickets[index].marker)?;
            match found.len() {
                0 => bail!(
                    "Creation remains uncertain. An absent match does not authorize another request."
                ),
                1 => found.remove(0),
                _ => {
                    return Err(anyhow::anyhow!(
                        "multiple exact identity matches require explicit resolution"
                    )
                    .context(Conflict));
                }
            }
        } else {
            batch.tickets[index].status = TicketStatus::Creating;
            store.save(batch)?;
            github::create(&ticket.title, &body)?
        };
        let checked = github::read(&created.mapping().number)?;
        checked
            .check_content(&ticket.title, &body)
            .context(Conflict)?;
        ensure!(created.mapping() == checked.mapping(), Conflict);
        batch.tickets[index].status = TicketStatus::Created(checked.mapping());
        if let Err(error) = store.save(batch) {
            // The rename can succeed before directory synchronization fails.
            // Report uncertainty until a restarted command checks durable state.
            batch.tickets[index].status = TicketStatus::Creating;
            return Err(error);
        }
    }
    ensure!(
        batch
            .tickets
            .iter()
            .all(|p| matches!(p.status, TicketStatus::Created(_))),
        "creation remains incomplete"
    );
    Ok(())
}
fn check_mappings(batch: &Batch) -> Result<()> {
    for (progress, ticket) in batch.tickets.iter().zip(&batch.plan.tickets) {
        if let TicketStatus::Created(mapping) = &progress.status {
            let remote = github::read(&mapping.number)?;
            remote
                .check_content(
                    &ticket.title,
                    &format!("{}\n\n{}", ticket.body, progress.marker),
                )
                .context(Conflict)?;
            ensure!(remote.mapping() == *mapping, Conflict);
        }
    }
    Ok(())
}
