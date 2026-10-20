//! Checked publication inputs and stable saved identities.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt::Write};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct TicketKey(String);
impl TryFrom<String> for TicketKey {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        ensure!(
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                && value.as_bytes()[0].is_ascii_lowercase(),
            "invalid ticket key"
        );
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct BatchId(String);
impl BatchId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for BatchId {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        let parsed = Uuid::parse_str(&value)?;
        ensure!(
            parsed.to_string() == value && !parsed.is_nil(),
            "invalid batch identity"
        );
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IssueNumber(std::num::NonZeroU64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub repository: String,
    pub parent: IssueNumber,
    pub tickets: Vec<Ticket>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub key: TicketKey,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    pub blockers: Vec<Blocker>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Blocker {
    Ticket(TicketKey),
    Issue(IssueNumber),
}
impl Plan {
    pub fn check(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported plan version");
        ensure!(
            self.repository == "LowkeyLab/bazel-repo",
            "unsupported repository"
        );
        ensure!(!self.tickets.is_empty(), "plan contains no tickets");
        let mut keys = BTreeSet::new();
        for ticket in &self.tickets {
            ensure!(keys.insert(&ticket.key), "duplicate ticket key");
            ensure!(
                !ticket.title.trim().is_empty() && !ticket.title.contains(['\n', '\r', '\0']),
                "invalid title"
            );
            ensure!(
                !ticket.body.trim().is_empty() && !ticket.body.contains('\0'),
                "invalid body"
            );
            ensure!(
                !ticket.body.contains("<!-- ticket-publication:"),
                "body contains a reserved identity marker"
            );
            let mut labels = BTreeSet::new();
            for label in &ticket.labels {
                ensure!(
                    !label.trim().is_empty()
                        && !label.chars().any(char::is_control)
                        && labels.insert(label),
                    "invalid or duplicate label"
                );
            }
        }
        for ticket in &self.tickets {
            for blocker in &ticket.blockers {
                if let Blocker::Ticket(key) = blocker {
                    ensure!(keys.contains(key), "missing blocker ticket");
                }
            }
        }
        let mut resolved = BTreeSet::new();
        while resolved.len() < self.tickets.len() {
            let before = resolved.len();
            for ticket in &self.tickets {
                if ticket.blockers.iter().all(|b| match b {
                    Blocker::Ticket(key) => resolved.contains(key),
                    Blocker::Issue(_) => true,
                }) {
                    resolved.insert(ticket.key.clone());
                }
            }
            if before == resolved.len() {
                bail!("dependency cycle");
            }
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        let mut fingerprint = String::with_capacity(64);
        for byte in Sha256::digest(serde_json::to_vec(self)?) {
            write!(fingerprint, "{byte:02x}")?;
        }
        Ok(fingerprint)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub key: TicketKey,
    pub marker: String,
    pub status: TicketStatus,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Pending,
    Creating,
    RetryAuthorized,
    Created(IssueMapping),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IssueId(std::num::NonZeroU64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct NodeId(String);
impl TryFrom<String> for NodeId {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        ensure!(
            !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control()),
            "invalid issue node ID"
        );
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueMapping {
    pub number: IssueNumber,
    pub id: IssueId,
    pub node_id: NodeId,
}
impl std::fmt::Display for IssueNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub schema_version: u32,
    #[serde(rename = "batch_id")]
    pub id: BatchId,
    pub plan_fingerprint: String,
    pub plan: Plan,
    pub tickets: Vec<Progress>,
}
impl Batch {
    pub fn prepare(batch_id: BatchId, plan: Plan) -> Result<Self> {
        plan.check()?;
        let tickets = plan
            .tickets
            .iter()
            .map(|t| Progress {
                key: t.key.clone(),
                marker: format!(
                    "<!-- ticket-publication:{}:{} -->",
                    batch_id.as_str(),
                    t.key.0
                ),
                status: TicketStatus::Pending,
            })
            .collect();
        Ok(Self {
            schema_version: 1,
            plan_fingerprint: plan.fingerprint()?,
            id: batch_id,
            plan,
            tickets,
        })
    }
    pub fn check(&self, expected: &BatchId) -> Result<()> {
        ensure!(
            self.schema_version == 1 && &self.id == expected,
            "invalid saved batch identity or version"
        );
        let checked = Self::prepare(self.id.clone(), self.plan.clone())?;
        ensure!(
            checked.plan_fingerprint == self.plan_fingerprint,
            "saved plan fingerprint mismatch"
        );
        ensure!(
            checked.tickets.len() == self.tickets.len()
                && checked
                    .tickets
                    .iter()
                    .zip(&self.tickets)
                    .all(|(expected, saved)| expected.key == saved.key
                        && expected.marker == saved.marker),
            "invalid saved ticket identities or progress"
        );
        let mut mappings = Vec::new();
        for progress in &self.tickets {
            if let TicketStatus::Created(mapping) = &progress.status {
                ensure!(
                    mapping.number != self.plan.parent,
                    "child mapping identifies the parent"
                );
                ensure!(
                    !mappings
                        .iter()
                        .any(|m: &&IssueMapping| m.number == mapping.number
                            || m.id == mapping.id
                            || m.node_id == mapping.node_id),
                    "duplicate saved issue mapping"
                );
                mappings.push(mapping);
            }
        }
        Ok(())
    }
}

impl std::str::FromStr for IssueNumber {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Ok(Self(value.parse()?))
    }
}
impl std::str::FromStr for TicketKey {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::try_from(value.to_owned())
    }
}
