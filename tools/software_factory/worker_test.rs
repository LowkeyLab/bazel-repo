use std::{cell::RefCell, rc::Rc};

use googletest::{expect_that, gtest, matchers::eq};
use serde_json::json;

use software_factory::{
    Error, Result,
    controller::{Controller, Effects, GitHub, Observation},
    model::{Action, AppIdentity, Comment, Identity, Issue, IssueKey, PROJECT, Snapshot},
    store::{StoppedOwner, Store},
    worker::{
        Admission, Boundary, GatewayDescription, LaunchEvidence, PoolReceipt, ResolvedSkill,
        SessionEvidence, WorkerCommand, WorkerIdentity, WorkerPhase, WorkerRequest, WorkerRuntime,
    },
};

mod native_worker_test;
mod research_test;

#[gtest]
fn native_admission_requires_every_creation_capability() {
    let wire = json!({
        "instance_id":"gateway-1", "profile_id":"worker", "authority_epoch":7,
        "capabilities":["durable-admission-v1","local-cli-create-v1"],
        "session_create":{"sources":["cli"],"parameters":["request_id","source","cwd","skills"]}
    });
    let description: GatewayDescription = serde_json::from_value(wire.clone()).unwrap();
    expect_that!(description.validate("worker").is_ok(), eq(true));
    for missing in ["request_id", "cwd", "skills"] {
        let mut partial = wire.clone();
        partial["session_create"]["parameters"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != missing);
        let description: GatewayDescription = serde_json::from_value(partial).unwrap();
        expect_that!(description.validate("worker").is_err(), eq(true));
    }
    let description: GatewayDescription = serde_json::from_value(wire).unwrap();
    expect_that!(description.validate("controller").is_err(), eq(true));
}

#[gtest]
fn original_pool_receipt_preserves_unix_bytes_and_refuses_uncertainty() {
    let assignment = json!({"assignment_handle":"11111111-1111-4111-8111-111111111111",
        "operation_id":"22222222-2222-4222-8222-222222222222", "repository_id":"33333333-3333-4333-8333-333333333333",
        "worktree_id":"44444444-4444-4444-8444-444444444444", "state":"active",
        "path":{"encoding":"unix-bytes","bytes":[47,119,255],"display":"/wrong"}});
    let receipt = PoolReceipt::from_capture("/private/original".into(), &json!({"capture_complete":true,"wrapper_exit":0,"pool_exit":0,"pool_outcome":"completed","known_catalog_blocker":false}), assignment.clone()).unwrap();
    expect_that!(receipt.path_bytes(), eq(&[47u8, 119, 255][..]));
    expect_that!(receipt.gateway_cwd().is_err(), eq(true));
    for outcome in ["pending", "unknown", "rejected"] {
        expect_that!(PoolReceipt::from_capture("/private/original".into(), &json!({"capture_complete":true,"wrapper_exit":0,"pool_exit":0,"pool_outcome":outcome,"known_catalog_blocker":false}),assignment.clone()).is_err(),eq(true));
    }
}

fn issue_key() -> IssueKey {
    "LowkeyLab/bazel-repo#2".to_owned().try_into().unwrap()
}
fn fixture_app() -> AppIdentity {
    AppIdentity {
        app_id: "100".into(),
        actor_id: "200".into(),
        login: "factory[bot]".into(),
    }
}
#[derive(Clone, Default)]
struct History(
    Rc<RefCell<Vec<Comment>>>,
    Rc<RefCell<Option<Snapshot>>>,
    Rc<RefCell<Option<String>>>,
    Rc<RefCell<LabelState>>,
);
#[derive(Default)]
struct LabelState {
    fault: String,
    sends: usize,
}
impl GitHub for History {
    fn pause_for_human(&mut self, issue: &IssueKey) -> Result<()> {
        if !self
            .0
            .borrow()
            .iter()
            .any(|c| c.issue == *issue && c.body.contains("human-input-labels"))
        {
            return Err(Error::Stop("label mutation without intent".into()));
        }
        self.3.borrow_mut().sends += 1;
        let fault = self.3.borrow().fault.clone();
        if fault == "fail-before" {
            return Err(Error::Stop("label write failed".into()));
        }
        let mut snapshot = self.snapshot()?;
        let labels = &mut snapshot.issues.get_mut(issue).unwrap().labels;
        if fault != "partial" {
            labels.retain(|s| s != "ready-for-agent");
        }
        if !labels.iter().any(|s| s == "needs-info") {
            labels.push("needs-info".into());
        }
        *self.1.borrow_mut() = Some(snapshot);
        if ["partial", "lost"].contains(&fault.as_str()) {
            return Err(Error::Stop("label acknowledgement lost".into()));
        }
        Ok(())
    }
    fn human_pause_confirmed(&mut self, issue: &IssueKey) -> Result<bool> {
        if self.3.borrow().fault == "readback-fail" {
            return Err(Error::Stop("label readback failed".into()));
        }
        let snapshot = self.snapshot()?;
        let labels = &snapshot.issues[issue].labels;
        Ok(labels.iter().any(|s| s == "needs-info")
            && !labels.iter().any(|s| s == "ready-for-agent"))
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        if let Some(snapshot) = self.1.borrow().as_ref() {
            return Ok(snapshot.clone());
        }
        let key = issue_key();
        let issue = Issue {
            key: key.clone(),
            node: "I2".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            open: true,
            pull_request: false,
            labels: vec!["ready-for-agent".into()],
            assignees: vec![],
            blockers_closed: vec![],
            parent: None,
            children: vec![],
            execution_clear: true,
        };
        Ok(Snapshot {
            project: PROJECT.into(),
            complete: true,
            members: vec![key.clone()],
            issues: std::collections::BTreeMap::from([(key, issue)]),
        })
    }
    fn history(&mut self, _: &IssueKey) -> Result<Vec<Comment>> {
        Ok(self.0.borrow().clone())
    }
    fn publish(&mut self, k: &IssueKey, body: &str) -> Result<String> {
        let id = format!("C{}", self.0.borrow().len());
        self.0.borrow_mut().push(Comment {
            id: id.clone(),
            issue: k.clone(),
            app: Some(fixture_app()),
            body: body.into(),
        });
        let lose = self
            .2
            .borrow()
            .as_ref()
            .is_some_and(|marker| body.contains(marker));
        if lose {
            self.2.borrow_mut().take();
            return Err(Error::Stop("lost publication acknowledgement".into()));
        }
        Ok(id)
    }
    fn exact_comment(&mut self, _: &IssueKey, id: &str) -> Result<Comment> {
        self.0
            .borrow()
            .iter()
            .find(|v| v.id == id)
            .cloned()
            .ok_or_else(|| Error::Stop("missing exact comment".into()))
    }
}
fn pool() -> PoolReceipt {
    PoolReceipt::from_capture("/private/original".into(), &json!({"capture_complete":true,"wrapper_exit":0,"pool_exit":0,"pool_outcome":"completed","known_catalog_blocker":false}),json!({"assignment_handle":"11111111-1111-4111-8111-111111111111","operation_id":"22222222-2222-4222-8222-222222222222","repository_id":"33333333-3333-4333-8333-333333333333","worktree_id":"44444444-4444-4444-8444-444444444444","state":"active","path":{"encoding":"unix-bytes","bytes":[47,119]}})).unwrap()
}
fn request() -> WorkerRequest {
    WorkerRequest {
        profile: "worker".into(),
        principal: "worker-owner".into(),
        mandatory_skills: vec!["implement".into(), "tdd".into(), "code-review".into()],
        pool: pool(),
    }
}
fn description() -> GatewayDescription {
    serde_json::from_value(json!({"instance_id":"gateway-1","profile_id":"worker","authority_epoch":7,"capabilities":["durable-admission-v1","local-cli-create-v1"],"session_create":{"sources":["cli"],"parameters":["request_id","source","cwd","skills"]}})).unwrap()
}
#[derive(Clone)]
struct Runtime(Rc<RefCell<RuntimeState>>, History);
// Independent fault-injection switches; combinations exercise uncertain remote effects.
#[allow(clippy::struct_excessive_bools)]
struct RuntimeState {
    sent: Vec<Action>,
    observed: Vec<WorkerIdentity>,
    boundaries: Vec<WorkerIdentity>,
    lost: bool,
    lose_at: Option<usize>,
    absent: bool,
    missing_skill: bool,
    frozen_partial: bool,
    no_gate: bool,
    review_failed: bool,
    terminal: bool,
    research_result: Option<serde_json::Value>,
    handoff_observation: Option<Admission>,
    stopped: bool,
    reconciled: bool,
}
impl Runtime {
    fn new(history: History) -> Self {
        Self(
            Rc::new(RefCell::new(RuntimeState {
                sent: vec![],
                observed: vec![],
                boundaries: vec![],
                lost: false,
                lose_at: None,
                absent: false,
                missing_skill: false,
                frozen_partial: false,
                no_gate: false,
                review_failed: false,
                terminal: false,
                research_result: None,
                handoff_observation: None,
                stopped: false,
                reconciled: false,
            })),
            history,
        )
    }
}
impl Effects for Runtime {
    fn send(&mut self, a: &Action) -> Result<()> {
        // The observable remote history must already contain the exact action intent.
        if !self
            .1
            .0
            .borrow()
            .iter()
            .any(|c| c.body.contains(a.id.as_str()))
        {
            return Err(Error::Stop("send without authenticated intent".into()));
        }
        if matches!(serde_json::from_str::<WorkerCommand>(&a.payload)?, WorkerCommand::Submit {text, ..} if text.contains("explicitly preloaded ask-matt"))
            && !self.1.human_pause_confirmed(&a.issue)?
        {
            return Err(Error::Stop("handoff before verified waiting labels".into()));
        }
        let mut state = self.0.borrow_mut();
        state.sent.push(a.clone());
        if state.lost || state.lose_at == Some(state.sent.len()) {
            state.lost = false;
            state.lose_at = None;
            return Err(Error::Stop("lost native acknowledgement".into()));
        }
        Ok(())
    }
    fn inspect_exact(&mut self, a: &Action) -> Result<Observation> {
        let state = self.0.borrow();
        if state.absent {
            return Ok(Observation::Unknown);
        }
        if !state.sent.contains(a) {
            return Ok(Observation::Absent);
        }
        let command: WorkerCommand = serde_json::from_str(&a.payload)?;
        let evidence = match command {
            WorkerCommand::Create { skills, .. } => serde_json::to_string(&SessionEvidence {
                session_id: "session-1".into(),
                stored_session_id: "session-1".into(),
                profile: "worker".into(),
                principal: "worker-owner".into(),
                creation_id: a.id.clone(),
                loaded_skills: skills
                    .iter()
                    .map(|name| ResolvedSkill {
                        name: name.clone(),
                        content_digest: software_factory::history::digest(name),
                    })
                    .collect(),
                frozen_binding: "private-native-skills-receipt:1".into(),
            })?,
            WorkerCommand::Submit { text, .. }
                if text.contains("explicitly preloaded ask-matt") =>
            {
                let mut handoff = admission("started");
                handoff.admission_id = "handoff-1".into();
                handoff.execution_generation = Some(5);
                serde_json::to_string(state.handoff_observation.as_ref().unwrap_or(&handoff))?
            }
            WorkerCommand::Submit { .. } | WorkerCommand::Cancel { .. } => {
                serde_json::to_string(&admission("started"))?
            }
            WorkerCommand::Publish { request, .. } => {
                serde_json::to_string(&software_factory::worker::PublicationResult {
                    number: 8,
                    node_id: "PR8".into(),
                    head: request.head,
                    base: "main".into(),
                    branch: request.branch,
                    intent: a.id.clone(),
                    url: "https://github.com/LowkeyLab/bazel-repo/pull/8".into(),
                })?
            }
        };
        Ok(Observation::Confirmed {
            action: a.clone(),
            evidence,
        })
    }
}
fn admission(status: &str) -> Admission {
    Admission {
        admission_id: "admission-1".into(),
        session_id: "session-1".into(),
        profile: "worker".into(),
        authority_epoch: 7,
        execution_generation: Some(4),
        status: status.into(),
        outcome: (status == "terminal").then(|| "completed".into()),
        result: (status == "terminal").then(|| json!({"final_response":"implemented"})),
    }
}
impl WorkerRuntime for Runtime {
    fn prepare(&mut self, r: &WorkerRequest, _: &str) -> Result<LaunchEvidence> {
        use std::os::unix::fs::MetadataExt;
        let state = self.0.borrow();
        Ok(LaunchEvidence {
            gateway: description(),
            mandatory_skills: if self.1.snapshot()?.issues[&issue_key()]
                .labels
                .iter()
                .any(|s| s == "wayfinder:research")
            {
                vec!["research".into(), "ask-matt".into()]
            } else {
                vec!["implement".into(), "tdd".into(), "code-review".into()]
            },
            controller_uid: std::fs::metadata("/proc/self")?.uid(),
            worker_uid: 10001,
            isolation_receipt: "supervisor:isolation-1".into(),
            credential_receipt: "supervisor:credentials-1".into(),
            review_gate: if state.no_gate {
                String::new()
            } else {
                "native-sync-review-gate:1".into()
            },
            skills: r
                .mandatory_skills
                .iter()
                .filter(|name| {
                    !state.missing_skill || !["tdd", "research"].contains(&name.as_str())
                })
                .map(|name| ResolvedSkill {
                    name: name.clone(),
                    content_digest: software_factory::history::digest(name),
                })
                .collect(),
        })
    }
    fn observe(&mut self, worker: &WorkerIdentity) -> Result<Admission> {
        self.0.borrow_mut().observed.push(worker.clone());
        let state = self.0.borrow();
        if worker.admission_id == "handoff-1" {
            return Ok(state.handoff_observation.clone().unwrap_or_else(|| {
                let mut result = admission("started");
                result.admission_id = "handoff-1".into();
                result.execution_generation = Some(5);
                result
            }));
        }
        let mut result = admission(if state.terminal {
            "terminal"
        } else {
            "started"
        });
        if state.terminal && state.research_result.is_some() {
            result.result.clone_from(&state.research_result);
        }
        Ok(result)
    }
    fn boundary(&mut self, worker: &WorkerIdentity) -> Result<Boundary> {
        self.0.borrow_mut().boundaries.push(worker.clone());
        Ok(if self.0.borrow().review_failed {
            Boundary::ReviewFailed {
                gate: "native-sync-review-gate:1".into(),
                findings: "independent-review:findings-1".into(),
            }
        } else {
            Boundary::Clear
        })
    }
    fn stopped_effects(&mut self, _: &WorkerIdentity) -> Result<Option<String>> {
        let s = self.0.borrow();
        Ok((s.stopped && s.reconciled).then(|| "supervisor:exact-cessation-and-effects-1".into()))
    }
}
struct Handoff;
impl StoppedOwner for Handoff {
    fn verify_stopped(&mut self, _: &Identity, _: &Identity) -> Result<String> {
        Ok("supervisor:controller-stopped".into())
    }
}
fn controller_setup() -> (
    tempfile::TempDir,
    Controller<History, Runtime>,
    History,
    Runtime,
) {
    let dir = tempfile::tempdir().unwrap();
    let history = History::default();
    let native = Runtime::new(history.clone());
    let store =
        Store::initialize(&dir.path().join("private"), fixture_app(), Identity::new()).unwrap();
    let mut c =
        Controller::new(store, history.clone(), native.clone(), "controller".into()).unwrap();
    c.poll().unwrap();
    (dir, c, history, native)
}
#[gtest]
fn observed_worker_job_contradiction_remains_stopped_after_field_restoration() {
    let (dir, mut c, _, _) = controller_setup();
    c.execute_worker(request()).unwrap();
    let path = dir.path().join("private/state.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let original_job = value["worker"]["plan"]["job"].clone();
    value["worker"]["plan"]["job"] = json!(Identity::new());
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    expect_that!(c.poll().is_err(), eq(true));
    let mut stopped: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stopped["worker"]["plan"]["job"] = original_job;
    std::fs::write(&path, serde_json::to_vec(&stopped).unwrap()).unwrap();
    expect_that!(c.poll().is_err(), eq(true));
}

#[gtest]
fn private_worker_receipt_cannot_disagree_with_authenticated_history() {
    let (dir, mut c, _, _) = controller_setup();
    c.execute_worker(request()).unwrap();
    let path = dir.path().join("private/state.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["worker"]["plan"]["request"]["principal"] = json!("forged-owner");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    expect_that!(c.poll().is_err(), eq(true));
}

#[gtest]
fn one_worker_uses_frozen_skills_authenticated_intents_and_retains_slot() {
    let (_dir, mut c, _, runtime) = controller_setup();
    let result = c.execute_worker(request()).unwrap();
    expect_that!(result.phase, eq(&WorkerPhase::Running));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    c.execute_worker(request()).unwrap();
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    runtime.0.borrow_mut().terminal = true;
    expect_that!(
        c.execute_worker(request()).unwrap().phase,
        eq(&WorkerPhase::Implemented)
    );
    expect_that!(
        matches!(
            c.poll().unwrap(),
            software_factory::controller::Decision::Retained(_)
        ),
        eq(true)
    );
}

#[gtest]
fn executing_retained_worker_observes_exact_identity_without_new_admission() {
    let (dir, mut c, mut history, runtime) = controller_setup();
    let running = c.execute_worker(request()).unwrap();
    let identity = running.identity().unwrap();
    let mut snapshot = history.snapshot().unwrap();
    snapshot
        .issues
        .get_mut(&issue_key())
        .unwrap()
        .execution_clear = false;
    *history.1.borrow_mut() = Some(snapshot);
    let before = runtime.0.borrow().observed.len();
    expect_that!(c.execute_worker(request()).unwrap(), eq(&running));
    expect_that!(runtime.0.borrow().observed.len(), eq(before + 1));
    expect_that!(runtime.0.borrow().observed.last(), eq(Some(&identity)));
    expect_that!(runtime.0.borrow().boundaries.last(), eq(Some(&identity)));
    expect_that!(c.prepare_admission().is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c =
        Controller::new(store, history.clone(), runtime.clone(), "controller".into()).unwrap();
    runtime.0.borrow_mut().review_failed = true;
    let blocked = c.execute_worker(request()).unwrap();
    expect_that!(blocked.phase, eq(&WorkerPhase::RepairBlocked));
    expect_that!(blocked.identity().unwrap(), eq(&identity));
    runtime.0.borrow_mut().review_failed = false;
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    expect_that!(c.execute_worker(request()).unwrap(), eq(&blocked));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    let (store, _, _) = c.into_parts();
    expect_that!(
        store.load().unwrap().active.unwrap().issue,
        eq(&issue_key())
    );
}

#[gtest]
fn retained_observation_keeps_all_other_authorization_checks() {
    for change in 0..6 {
        let (_dir, mut c, mut history, runtime) = controller_setup();
        c.execute_worker(request()).unwrap();
        let observed = runtime.0.borrow().observed.len();
        let mut snapshot = history.snapshot().unwrap();
        let issue = snapshot.issues.get_mut(&issue_key()).unwrap();
        issue.execution_clear = false;
        match change {
            0 => issue.assignees.push("another-owner".into()),
            1 => issue.labels.push("blocked".into()),
            2 => issue.blockers_closed.push(false),
            3 => issue.open = false,
            4 => snapshot.members.clear(),
            _ => snapshot.complete = false,
        }
        *history.1.borrow_mut() = Some(snapshot);
        expect_that!(c.execute_worker(request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().observed.len(), eq(observed));
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
        let (store, _, _) = c.into_parts();
        expect_that!(
            store.load().unwrap().active.unwrap().issue,
            eq(&issue_key())
        );
    }
}

#[gtest]
fn occupied_inventory_cannot_replace_retained_worker_or_publish_a_pr() {
    let (_dir, mut c, mut history, runtime) = controller_setup();
    let running = c.execute_worker(request()).unwrap();
    let mut snapshot = history.snapshot().unwrap();
    snapshot
        .issues
        .get_mut(&issue_key())
        .unwrap()
        .execution_clear = false;
    let other: IssueKey = "LowkeyLab/bazel-repo#3".to_owned().try_into().unwrap();
    let mut unrelated = snapshot.issues[&issue_key()].clone();
    unrelated.key = other.clone();
    unrelated.node = "I3".into();
    unrelated.execution_clear = true;
    snapshot.members.push(other.clone());
    snapshot.issues.insert(other, unrelated);
    *history.1.borrow_mut() = Some(snapshot);
    let mut changed = request();
    changed.principal = "replacement-owner".into();
    expect_that!(c.execute_worker(changed).is_err(), eq(true));
    runtime.0.borrow_mut().terminal = true;
    let completed = c.execute_worker(request()).unwrap();
    expect_that!(completed.phase, eq(&WorkerPhase::Implemented));
    expect_that!(completed.plan, eq(&running.plan));
    expect_that!(
        c.publish_worker_pr(software_factory::worker::PublicationRequest {
            branch: "feat/worker-2".into(),
            head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            title: "Implement issue 2".into(),
            body: "Closes #2".into(),
        })
        .is_err(),
        eq(true)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    let (store, _, _) = c.into_parts();
    expect_that!(
        store.load().unwrap().active.unwrap().issue,
        eq(&issue_key())
    );
}

#[gtest]
fn exact_cancellation_remains_available_with_occupied_inventory() {
    let (_dir, mut c, mut history, runtime) = controller_setup();
    let running = c.execute_worker(request()).unwrap();
    let mut snapshot = history.snapshot().unwrap();
    snapshot
        .issues
        .get_mut(&issue_key())
        .unwrap()
        .execution_clear = false;
    *history.1.borrow_mut() = Some(snapshot);
    let cancelling = c.cancel_worker("operator cancellation".into()).unwrap();
    expect_that!(cancelling.phase, eq(&WorkerPhase::Cancelling));
    expect_that!(
        cancelling.identity().unwrap(),
        eq(&running.identity().unwrap())
    );
    runtime.0.borrow_mut().stopped = true;
    runtime.0.borrow_mut().reconciled = true;
    expect_that!(
        c.cancel_worker("operator cancellation".into())
            .unwrap()
            .phase,
        eq(&WorkerPhase::Stopped)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
}

#[gtest]
fn occupied_inventory_blocks_new_create_and_submit_in_retained_early_phases() {
    for lost_at in [None, Some(1), Some(2)] {
        let (_dir, mut c, mut history, runtime) = controller_setup();
        if let Some(sent) = lost_at {
            runtime.0.borrow_mut().lose_at = Some(sent);
            expect_that!(c.execute_worker(request()).is_err(), eq(true));
        }
        let sent = runtime.0.borrow().sent.len();
        let mut snapshot = history.snapshot().unwrap();
        snapshot
            .issues
            .get_mut(&issue_key())
            .unwrap()
            .execution_clear = false;
        *history.1.borrow_mut() = Some(snapshot);
        expect_that!(c.execute_worker(request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(sent));
        let (store, _, _) = c.into_parts();
        let state = store.load().unwrap();
        expect_that!(state.active.unwrap().issue, eq(&issue_key()));
        expect_that!(
            state.worker.map(|worker| worker.phase),
            eq(&lost_at.map(|sent| {
                if sent == 1 {
                    WorkerPhase::Prepared
                } else {
                    WorkerPhase::Created
                }
            }))
        );
    }
}

#[gtest]
fn lost_create_acknowledgement_restarts_exact_worker_without_resend() {
    let (dir, mut c, history, runtime) = controller_setup();
    runtime.0.borrow_mut().lost = true;
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    let before = store.load().unwrap().worker.unwrap();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    let resumed = c.execute_worker(request()).unwrap();
    expect_that!(resumed.plan.run == before.plan.run, eq(true));
    expect_that!(resumed.plan.create == before.plan.create, eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn lost_submit_ack_and_unknown_result_never_repeat_or_release_the_slot() {
    let (dir, mut c, history, runtime) = controller_setup();
    runtime.0.borrow_mut().lose_at = Some(2);
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    runtime.0.borrow_mut().absent = true;
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    expect_that!(c.poll().is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    runtime.0.borrow_mut().absent = false;
    expect_that!(
        c.execute_worker(request()).unwrap().phase,
        eq(&WorkerPhase::Running)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn pr_publication_has_its_own_intent_exact_remote_result_and_restart_recovery() {
    let (dir, mut c, history, runtime) = controller_setup();
    let publication = software_factory::worker::PublicationRequest {
        branch: "feat/worker-2".into(),
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        title: "Implement issue 2".into(),
        body: "Closes #2".into(),
    };
    c.execute_worker(request()).unwrap();
    expect_that!(c.publish_worker_pr(publication.clone()).is_err(), eq(true));
    runtime.0.borrow_mut().terminal = true;
    expect_that!(
        c.execute_worker(request()).unwrap().publication.is_none(),
        eq(true)
    );
    runtime.0.borrow_mut().lose_at = Some(3);
    expect_that!(c.publish_worker_pr(publication.clone()).is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    let result = c.publish_worker_pr(publication).unwrap();
    expect_that!(result.phase, eq(&WorkerPhase::ReviewCiWait));
    expect_that!(result.publication.unwrap().number, eq(8));
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    expect_that!(
        matches!(
            c.poll().unwrap(),
            software_factory::controller::Decision::Retained(_)
        ),
        eq(true)
    );
}

impl software_factory::controller::Hermes for Runtime {}
impl software_factory::controller::Pool for Runtime {}
impl software_factory::controller::Notifications for Runtime {}

#[gtest]
fn worker_runtime_composes_with_existing_separate_pool_and_notification_routes() {
    let (_dir, c, history, runtime) = controller_setup();
    let (store, _, _) = c.into_parts();
    let notifications = Runtime::new(history.clone());
    let pool = Runtime::new(history.clone());
    let effects = software_factory::controller::NativeEffects {
        hermes: runtime.clone(),
        pool: pool.clone(),
        notifications: notifications.clone(),
    };
    let mut c = Controller::new(store, history, effects, "controller".into()).unwrap();
    c.execute_worker(request()).unwrap();
    let mut notice = runtime.0.borrow().sent[1].clone();
    notice.id = Identity::new();
    notice.kind = software_factory::model::EffectKind::Notification;
    notice.target = "originating-slack:fixture".into();
    c.perform(&notice).unwrap();
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    expect_that!(pool.0.borrow().sent.len(), eq(0));
    expect_that!(notifications.0.borrow().sent.len(), eq(1));
}

#[gtest]
fn caller_cannot_omit_skills_from_trusted_host_mandatory_manifest() {
    let (_dir, mut c, _, runtime) = controller_setup();
    let mut partial = request();
    partial.mandatory_skills = vec!["implement".into()];
    expect_that!(c.execute_worker(partial).is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.is_empty(), eq(true));
}

#[gtest]
fn missing_skill_or_synchronous_gate_stops_before_create_and_preserves_slot() {
    for no_gate in [false, true] {
        let (_dir, mut c, _, runtime) = controller_setup();
        runtime.0.borrow_mut().no_gate = no_gate;
        runtime.0.borrow_mut().missing_skill = !no_gate;
        expect_that!(c.execute_worker(request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.is_empty(), eq(true));
        expect_that!(
            matches!(
                c.poll().unwrap(),
                software_factory::controller::Decision::Retained(_)
            ),
            eq(true)
        );
    }
}

#[gtest]
fn internal_review_failure_is_a_durable_repair_stop_not_an_automatic_loop() {
    let (dir, mut c, history, runtime) = controller_setup();
    runtime.0.borrow_mut().review_failed = true;
    expect_that!(
        c.execute_worker(request()).unwrap().phase,
        eq(&WorkerPhase::RepairBlocked)
    );
    runtime.0.borrow_mut().review_failed = false;
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    expect_that!(
        c.execute_worker(request()).unwrap().phase,
        eq(&WorkerPhase::RepairBlocked)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn cancellation_ack_is_not_cessation_and_effects_must_be_reconciled() {
    let (_dir, mut c, _, runtime) = controller_setup();
    c.execute_worker(request()).unwrap();
    expect_that!(
        c.cancel_worker("operator cancellation".into())
            .unwrap()
            .phase,
        eq(&WorkerPhase::Cancelling)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    runtime.0.borrow_mut().stopped = true;
    expect_that!(
        c.cancel_worker("operator cancellation".into())
            .unwrap()
            .phase,
        eq(&WorkerPhase::Cancelling)
    );
    runtime.0.borrow_mut().reconciled = true;
    expect_that!(
        c.cancel_worker("operator cancellation".into())
            .unwrap()
            .phase,
        eq(&WorkerPhase::Stopped)
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    expect_that!(
        matches!(
            c.poll().unwrap(),
            software_factory::controller::Decision::Retained(_)
        ),
        eq(true)
    );
}
