//! Native graph checks. Saved observations never replace fresh relationship reads.
use crate::{
    github::{self, Conflict},
    plan::{Batch, Blocker, GraphProgress, IssueMapping, TicketStatus},
    store::Store,
};
use anyhow::{Context, Result, ensure};

struct TicketGraph {
    issue: IssueMapping,
    parent_present: bool,
    missing: Vec<IssueMapping>,
}
struct Graph {
    tickets: Vec<TicketGraph>,
    progress: GraphProgress,
}

fn mapping(batch: &Batch, index: usize) -> Result<IssueMapping> {
    match &batch.tickets[index].status {
        TicketStatus::Created(mapping) => Ok(mapping.clone()),
        _ => anyhow::bail!("graph requires checked issue mappings"),
    }
}
fn contains(mappings: &[github::Relationship], expected: &IssueMapping) -> Result<bool> {
    for mapping in mappings {
        if mapping.matches(expected)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn read(batch: &Batch) -> Result<Graph> {
    let parent = github::read(&batch.plan.parent)?.mapping();
    let children = github::relationships(&parent.number, "sub_issues")?;
    let mut graph = Graph {
        tickets: Vec::new(),
        progress: GraphProgress::default(),
    };
    for (index, ticket) in batch.plan.tickets.iter().enumerate() {
        let issue = mapping(batch, index)?;
        let remote = github::read(&issue.number)?;
        ensure!(remote.mapping() == issue, Conflict);
        remote.check_content(
            &ticket.title,
            &format!("{}\n\n{}", ticket.body, batch.tickets[index].marker),
        )?;
        let actual_parent = github::parent(&issue)?;
        ensure!(
            actual_parent
                .as_ref()
                .is_none_or(|id| *id == parent.node_id),
            Conflict
        );
        let parent_present = contains(&children, &issue)?;
        ensure!(
            parent_present == actual_parent.is_some(),
            "inconsistent parent relationship reads"
        );
        let existing = github::relationships(&issue.number, "dependencies/blocked_by")?;
        let mut required = Vec::new();
        for blocker in &ticket.blockers {
            let blocker = match blocker {
                Blocker::Issue(number) => github::read(number)?.mapping(),
                Blocker::Ticket(key) => mapping(
                    batch,
                    batch
                        .tickets
                        .iter()
                        .position(|p| p.key == *key)
                        .context("missing blocker mapping")?,
                )?,
            };
            ensure!(blocker != issue, Conflict);
            if !required.contains(&blocker) {
                required.push(blocker);
            }
        }
        graph.progress.parent_links += usize::from(parent_present);
        let mut missing = Vec::new();
        for blocker in &required {
            if contains(&existing, blocker)? {
                graph.progress.dependency_links += 1;
            } else {
                missing.push(blocker.clone());
            }
        }
        graph.tickets.push(TicketGraph {
            issue,
            parent_present,
            missing,
        });
    }
    graph.progress.verified = graph
        .tickets
        .iter()
        .all(|ticket| ticket.parent_present && ticket.missing.is_empty());
    Ok(graph)
}
pub fn observe(batch: &Batch) -> Result<GraphProgress> {
    Ok(read(batch)?.progress)
}

pub fn run(batch: &mut Batch, store: &Store) -> Result<()> {
    // Persist a new observation before any graph mutation. Never trust old receipts.
    batch.graph = GraphProgress::default();
    store.save(batch)?;
    loop {
        let graph = read(batch)?;
        batch.graph = graph.progress;
        store.save(batch)?;
        if batch.graph.verified {
            return Ok(());
        }
        // Apply one missing relationship, then read the whole graph again.
        // A lost response is safe to resume because no parent replacement is allowed.
        let mut changed = false;
        for ticket in graph.tickets {
            if !ticket.parent_present {
                github::add_child(&batch.plan.parent, &ticket.issue)?;
                ensure!(
                    contains(
                        &github::relationships(&batch.plan.parent, "sub_issues")?,
                        &ticket.issue
                    )?,
                    "accepted parent link lacks fresh evidence"
                );
                changed = true;
                break;
            }
            if let Some(blocker) = ticket.missing.first() {
                github::add_blocker(&ticket.issue.number, blocker)?;
                ensure!(
                    contains(
                        &github::relationships(&ticket.issue.number, "dependencies/blocked_by")?,
                        blocker
                    )?,
                    "accepted dependency lacks fresh evidence"
                );
                changed = true;
                break;
            }
        }
        ensure!(changed, "graph remains incomplete");
    }
}
