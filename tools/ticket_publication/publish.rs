//! Creation transitions. A durable intent precedes each remote mutation.
use crate::github::Conflict;
use crate::{
    github,
    plan::{Batch, Blocker, IssueMapping, TicketStatus},
    store::Store,
};
use anyhow::{Context, Result, bail, ensure};

pub fn run(github: &github::Client<'_>, batch: &mut Batch, store: &Store) -> Result<()> {
    let observed = observe(github, batch)?;
    crate::graph::preflight_capacity(github, batch, &observed)?;
    github.verified(crate::progress::Step::RemoteChecks);
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
        let github = &github.scope(crate::progress::Context::ticket(
            crate::progress::Phase::Creation,
            batch,
            index,
        ));
        let ticket = &batch.plan.tickets[index];
        let body = format!("{}\n\n{}", ticket.body, batch.tickets[index].marker);
        let created = if let Some(mapping) = &observed[index] {
            let remote = github.read(&mapping.number)?;
            ensure!(remote.mapping() == *mapping, Conflict);
            remote
        } else if matches!(
            batch.tickets[index].status,
            TicketStatus::Creating | TicketStatus::RetryAuthorized
        ) {
            let mut found = github.find(&batch.tickets[index].marker)?;
            match found.len() {
                0 if matches!(batch.tickets[index].status, TicketStatus::RetryAuthorized) => {
                    batch.tickets[index].status = TicketStatus::Creating;
                    store.save(batch)?;
                    github.create(&ticket.title, &body)?
                }
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
            github.create(&ticket.title, &body)?
        };
        let checked = github.read(&created.mapping().number)?;
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
        github
            .scope(crate::progress::Context::ticket(
                crate::progress::Phase::Creation,
                batch,
                index,
            ))
            .verified(crate::progress::Step::Issue);
    }
    ensure!(
        batch
            .tickets
            .iter()
            .all(|p| matches!(p.status, TicketStatus::Created(_))),
        "creation remains incomplete"
    );
    crate::graph::run(github, batch, store)?;
    crate::labels::run(github, batch, store)
}
/// Read remote evidence without changing saved progress or GitHub state.
pub fn observe(github: &github::Client<'_>, batch: &Batch) -> Result<Vec<Option<IssueMapping>>> {
    github.read(&batch.plan.parent)?;
    let issues = if batch
        .tickets
        .iter()
        .any(|p| !matches!(p.status, TicketStatus::Created(_)))
    {
        github.enumerate()?
    } else {
        Vec::new()
    };
    batch
        .tickets
        .iter()
        .zip(&batch.plan.tickets)
        .map(|(progress, ticket)| {
            let github = &github.scope(
                crate::progress::Context::new(crate::progress::Phase::RemoteChecks)
                    .related(crate::progress::Subject::Ticket(ticket.key.clone())),
            );
            let remote = if let TicketStatus::Created(mapping) = &progress.status {
                let remote = github.read(&mapping.number)?;
                ensure!(remote.mapping() == *mapping, Conflict);
                Some(remote)
            } else {
                let mut found: Vec<_> = issues
                    .iter()
                    .filter(|issue| issue.has_marker(&progress.marker))
                    .cloned()
                    .collect();
                ensure!(found.len() <= 1, Conflict);
                found.pop()
            };
            if let Some(remote) = remote {
                remote
                    .check_content(
                        &ticket.title,
                        &format!("{}\n\n{}", ticket.body, progress.marker),
                    )
                    .context(Conflict)?;
                Ok(Some(remote.mapping()))
            } else {
                Ok(None)
            }
        })
        .collect()
}

/// Report one publication attempt, including resumptions and stopped outcomes.
pub fn publication(
    github: &github::Client<'_>,
    saved: &mut Batch,
    store: &Store,
) -> (crate::progress::Outcome, Result<()>) {
    saved.graph = crate::plan::GraphProgress::default();
    saved.labels.checked = false;
    saved.labels.verified = false;
    let result = run(github, saved, store);
    if result.is_err() {
        saved.graph.verified = false;
        saved.labels.verified = false;
    }
    let status = if result.is_ok() {
        crate::progress::Outcome::Complete
    } else if result
        .as_ref()
        .err()
        .is_some_and(|e| e.downcast_ref::<github::Conflict>().is_some())
    {
        crate::progress::Outcome::Conflict
    } else if result.is_err()
        && saved.tickets.iter().any(|p| {
            matches!(
                p.status,
                crate::plan::TicketStatus::Creating | crate::plan::TicketStatus::RetryAuthorized
            )
        })
    {
        crate::progress::Outcome::Uncertain
    } else {
        crate::progress::Outcome::Partial
    };
    github.progress.finish(status);
    (status, result)
}
