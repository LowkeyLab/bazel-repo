//! Supported native JSON-RPC mapping. Missing trusted host extensions fail closed.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    Error, Result,
    controller::{Effects, Hermes, Observation},
    model::{Action, Identity},
    require,
    worker::{
        Admission, Boundary, GatewayDescription, LaunchEvidence, PoolReceipt, PublicationRequest,
        PublicationResult, SessionEvidence, WorkerCommand, WorkerIdentity, WorkerRequest,
        WorkerRuntime,
    },
};

/// An authenticated connection to the existing composed gateway, never a substitute agent.
pub trait NativeRpc {
    /// # Errors
    /// Returns native protocol/authentication errors without changing transport or retrying.
    fn call(&mut self, method: &str, params: Value) -> Result<Value>;
}
/// Operator-provisioned trusted host boundary. No method here is an invented gateway RPC.
/// Implementations must use reviewed supported supervision/extension receipts, not worker prose.
pub trait NativeHost {
    /// # Errors
    /// Resolves ALL mandatory content and verifies OS, credentials, and synchronous review gate.
    fn prerequisites(
        &mut self,
        request: &WorkerRequest,
        gateway: &GatewayDescription,
        issue_url: &str,
    ) -> Result<LaunchEvidence>;
    /// # Errors
    /// Revalidates this exact effect against the protected host launch authorization.
    fn authorize(&mut self, action: &Action) -> Result<()>;
    /// # Errors
    /// Binds actual frozen loaded content and original owner to the exact native creation.
    /// `launch_request.skills` alone is insufficient: native creation can skip partial misses.
    fn frozen_session(&mut self, action: &Action, info: &Value) -> Result<SessionEvidence>;
    /// # Errors
    /// Reads an original exact native receipt after transport loss. Absence never permits resend.
    fn exact_reply(&mut self, action: &Action) -> Result<Option<Value>>;
    /// # Errors
    /// Reads a mandatory synchronous review-before-correction gate; not a polling/cancel race.
    fn boundary(&mut self, worker: &WorkerIdentity) -> Result<Boundary>;
    /// # Errors
    /// Proves exact cessation plus reconciled effects and retained unfinished pool work.
    fn stopped_effects(&mut self, worker: &WorkerIdentity) -> Result<Option<String>>;
}
pub trait PoolInspector {
    /// # Errors
    /// Verifies the original receipt and exact current assignment. Never acquires or recovers.
    fn verify(&mut self, receipt: &PoolReceipt) -> Result<()>;
}
pub trait PublicationApi {
    /// # Errors
    /// Publishes exactly one PR for this durable intent. Unknown acknowledgement is not a retry.
    fn publish(&mut self, request: &PublicationRequest, intent: &Identity) -> Result<()>;
    /// # Errors
    /// Reads the exact PR marker, repository, branch, head, and base; rejects ambiguity.
    fn inspect(
        &mut self,
        request: &PublicationRequest,
        intent: &Identity,
    ) -> Result<Option<PublicationResult>>;
}

/// The stock gateway does not demonstrate the required review-before-repair contract.
/// This explicit default blocks admission; `accept_hooks` cannot clear it.
pub struct UnprovisionedHost;
fn missing<T>() -> Result<T> {
    Err(Error::Stop("trusted native review gate, all-skill frozen-content proof, OS isolation, credentials, and exact recovery prerequisites are not provisioned".into()))
}
impl NativeHost for UnprovisionedHost {
    fn prerequisites(
        &mut self,
        _: &WorkerRequest,
        _: &GatewayDescription,
        _: &str,
    ) -> Result<LaunchEvidence> {
        missing()
    }
    fn authorize(&mut self, _: &Action) -> Result<()> {
        missing()
    }
    fn frozen_session(&mut self, _: &Action, _: &Value) -> Result<SessionEvidence> {
        missing()
    }
    fn exact_reply(&mut self, _: &Action) -> Result<Option<Value>> {
        missing()
    }
    fn boundary(&mut self, _: &WorkerIdentity) -> Result<Boundary> {
        missing()
    }
    fn stopped_effects(&mut self, _: &WorkerIdentity) -> Result<Option<String>> {
        missing()
    }
}

pub struct NativeWorker<R, H, P, J> {
    rpc: R,
    host: H,
    pool: P,
    publications: J,
    /// Only an optimization for acknowledged replies. Durable authority is `Controller` history/`Store`;
    /// a restarted adapter must obtain exact native evidence from the trusted host or stop.
    replies: BTreeMap<Identity, Value>,
}
impl<R, H, P, J> NativeWorker<R, H, P, J> {
    #[must_use]
    pub const fn new(rpc: R, host: H, pool: P, publications: J) -> Self {
        Self {
            rpc,
            host,
            pool,
            publications,
            replies: BTreeMap::new(),
        }
    }
}
fn string(value: &Value, name: &str) -> Result<String> {
    value[name]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::Stop(format!("missing native {name}")))
}
/// Parse the canonical `AdmissionReceipt`, not the standalone sidecar's streaming response.
///
/// # Errors
/// Rejects malformed identities/status/generation. Unknown remains explicit for controller stop.
pub fn parse_admission(value: &Value) -> Result<Admission> {
    let status = string(value, "status")?;
    require(
        matches!(
            status.as_str(),
            "queued" | "started" | "unknown" | "terminal"
        ),
        "unrecognized native admission status",
    )?;
    let authority_epoch = value["authority_epoch"]
        .as_u64()
        .ok_or_else(|| Error::Stop("missing native authority epoch".into()))?;
    let execution_generation = match value.get("execution_generation") {
        Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .ok_or_else(|| Error::Stop("invalid native generation".into()))?,
        ),
        None => return Err(Error::Stop("missing native generation field".into())),
    };
    require(
        matches!(status.as_str(), "queued" | "unknown")
            || (status == "terminal" && value["outcome"] == "cancelled")
            || execution_generation.is_some(),
        "nonqueued admission lacks generation",
    )?;
    Ok(Admission {
        admission_id: string(value, "admission_id")?,
        session_id: string(&value["ref"], "session_id")?,
        profile: string(&value["ref"], "profile_id")?,
        authority_epoch,
        execution_generation,
        status,
        outcome: value["outcome"].as_str().map(str::to_owned),
        result: value.get("result").filter(|v| !v.is_null()).cloned(),
    })
}
impl<R: NativeRpc, H: NativeHost, P: PoolInspector, J: PublicationApi> Effects
    for NativeWorker<R, H, P, J>
{
    fn send(&mut self, action: &Action) -> Result<()> {
        let command: WorkerCommand = serde_json::from_str(&action.payload)?;
        require(
            command.action(&action.id, &action.issue)? == *action,
            "changed native command target",
        )?;
        self.host.authorize(action)?;
        let reply = match &command {
            WorkerCommand::Create {
                request_id,
                profile,
                cwd,
                skills,
                ..
            } => {
                require(request_id == &action.id, "create identity mismatch")?;
                let gateway: GatewayDescription =
                    serde_json::from_value(self.rpc.call("runtime.describe", json!({}))?)?;
                gateway.validate(profile)?;
                self.rpc.call("session.create",json!({"request_id":request_id.as_str(),"source":"cli","cwd":cwd,"skills":skills}))?
            }
            WorkerCommand::Submit {
                session_id,
                submission_id,
                text,
            } => {
                require(submission_id == &action.id, "submit identity mismatch")?;
                self.rpc.call("prompt.submit",json!({"session_id":session_id,"submission_id":submission_id.as_str(),"text":text}))?
            }
            WorkerCommand::Cancel { worker } => self.rpc.call(
                "prompt.cancel",
                json!({"session_id":worker.session_id,"admission_id":worker.admission_id}),
            )?,
            WorkerCommand::Publish { request, intent } => {
                require(intent == &action.id, "publication identity mismatch")?;
                self.publications.publish(request, intent)?;
                Value::Null
            }
        };
        self.replies.insert(action.id.clone(), reply);
        Ok(())
    }
    fn inspect_exact(&mut self, action: &Action) -> Result<Observation> {
        let command: WorkerCommand = serde_json::from_str(&action.payload)?;
        require(
            command.action(&action.id, &action.issue)? == *action,
            "changed native inspection target",
        )?;
        if let WorkerCommand::Publish { request, intent } = &command {
            return self.publications.inspect(request, intent)?.map_or(
                Ok(Observation::Unknown),
                |result| {
                    Ok(Observation::Confirmed {
                        action: action.clone(),
                        evidence: serde_json::to_string(&result)?,
                    })
                },
            );
        }
        let reply = if let Some(reply) = self.host.exact_reply(action)? {
            reply
        } else if let Some(reply) = self.replies.get(&action.id) {
            reply.clone()
        } else {
            return Ok(Observation::Unknown);
        };
        let evidence = match command {
            WorkerCommand::Create {
                request_id,
                profile,
                principal,
                cwd,
                skills,
                ..
            } => {
                let session = string(&reply, "session_id")?;
                require(
                    string(&reply, "stored_session_id")? == session,
                    "native logical identity mismatch",
                )?;
                let info = self
                    .rpc
                    .call("session.info", json!({"session_id":session}))?;
                let launch = &info["launch_request"];
                require(
                    info["profile_id"] == profile
                        && info["cwd"] == cwd
                        && launch["request_id"] == request_id.as_str()
                        && launch["source"] == "cli"
                        && launch["cwd"] == cwd
                        && launch["skills"] == serde_json::to_value(&skills)?,
                    "native frozen launch mismatch",
                )?;
                let frozen = self.host.frozen_session(action, &info)?;
                require(
                    frozen.session_id == session
                        && frozen.stored_session_id == session
                        && frozen.creation_id == request_id
                        && frozen.profile == profile
                        && frozen.principal == principal
                        && !frozen.frozen_binding.is_empty(),
                    "trusted frozen session binding mismatch",
                )?;
                serde_json::to_string(&frozen)?
            }
            WorkerCommand::Submit { session_id, .. } => {
                let admitted = parse_admission(&reply)?;
                require(
                    admitted.session_id == session_id,
                    "submission admission belongs to another session",
                )?;
                let receipt = self.rpc.call("prompt.receipt",json!({"session_id":session_id,"admission_id":admitted.admission_id,"include_result":true}))?;
                let observed = parse_admission(&receipt)?;
                require(
                    observed.admission_id == admitted.admission_id
                        && observed.session_id == admitted.session_id
                        && observed.profile == admitted.profile
                        && observed.authority_epoch == admitted.authority_epoch
                        && (observed.execution_generation == admitted.execution_generation
                            || (admitted.status == "queued"
                                && admitted.execution_generation.is_none())),
                    "exact native receipt mismatch",
                )?;
                public_admission(observed)?
            }
            WorkerCommand::Cancel { worker } => {
                let observed = parse_admission(&self.rpc.call("prompt.receipt",json!({"session_id":worker.session_id,"admission_id":worker.admission_id,"include_result":true}))?)?;
                require(
                    observed.admission_id == worker.admission_id
                        && observed.session_id == worker.session_id
                        && observed.profile == worker.profile
                        && (worker.execution_generation.is_none()
                            || observed.execution_generation == worker.execution_generation),
                    "cancel receipt belongs to another worker/generation",
                )?;
                public_admission(observed)?
            }
            WorkerCommand::Publish { .. } => {
                return Err(Error::Stop(
                    "publication inspection routing mismatch".into(),
                ));
            }
        };
        Ok(Observation::Confirmed {
            action: action.clone(),
            evidence,
        })
    }
}
impl<R: NativeRpc, H: NativeHost, P: PoolInspector, J: PublicationApi> Hermes
    for NativeWorker<R, H, P, J>
{
}
fn public_admission(mut receipt: Admission) -> Result<String> {
    // Authenticated issue history keeps exact result references, never raw terminal content.
    receipt.result = None;
    Ok(serde_json::to_string(&receipt)?)
}
impl<R: NativeRpc, H: NativeHost, P: PoolInspector, J: PublicationApi> WorkerRuntime
    for NativeWorker<R, H, P, J>
{
    fn prepare(&mut self, request: &WorkerRequest, url: &str) -> Result<LaunchEvidence> {
        let gateway: GatewayDescription =
            serde_json::from_value(self.rpc.call("runtime.describe", json!({}))?)?;
        gateway.validate(&request.profile)?;
        self.pool.verify(&request.pool)?;
        let evidence = self.host.prerequisites(request, &gateway, url)?;
        require(
            evidence.gateway == gateway,
            "host attestation is not bound to negotiated gateway",
        )?;
        evidence.validate(request)?;
        Ok(evidence)
    }
    fn observe(&mut self, worker: &WorkerIdentity) -> Result<Admission> {
        let observed = parse_admission(&self.rpc.call("prompt.receipt",json!({"session_id":worker.session_id,"admission_id":worker.admission_id,"include_result":true}))?)?;
        observed.validate(worker)?;
        Ok(observed)
    }
    fn boundary(&mut self, worker: &WorkerIdentity) -> Result<Boundary> {
        self.host.boundary(worker)
    }
    fn stopped_effects(&mut self, worker: &WorkerIdentity) -> Result<Option<String>> {
        self.host.stopped_effects(worker)
    }
}
