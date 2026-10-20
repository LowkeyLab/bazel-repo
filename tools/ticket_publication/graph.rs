//! Native graph checks. Saved observations never replace fresh relationship reads.
use crate::{
    github::{self, Conflict},
    plan::{
        Batch, Blocker, GraphProgress, IssueMapping, IssueNumber, MAX_DEPENDENCIES, MAX_SUB_ISSUES,
        TicketStatus,
    },
    store::Store,
};
use anyhow::{Context, Result, ensure};

struct TicketGraph {
    issue: IssueMapping,
    parent_present: bool,
    missing: Vec<IssueMapping>,
}
struct Graph {
    parent: IssueMapping,
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
/// Check remaining capacity before creating issues. Existing planned links use no new slots.
pub fn preflight_capacity(batch: &Batch, issues: &[Option<IssueMapping>]) -> Result<()> {
    ensure!(
        issues.len() == batch.plan.tickets.len(),
        "incomplete observed mappings"
    );
    let parent = &batch.plan.parent;
    let children = github::relationships(parent, "sub_issues")?;
    let mut missing = issues.len();
    for issue in issues.iter().flatten() {
        if contains(&children, issue)? {
            missing -= 1;
        }
    }
    ensure!(
        children
            .len()
            .checked_add(missing)
            .is_some_and(|total| total <= MAX_SUB_ISSUES),
        "publication batch exceeds the remaining native parent capacity"
    );
    preflight_dependencies(&planned_edges(batch, issues)?)
}

#[derive(Clone, PartialEq, Eq)]
enum PlannedIssue {
    Unmapped(usize),
    Existing(IssueMapping),
}
impl PlannedIssue {
    fn observed(index: usize, issues: &[Option<IssueMapping>]) -> Self {
        issues[index]
            .clone()
            .map_or(Self::Unmapped(index), Self::Existing)
    }
}
fn external_mapping(cache: &mut Vec<IssueMapping>, number: &IssueNumber) -> Result<IssueMapping> {
    if let Some(mapping) = cache.iter().find(|m| m.number == *number) {
        return Ok(mapping.clone());
    }
    let mapping = github::read(number)?.mapping();
    cache.push(mapping.clone());
    Ok(mapping)
}
fn planned_edges(
    batch: &Batch,
    issues: &[Option<IssueMapping>],
) -> Result<Vec<(PlannedIssue, PlannedIssue)>> {
    let mut edges = Vec::new();
    let mut external = Vec::new();
    for (index, ticket) in batch.plan.tickets.iter().enumerate() {
        let subject = PlannedIssue::observed(index, issues);
        for blocker in &ticket.blockers {
            let blocker = match blocker {
                Blocker::Issue(number) => {
                    PlannedIssue::Existing(external_mapping(&mut external, number)?)
                }
                Blocker::Ticket(key) => PlannedIssue::observed(
                    batch
                        .tickets
                        .iter()
                        .position(|p| p.key == *key)
                        .context("missing blocker mapping")?,
                    issues,
                ),
            };
            ensure!(subject != blocker, Conflict);
            let edge = (subject.clone(), blocker);
            if !edges.contains(&edge) {
                edges.push(edge);
            }
        }
    }
    Ok(edges)
}
fn check_dependency_capacity(
    source: &PlannedIssue,
    required: &[&PlannedIssue],
    relation: &str,
) -> Result<()> {
    let existing = match source {
        PlannedIssue::Existing(mapping) => github::relationships(&mapping.number, relation)?,
        PlannedIssue::Unmapped(_) => Vec::new(),
    };
    let mut missing = 0;
    for target in required {
        let present = match target {
            PlannedIssue::Existing(mapping) => contains(&existing, mapping)?,
            PlannedIssue::Unmapped(_) => false,
        };
        missing += usize::from(!present);
    }
    ensure!(
        existing
            .len()
            .checked_add(missing)
            .is_some_and(|total| total <= MAX_DEPENDENCIES),
        "publication batch exceeds {relation} capacity"
    );
    Ok(())
}
fn preflight_dependencies(edges: &[(PlannedIssue, PlannedIssue)]) -> Result<()> {
    let reversed: Vec<_> = edges
        .iter()
        .map(|(subject, blocker)| (blocker.clone(), subject.clone()))
        .collect();
    for (relation, direction) in [
        ("dependencies/blocked_by", edges),
        ("dependencies/blocking", reversed.as_slice()),
    ] {
        let mut checked = Vec::new();
        for (source, _) in direction {
            if checked.contains(&source) {
                continue;
            }
            let required: Vec<_> = direction
                .iter()
                .filter(|(subject, _)| subject == source)
                .map(|(_, target)| target)
                .collect();
            check_dependency_capacity(source, &required, relation)?;
            checked.push(source);
        }
    }
    Ok(())
}

fn read(batch: &Batch) -> Result<Graph> {
    let parent = github::read(&batch.plan.parent)?.mapping();
    let children = github::relationships(&parent.number, "sub_issues")?;
    let mut graph = Graph {
        parent: parent.clone(),
        tickets: Vec::new(),
        progress: GraphProgress::default(),
    };
    let mut external = Vec::new();
    for (index, ticket) in batch.plan.tickets.iter().enumerate() {
        let issue = mapping(batch, index)?;
        check_ticket(batch, index)?;
        let parent_present = check_parent(&issue, &parent, &children)?;
        let existing = github::relationships(&issue.number, "dependencies/blocked_by")?;
        let mut required = Vec::new();
        for blocker in &ticket.blockers {
            let blocker = match blocker {
                Blocker::Issue(number) => external_mapping(&mut external, number)?,
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

fn check_ticket(batch: &Batch, index: usize) -> Result<()> {
    let issue = mapping(batch, index)?;
    let remote = github::read(&issue.number)?;
    ensure!(remote.mapping() == issue, Conflict);
    remote.check_content(
        &batch.plan.tickets[index].title,
        &format!(
            "{}\n\n{}",
            batch.plan.tickets[index].body, batch.tickets[index].marker
        ),
    )
}
fn check_parent(
    issue: &IssueMapping,
    parent: &IssueMapping,
    children: &[github::Relationship],
) -> Result<bool> {
    let actual = github::parent(issue)?;
    ensure!(
        actual.as_ref().is_none_or(|id| *id == parent.node_id),
        Conflict
    );
    let present = contains(children, issue)?;
    ensure!(
        present == actual.is_some(),
        "inconsistent parent relationship reads"
    );
    Ok(present)
}
fn ensure_parent(
    batch: &mut Batch,
    store: &Store,
    index: usize,
    issue: &IssueMapping,
    parent: &IssueMapping,
) -> Result<()> {
    check_ticket(batch, index)?;
    let children = github::relationships(&parent.number, "sub_issues")?;
    if !check_parent(issue, parent, &children)? {
        store.save(batch)?;
        github::add_child(&parent.number, issue)?;
        let children = github::relationships(&parent.number, "sub_issues")?;
        ensure!(
            check_parent(issue, parent, &children)?,
            "accepted parent link lacks fresh evidence"
        );
    }
    check_ticket(batch, index)?;
    batch.graph.parent_links += 1;
    store.save(batch)
}
fn ensure_dependency(
    batch: &mut Batch,
    store: &Store,
    index: usize,
    issue: &IssueMapping,
    blocker: &IssueMapping,
) -> Result<()> {
    check_ticket(batch, index)?;
    let existing = github::relationships(&issue.number, "dependencies/blocked_by")?;
    if !contains(&existing, blocker)? {
        store.save(batch)?;
        github::add_blocker(&issue.number, blocker)?;
        ensure!(
            contains(
                &github::relationships(&issue.number, "dependencies/blocked_by")?,
                blocker
            )?,
            "accepted dependency lacks fresh evidence"
        );
    }
    check_ticket(batch, index)?;
    batch.graph.dependency_links += 1;
    store.save(batch)
}

pub fn run(batch: &mut Batch, store: &Store) -> Result<()> {
    batch.graph = GraphProgress::default();
    store.save(batch)?;
    // Check the complete batch before mutations. Check affected issues after each link.
    let graph = read(batch)?;
    batch.graph = graph.progress;
    store.save(batch)?;
    if batch.graph.verified {
        return Ok(());
    }
    for (index, ticket) in graph.tickets.into_iter().enumerate() {
        if !ticket.parent_present {
            ensure_parent(batch, store, index, &ticket.issue, &graph.parent)?;
        }
        for blocker in ticket.missing {
            ensure_dependency(batch, store, index, &ticket.issue, &blocker)?;
        }
    }
    batch.graph = read(batch)?.progress;
    store.save(batch)?;
    ensure!(
        batch.graph.verified,
        "graph changed during publication; inspect and resume"
    );
    Ok(())
}
