//! Label publication follows a complete graph check and ends with fresh evidence.
use crate::progress::{Context as ProgressContext, Phase, Step};
use crate::{
    github::{self, Conflict},
    graph,
    plan::{Batch, LabelProgress, LabelsProgress, TicketStatus},
    store::Store,
};
use anyhow::{Result, ensure};

fn ticket(github: &github::Client<'_>, batch: &Batch, index: usize) -> Result<LabelProgress> {
    let github = &github.scope(ProgressContext::ticket(Phase::Labels, batch, index));
    let actual = graph::check_ticket(github, batch, index)?.labels()?;
    let planned = &batch.plan.tickets[index];
    let mut triage = Vec::new();
    for label in actual.iter().chain(&planned.labels) {
        if crate::plan::is_triage_label(label)
            && !triage
                .iter()
                .any(|name: &String| name.eq_ignore_ascii_case(label))
        {
            triage.push(label.clone());
        }
    }
    ensure!(triage.len() <= 1, Conflict);
    let mut progress = LabelProgress {
        key: planned.key.clone(),
        applied: Vec::new(),
        remaining: Vec::new(),
    };
    for label in &planned.labels {
        if actual.iter().any(|name| name.eq_ignore_ascii_case(label)) {
            progress.applied.push(label.clone());
        } else {
            progress.remaining.push(label.clone());
        }
    }
    Ok(progress)
}

pub fn observe(github: &github::Client<'_>, batch: &Batch) -> Result<LabelsProgress> {
    let tickets = (0..batch.tickets.len())
        .map(|index| ticket(github, batch, index))
        .collect::<Result<Vec<_>>>()?;
    let verified = tickets.iter().all(|progress| progress.remaining.is_empty());
    Ok(LabelsProgress {
        checked: true,
        tickets,
        verified,
    })
}

pub fn run(github: &github::Client<'_>, batch: &mut Batch, store: &Store) -> Result<()> {
    ensure!(
        batch.graph.verified,
        "labels require a checked complete graph"
    );
    batch.labels = observe(github, batch)?;
    store.save(batch)?;
    for index in 0..batch.tickets.len() {
        let github = &github.scope(ProgressContext::ticket(Phase::Labels, batch, index));
        // Check content and labels again immediately before the dependent mutation.
        batch.labels.tickets[index] = ticket(github, batch, index)?;
        let remaining = batch.labels.tickets[index].remaining.clone();
        if remaining.is_empty() {
            continue;
        }
        batch.labels.verified = false;
        store.save(batch)?;
        let TicketStatus::Created(mapping) = &batch.tickets[index].status else {
            anyhow::bail!("labels require checked issue mappings");
        };
        github.add_labels(&mapping.number, &remaining)?;
        batch.labels.tickets[index] = ticket(github, batch, index)?;
        ensure!(
            batch.labels.tickets[index].remaining.is_empty(),
            "accepted labels lack fresh evidence"
        );
        store.save(batch)?;
        github.verified(Step::Labels);
    }
    let github = &github.scope(ProgressContext::new(Phase::FinalVerification));
    // Neither a successful write nor a saved observation establishes completion.
    batch.graph = graph::observe(github, batch)?;
    ensure!(
        batch.graph.verified,
        "graph changed during label publication"
    );
    batch.labels = observe(github, batch)?;
    ensure!(batch.labels.verified, "labels changed during publication");
    store.save(batch)
}
