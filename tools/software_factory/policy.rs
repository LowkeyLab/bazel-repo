use std::collections::BTreeSet;

use crate::{
    Result,
    model::{Issue, IssueKey, PROJECT, REPOSITORY, Snapshot, Work},
    require,
};

pub fn authorization(issue: &Issue, owner: &str) -> Option<Work> {
    let has = |s: &str| issue.labels.iter().any(|label| label == s);
    if issue.key.repository() != REPOSITORY
        || !issue.open
        || issue.pull_request
        || !issue.execution_clear
        || issue.assignees.iter().any(|a| a != owner)
        || issue.blockers_closed.iter().any(|closed| !closed)
        || [
            "needs-info",
            "needs-triage",
            "ready-for-human",
            "blocked",
            "waiting",
        ]
        .iter()
        .any(|label| has(label))
    {
        return None;
    }
    // Type labels retain their HITL contract even if readiness is present.
    if issue.labels.iter().any(|label| {
        label.starts_with("wayfinder:")
            && label != "wayfinder:research"
            && label != "wayfinder:implementation"
    }) {
        return None;
    }
    if has("wayfinder:research") {
        Some(Work::Research)
    } else if has("ready-for-agent") {
        Some(Work::Implementation)
    } else {
        None
    }
}
pub fn validate(snapshot: &Snapshot) -> Result<()> {
    require(
        snapshot.complete && snapshot.project == PROJECT,
        "incomplete or unrelated Project read",
    )?;
    let mut members = BTreeSet::new();
    for key in &snapshot.members {
        require(members.insert(key), "duplicate Project issue")?;
        require(snapshot.issues.contains_key(key), "missing Project issue")?;
    }
    for (key, issue) in &snapshot.issues {
        require(
            key == &issue.key && !issue.node.is_empty(),
            "issue identity mismatch",
        )?;
        chrono::DateTime::parse_from_rfc3339(&issue.created_at)
            .map_err(|_| crate::Error::Stop("invalid creation timestamp".into()))?;
        if let Some(parent) = &issue.parent {
            let parent = snapshot
                .issues
                .get(parent)
                .ok_or_else(|| crate::Error::Stop("missing native parent".into()))?;
            require(
                parent.children.iter().filter(|child| *child == key).count() == 1,
                "contradictory native sibling order",
            )?;
        }
        let mut seen = BTreeSet::new();
        for child in &issue.children {
            require(seen.insert(child), "duplicate native child")?;
            let child = snapshot
                .issues
                .get(child)
                .ok_or_else(|| crate::Error::Stop("missing native child".into()))?;
            require(
                child.parent.as_ref() == Some(key),
                "contradictory native parent",
            )?;
        }
        let mut ancestors = BTreeSet::new();
        let mut current = issue;
        while let Some(parent) = &current.parent {
            require(ancestors.insert(parent), "native parent cycle")?;
            current = snapshot
                .issues
                .get(parent)
                .ok_or_else(|| crate::Error::Stop("missing native ancestor".into()))?;
        }
    }
    Ok(())
}
pub fn select(snapshot: &Snapshot, owner: &str) -> Result<Option<(IssueKey, Work)>> {
    validate(snapshot)?;
    let eligible: BTreeSet<_> = snapshot
        .members
        .iter()
        .filter(|key| authorization(&snapshot.issues[*key], owner).is_some())
        .collect();
    let mut candidates = Vec::new();
    for key in &eligible {
        let issue = &snapshot.issues[*key];
        let earlier = issue.parent.as_ref().is_some_and(|parent| {
            snapshot.issues[parent]
                .children
                .iter()
                .take_while(|child| *child != *key)
                .any(|child| eligible.contains(child))
        });
        if !earlier {
            candidates.push(issue);
        }
    }
    candidates.sort_by_key(|i| {
        (
            chrono::DateTime::parse_from_rfc3339(&i.created_at).expect("validated date"),
            i.key.repository(),
            i.key.number(),
        )
    });
    Ok(candidates
        .first()
        .map(|i| (i.key.clone(), authorization(i, owner).expect("eligible"))))
}

#[derive(Clone, Debug)]
pub struct PollPolicy {
    max_failures: u32,
    cap_seconds: u64,
    failures: u32,
}
impl PollPolicy {
    pub fn new(max_failures: u32, cap_seconds: u64) -> Result<Self> {
        require(
            max_failures > 0 && cap_seconds >= 60,
            "missing approved retry bounds",
        )?;
        Ok(Self {
            max_failures,
            cap_seconds,
            failures: 0,
        })
    }
    pub fn observed(&mut self, success: bool) -> Result<u64> {
        if success {
            self.failures = 0;
            return Ok(60);
        }
        self.failures = self.failures.saturating_add(1);
        require(
            self.failures <= self.max_failures,
            "poll failures exhausted; operator action required",
        )?;
        Ok(60u64
            .saturating_mul(2u64.saturating_pow(self.failures))
            .min(self.cap_seconds))
    }
}
