use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Result, require};

pub const REPOSITORY: &str = "LowkeyLab/bazel-repo";
pub const PROJECT: &str = "PVT_kwDOCsIaD84BrIqS";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IssueKey(String);
impl TryFrom<String> for IssueKey {
    type Error = crate::Error;
    fn try_from(s: String) -> Result<Self> {
        let (repo, number) = s
            .rsplit_once('#')
            .ok_or_else(|| crate::Error::Stop("invalid issue key".into()))?;
        require(
            repo.split('/').count() == 2
                && repo.split('/').all(|v| {
                    !v.is_empty()
                        && v.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                }),
            "invalid repository",
        )?;
        let n: u64 = number
            .parse()
            .map_err(|_| crate::Error::Stop("invalid issue number".into()))?;
        require(
            n > 0 && n.to_string() == number,
            "noncanonical issue number",
        )?;
        Ok(Self(s))
    }
}
impl From<IssueKey> for String {
    fn from(v: IssueKey) -> Self {
        v.0
    }
}
impl IssueKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn repository(&self) -> &str {
        self.0.rsplit_once('#').expect("validated key").0
    }
    pub fn number(&self) -> u64 {
        self.0
            .rsplit_once('#')
            .expect("validated key")
            .1
            .parse()
            .expect("validated number")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Identity(String);
impl Identity {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Default for Identity {
    fn default() -> Self {
        Self::new()
    }
}
impl TryFrom<String> for Identity {
    type Error = crate::Error;
    fn try_from(s: String) -> Result<Self> {
        let id = Uuid::parse_str(&s).map_err(|_| crate::Error::Stop("invalid identity".into()))?;
        require(!id.is_nil() && id.to_string() == s, "noncanonical identity")?;
        Ok(Self(s))
    }
}
impl From<Identity> for String {
    fn from(v: Identity) -> Self {
        v.0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Work {
    Implementation,
    Research,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub key: IssueKey,
    pub node: String,
    pub created_at: String,
    pub open: bool,
    pub pull_request: bool,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub blockers_closed: Vec<bool>,
    pub parent: Option<IssueKey>,
    pub children: Vec<IssueKey>,
    /// Trusted execution adapter must establish this, never issue prose.
    pub execution_clear: bool,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub project: String,
    pub complete: bool,
    pub members: Vec<IssueKey>,
    pub issues: BTreeMap<IssueKey, Issue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIdentity {
    pub app_id: String,
    pub actor_id: String,
    pub login: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub issue: IssueKey,
    pub app: Option<AppIdentity>,
    pub body: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    Pool,
    Hermes,
    Notification,
    IssueReport,
    Label,
    HostedReview,
    Check,
    Merge,
    Handoff,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub id: Identity,
    pub issue: IssueKey,
    pub kind: EffectKind,
    /// Exact target and native tool identities, not a search or newest resource.
    pub target: String,
    pub payload: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Selected {
        job: Identity,
        work: Work,
        owner: Identity,
    },
    Ownership {
        previous: Identity,
        owner: Identity,
        evidence: String,
    },
    Intent(Action),
    Outcome {
        action: Identity,
        evidence: String,
    },
    Wait {
        reason: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Active {
    pub issue: IssueKey,
    pub job: Identity,
    pub work: Work,
}
