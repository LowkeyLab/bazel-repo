//! Native worker admission and exact-target recovery. No activation entry point.
use serde::{Deserialize, Serialize};

use crate::{Result, require};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreationContract {
    pub sources: Vec<String>,
    pub parameters: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayDescription {
    pub instance_id: String,
    pub profile_id: String,
    pub authority_epoch: u64,
    pub capabilities: Vec<String>,
    pub session_create: CreationContract,
}
impl GatewayDescription {
    /// # Errors
    /// Refuses a foreign profile or an incomplete deployed native creation contract.
    pub fn validate(&self, profile: &str) -> Result<()> {
        require(
            !self.instance_id.is_empty() && self.profile_id == profile && !profile.is_empty(),
            "gateway identity/profile mismatch",
        )?;
        for capability in ["durable-admission-v1", "local-cli-create-v1"] {
            require(
                self.capabilities.iter().any(|v| v == capability),
                "missing native gateway capability",
            )?;
        }
        require(
            self.session_create.sources.iter().any(|v| v == "cli"),
            "native CLI policy unavailable",
        )?;
        for parameter in ["request_id", "source", "cwd", "skills"] {
            require(
                self.session_create
                    .parameters
                    .iter()
                    .any(|v| v == parameter),
                "missing frozen launch parameter",
            )?;
        }
        Ok(())
    }
}

/// An original pool-receipt capture, never a display-path reconstruction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReceipt {
    directory: std::path::PathBuf,
    summary: serde_json::Value,
    selected: serde_json::Value,
    assignment: crate::model::Identity,
    operation: crate::model::Identity,
    repository: crate::model::Identity,
    worktree: crate::model::Identity,
    path: Vec<u8>,
}
impl PoolReceipt {
    /// # Errors
    /// Rejects incomplete, uncertain, non-active, or lossy pool evidence.
    pub fn from_capture(
        directory: std::path::PathBuf,
        summary: &serde_json::Value,
        selected: serde_json::Value,
    ) -> Result<Self> {
        require(
            directory.is_absolute(),
            "original receipt path must be absolute",
        )?;
        require(
            summary["capture_complete"] == true
                && summary["wrapper_exit"] == 0
                && summary["pool_exit"] == 0
                && summary["pool_outcome"] == "completed"
                && summary["known_catalog_blocker"] == false,
            "pool capture pending, unknown, rejected, or incomplete",
        )?;
        require(
            selected["state"] == "active" && selected["path"]["encoding"] == "unix-bytes",
            "pool assignment is not active or lossless",
        )?;
        let identity = |key: &str| -> Result<crate::model::Identity> {
            selected[key]
                .as_str()
                .ok_or_else(|| crate::Error::Stop("pool identity missing".into()))?
                .to_owned()
                .try_into()
        };
        let path: Vec<u8> = serde_json::from_value(selected["path"]["bytes"].clone())?;
        require(
            path.first() == Some(&b'/') && !path.contains(&0),
            "invalid absolute pool path bytes",
        )?;
        Ok(Self {
            directory,
            summary: summary.clone(),
            assignment: identity("assignment_handle")?,
            operation: identity("operation_id")?,
            repository: identity("repository_id")?,
            worktree: identity("worktree_id")?,
            selected,
            path,
        })
    }
    #[must_use]
    pub fn path_bytes(&self) -> &[u8] {
        &self.path
    }
    #[must_use]
    pub const fn assignment(&self) -> &crate::model::Identity {
        &self.assignment
    }
    #[must_use]
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }
    pub(crate) const fn selected(&self) -> &serde_json::Value {
        &self.selected
    }
    /// # Errors
    /// A JSON gateway cannot name a non-UTF-8 Unix directory. Retain it and stop.
    pub fn gateway_cwd(&self) -> Result<&str> {
        std::str::from_utf8(&self.path).map_err(|_| {
            crate::Error::Stop(
                "native JSON cwd cannot represent pool bytes; no alternate worktree".into(),
            )
        })
    }
    /// # Errors
    /// Revalidates the retained native capture after deserialization.
    pub fn validate(&self) -> Result<()> {
        require(
            Self::from_capture(self.directory.clone(), &self.summary, self.selected.clone())?
                == *self,
            "pool receipt binding changed",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRequest {
    pub profile: String,
    pub principal: String,
    pub mandatory_skills: Vec<String>,
    pub pool: PoolReceipt,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSkill {
    pub name: String,
    /// Digest of the actual resolved content, not the requested name.
    pub content_digest: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchEvidence {
    pub gateway: GatewayDescription,
    /// Trusted host discovery from repository/workflow requirements, independent of caller picks.
    pub mandatory_skills: Vec<String>,
    pub controller_uid: u32,
    pub worker_uid: u32,
    pub isolation_receipt: String,
    pub credential_receipt: String,
    /// A trusted synchronous review-before-correction gate; shell-hook consent is not one.
    pub review_gate: String,
    pub skills: Vec<ResolvedSkill>,
}
impl LaunchEvidence {
    /// # Errors
    /// All native, OS, credential, skill, and synchronous boundary prerequisites are mandatory.
    pub fn validate(&self, request: &WorkerRequest) -> Result<()> {
        use std::os::unix::fs::MetadataExt;
        request.pool.validate()?;
        request.pool.gateway_cwd()?;
        self.gateway.validate(&request.profile)?;
        require(
            !request.principal.is_empty()
                && self.controller_uid == std::fs::metadata("/proc/self")?.uid()
                && self.controller_uid != self.worker_uid
                && self.worker_uid != 0,
            "controller/worker OS identities must differ and bind this process; worker cannot be root",
        )?;
        require(
            !self.isolation_receipt.is_empty() && !self.credential_receipt.is_empty(),
            "missing OS isolation or model/gateway credentials evidence",
        )?;
        require(
            !self.review_gate.is_empty(),
            "mandatory synchronous review-before-repair gate unavailable",
        )?;
        require(
            request.mandatory_skills.iter().any(|s| s == "implement"),
            "implement is mandatory",
        )?;
        require(
            request.mandatory_skills == self.mandatory_skills,
            "caller omitted or changed trusted mandatory skill manifest",
        )?;
        validate_skills(&request.mandatory_skills, &self.skills)
    }
}
fn validate_skills(required: &[String], loaded: &[ResolvedSkill]) -> Result<()> {
    let names: std::collections::BTreeSet<_> = loaded.iter().map(|s| s.name.as_str()).collect();
    let wanted: std::collections::BTreeSet<_> = required.iter().map(String::as_str).collect();
    require(
        names.len() == loaded.len()
            && wanted.len() == required.len()
            && !wanted.contains("")
            && names == wanted,
        "every mandatory skill must resolve and load exactly once",
    )?;
    require(
        loaded.iter().all(|s| {
            s.content_digest.len() == 64
                && s.content_digest
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        }),
        "actual skill content digest missing",
    )
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvidence {
    pub session_id: String,
    pub stored_session_id: String,
    pub profile: String,
    pub principal: String,
    pub creation_id: crate::model::Identity,
    pub loaded_skills: Vec<ResolvedSkill>,
    pub frozen_binding: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Admission {
    pub admission_id: String,
    pub session_id: String,
    pub profile: String,
    pub authority_epoch: u64,
    pub execution_generation: Option<u64>,
    pub status: String,
    pub outcome: Option<String>,
    /// Exact structured native terminal result retained privately, not an implementer acceptance claim.
    pub result: Option<serde_json::Value>,
}
impl Admission {
    /// # Errors
    /// Rejects mismatched, malformed, unknown, regressed, or changed execution identities.
    pub fn validate(&self, identity: &WorkerIdentity) -> Result<()> {
        require(
            !self.admission_id.is_empty()
                && self.admission_id == identity.admission_id
                && self.session_id == identity.session_id
                && self.profile == identity.profile,
            "native admission target mismatch",
        )?;
        require(
            matches!(self.status.as_str(), "queued" | "started" | "terminal"),
            "native execution unknown; do not replay",
        )?;
        require(
            identity.execution_generation.is_none()
                || self.execution_generation == identity.execution_generation,
            "native execution generation changed",
        )?;
        let never_started_cancel = self.status == "terminal"
            && self.outcome.as_deref() == Some("cancelled")
            && self.execution_generation.is_none()
            && identity.execution_generation.is_none();
        require(
            self.status == "queued" || never_started_cancel || self.execution_generation.is_some(),
            "missing execution generation",
        )?;
        require(
            self.status != "terminal"
                || (self.outcome.is_some() && (self.result.is_some() || never_started_cancel)),
            "terminal result missing",
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerIdentity {
    pub run: crate::model::Identity,
    pub session_id: String,
    pub admission_id: String,
    pub profile: String,
    pub principal: String,
    pub execution_generation: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Boundary {
    Clear,
    ReviewFailed { gate: String, findings: String },
}
/// Trusted adapters must resolve all skills, verify isolation/credentials, and bind a mandatory
/// synchronous native boundary BEFORE creating anything. Polling is not that boundary.
pub trait WorkerRuntime: crate::controller::Effects {
    /// # Errors
    /// Missing prerequisites stop admission. No transport or checkout fallback is permitted.
    fn prepare(&mut self, request: &WorkerRequest, issue_url: &str) -> Result<LaunchEvidence>;
    /// # Errors
    /// Reads the exact native admission including its structured terminal result.
    fn observe(&mut self, worker: &WorkerIdentity) -> Result<Admission>;
    /// # Errors
    /// Reads the trusted mandatory gate, never a worker-provided review summary.
    fn boundary(&mut self, worker: &WorkerIdentity) -> Result<Boundary>;
    /// # Errors
    /// Confirms exact cessation AND reconciled effects with unfinished work preserved.
    fn stopped_effects(&mut self, worker: &WorkerIdentity) -> Result<Option<String>>;
}
impl<H, P, N> WorkerRuntime for crate::controller::NativeEffects<H, P, N>
where
    H: WorkerRuntime + crate::controller::Hermes,
    P: crate::controller::Pool,
    N: crate::controller::Notifications,
{
    fn prepare(&mut self, request: &WorkerRequest, issue_url: &str) -> Result<LaunchEvidence> {
        self.hermes.prepare(request, issue_url)
    }
    fn observe(&mut self, worker: &WorkerIdentity) -> Result<Admission> {
        self.hermes.observe(worker)
    }
    fn boundary(&mut self, worker: &WorkerIdentity) -> Result<Boundary> {
        self.hermes.boundary(worker)
    }
    fn stopped_effects(&mut self, worker: &WorkerIdentity) -> Result<Option<String>> {
        self.hermes.stopped_effects(worker)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerPhase {
    Prepared,
    Created,
    Running,
    Implemented,
    RepairBlocked,
    Cancelling,
    Stopped,
    ReviewCiWait,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerPlan {
    pub run: crate::model::Identity,
    pub create: crate::model::Identity,
    pub submit: crate::model::Identity,
    pub cancel: crate::model::Identity,
    pub publication: crate::model::Identity,
    pub issue: crate::model::IssueKey,
    pub job: crate::model::Identity,
    pub request: WorkerRequest,
    pub launch: LaunchEvidence,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerRecord {
    pub plan: WorkerPlan,
    pub phase: WorkerPhase,
    pub session: Option<SessionEvidence>,
    pub admission: Option<Admission>,
    pub stop_reason: Option<String>,
    pub cessation: Option<String>,
    pub publication: Option<PublicationResult>,
}
impl WorkerRecord {
    /// # Errors
    /// Requires the originally bound session and admission identities.
    pub fn identity(&self) -> Result<WorkerIdentity> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| crate::Error::Stop("worker has no exact session".into()))?;
        let admission = self
            .admission
            .as_ref()
            .ok_or_else(|| crate::Error::Stop("worker has no exact admission".into()))?;
        Ok(WorkerIdentity {
            run: self.plan.run.clone(),
            session_id: session.session_id.clone(),
            admission_id: admission.admission_id.clone(),
            profile: session.profile.clone(),
            principal: session.principal.clone(),
            execution_generation: admission.execution_generation,
        })
    }
    /// # Errors
    /// Returns the integrity digest of the private worker receipt for authenticated history.
    pub fn digest(&self) -> Result<String> {
        Ok(crate::history::digest(&serde_json::to_string(self)?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationRequest {
    pub branch: String,
    pub head: String,
    pub title: String,
    pub body: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationResult {
    pub number: u64,
    pub node_id: String,
    pub head: String,
    pub base: String,
    pub branch: String,
    pub intent: crate::model::Identity,
    pub url: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerCommand {
    Create {
        request_id: crate::model::Identity,
        profile: String,
        principal: String,
        cwd: String,
        skills: Vec<String>,
        launch_digest: String,
    },
    Submit {
        session_id: String,
        submission_id: crate::model::Identity,
        text: String,
    },
    Cancel {
        worker: WorkerIdentity,
    },
    Publish {
        request: PublicationRequest,
        intent: crate::model::Identity,
    },
}
impl WorkerCommand {
    /// # Errors
    /// Serializes an exact action payload with its stable native identity.
    pub fn action(
        &self,
        id: &crate::model::Identity,
        issue: &crate::model::IssueKey,
    ) -> Result<crate::model::Action> {
        let target = match self {
            Self::Create { request_id, .. } => format!("native-create:{}", request_id.as_str()),
            Self::Submit {
                session_id,
                submission_id,
                ..
            } => format!("native-submit:{session_id}:{}", submission_id.as_str()),
            Self::Cancel { worker } => format!(
                "native-cancel:{}:{}:{:?}",
                worker.session_id, worker.admission_id, worker.execution_generation
            ),
            Self::Publish { intent, .. } => {
                format!("LowkeyLab/bazel-repo:publication:{}", intent.as_str())
            }
        };
        Ok(crate::model::Action {
            id: id.clone(),
            issue: issue.clone(),
            kind: crate::model::EffectKind::Hermes,
            target,
            payload: serde_json::to_string(self)?,
        })
    }
}
