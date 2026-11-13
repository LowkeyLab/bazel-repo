use std::{cell::RefCell, rc::Rc};

use googletest::{expect_that, gtest, matchers::eq};
use serde_json::{Value, json};
use software_factory::{
    Error, Result,
    controller::Controller,
    model::{Action, Identity},
    native_worker::{
        NativeHost, NativeRpc, NativeWorker, PoolInspector, PublicationApi, UnprovisionedHost,
        parse_admission,
    },
    worker::{
        Boundary, GatewayDescription, LaunchEvidence, PoolReceipt, PublicationRequest,
        PublicationResult, SessionEvidence, WorkerIdentity, WorkerPhase, WorkerRequest,
    },
    worker_io::{GitHubPublications, NativeRpcCommand},
};

use super::{Runtime, WorkerRuntime, controller_setup, description, request};

#[gtest]
fn native_admission_parser_preserves_generation_epoch_and_terminal_result() {
    let wire = json!({"admission_id":"a1","ref":{"session_id":"s1","profile_id":"worker"},"sequence":0,"status":"terminal","outcome":"completed","authority_epoch":9,"execution_generation":4,"result":{"final_response":"done"}});
    let receipt = parse_admission(&wire).unwrap();
    expect_that!(receipt.authority_epoch, eq(9));
    expect_that!(receipt.execution_generation, eq(Some(4)));
    expect_that!(
        receipt.result.unwrap()["final_response"].as_str(),
        eq(Some("done"))
    );
    let mut bad = wire;
    bad["ref"]["profile_id"] = Value::Null;
    expect_that!(parse_admission(&bad).is_err(), eq(true));
}

#[gtest]
fn cancelling_never_started_queued_admission_needs_no_execution_generation_or_result() {
    let wire = json!({"admission_id":"a1","ref":{"session_id":"s1","profile_id":"worker"},"sequence":0,"status":"terminal","outcome":"cancelled","authority_epoch":9,"execution_generation":null});
    let receipt = parse_admission(&wire).unwrap();
    let worker = WorkerIdentity {
        run: Identity::new(),
        session_id: "s1".into(),
        admission_id: "a1".into(),
        profile: "worker".into(),
        principal: "worker-owner".into(),
        execution_generation: None,
    };
    expect_that!(receipt.validate(&worker).is_ok(), eq(true));
    let mut started = wire;
    started["status"] = json!("started");
    started["outcome"] = Value::Null;
    expect_that!(parse_admission(&started).is_err(), eq(true));
}

#[derive(Clone, Default)]
struct Wire(Rc<RefCell<Vec<(String, Value)>>>);
impl NativeRpc for Wire {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.0.borrow_mut().push((method.into(), params));
        match method {
            "runtime.describe" => Ok(serde_json::to_value(description()).unwrap()),
            "session.create" => Ok(
                json!({"session_id":"session-1","stored_session_id":"session-1","messages":[],"message_count":0,"info":{"profile_id":"worker"}}),
            ),
            "session.info" => {
                let create = self
                    .0
                    .borrow()
                    .iter()
                    .find(|(method, _)| method == "session.create")
                    .unwrap()
                    .1
                    .clone();
                Ok(json!({"profile_id":"worker","cwd":"/w","launch_request":create}))
            }
            "prompt.submit" | "prompt.receipt" | "prompt.cancel" => Ok(
                json!({"admission_id":"admission-1","ref":{"session_id":"session-1","profile_id":"worker"},"sequence":0,"status":"started","outcome":null,"authority_epoch":7,"execution_generation":4}),
            ),
            _ => Err(Error::Stop(format!("invented RPC {method}"))),
        }
    }
}
struct Host(Runtime, Option<Value>);
impl NativeHost for Host {
    fn prerequisites(
        &mut self,
        r: &WorkerRequest,
        _: &GatewayDescription,
        url: &str,
    ) -> Result<LaunchEvidence> {
        self.0.prepare(r, url)
    }
    fn authorize(&mut self, _: &Action) -> Result<()> {
        Ok(())
    }
    fn frozen_session(&mut self, a: &Action, _: &Value) -> Result<SessionEvidence> {
        let mut loaded_skills = self.0.prepare(&request(), "fixture")?.skills;
        if self.0.0.borrow().frozen_partial {
            loaded_skills.pop();
        }
        Ok(SessionEvidence {
            session_id: "session-1".into(),
            stored_session_id: "session-1".into(),
            profile: "worker".into(),
            principal: "worker-owner".into(),
            creation_id: a.id.clone(),
            loaded_skills,
            frozen_binding: "supervisor:actual-frozen-content-1".into(),
        })
    }
    fn exact_reply(&mut self, _: &Action) -> Result<Option<Value>> {
        Ok(self.1.clone())
    }
    fn boundary(&mut self, w: &WorkerIdentity) -> Result<Boundary> {
        self.0.boundary(w)
    }
    fn stopped_effects(&mut self, w: &WorkerIdentity) -> Result<Option<String>> {
        self.0.stopped_effects(w)
    }
}
struct Pool;
impl PoolInspector for Pool {
    fn verify(&mut self, r: &PoolReceipt) -> Result<()> {
        r.validate()
    }
}
struct Publications;
impl PublicationApi for Publications {
    fn publish(&mut self, _: &PublicationRequest, _: &Identity) -> Result<()> {
        Err(Error::Stop("not provisioned".into()))
    }
    fn inspect(
        &mut self,
        _: &PublicationRequest,
        _: &Identity,
    ) -> Result<Option<PublicationResult>> {
        Ok(None)
    }
}
#[gtest]
fn controller_native_adapter_uses_only_flat_supported_rpc_and_explicit_skill_preload() {
    let (dir, c, history, runtime) = controller_setup();
    let (store, _, _) = c.into_parts();
    let wire = Wire::default();
    let native = NativeWorker::new(wire.clone(), Host(runtime, None), Pool, Publications);
    let mut c = Controller::new(store, history, native, "controller".into()).unwrap();
    expect_that!(
        c.execute_worker(request()).unwrap().phase,
        eq(&WorkerPhase::Running)
    );
    let calls = wire.0.borrow();
    let create = &calls.iter().find(|(m, _)| m == "session.create").unwrap().1;
    expect_that!(create["source"].as_str(), eq(Some("cli")));
    expect_that!(create["cwd"].as_str(), eq(Some("/w")));
    expect_that!(
        create["skills"] == json!(["implement", "tdd", "code-review"]),
        eq(true)
    );
    expect_that!(create.as_object().unwrap().len(), eq(4));
    let submit = &calls.iter().find(|(m, _)| m == "prompt.submit").unwrap().1;
    expect_that!(
        submit["text"]
            .as_str()
            .unwrap()
            .contains("https://github.com/LowkeyLab/bazel-repo/issues/2"),
        eq(true)
    );
    expect_that!(
        submit["text"].as_str().unwrap().starts_with("/implement"),
        eq(false)
    );
    drop(dir);
}
#[gtest]
fn native_rpc_command_sends_one_supported_packet_and_rejects_wrong_reply_identity() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("native-bridge");
    let input = dir.path().join("input");
    let args = dir.path().join("args");
    std::fs::write(&command,format!("#!/bin/sh\nprintf '%s' \"$*\" > '{}'\n/bin/cat > '{}'\nprintf '%s' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"pong\":true}}}}'\n",args.display(),input.display())).unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut rpc = NativeRpcCommand::new(command, "PATH".into()).unwrap();
    expect_that!(
        rpc.call("ping", json!({})).unwrap()["pong"] == true,
        eq(true)
    );
    expect_that!(std::fs::read_to_string(&args).unwrap().is_empty(), eq(true));
    let packet: Value = serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
    expect_that!(packet["method"].as_str(), eq(Some("ping")));
    expect_that!(packet["params"] == json!({}), eq(true));
    expect_that!(rpc.call("ping", json!({})).is_err(), eq(true));
}

#[gtest]
fn partial_actual_frozen_skill_loading_stops_before_submit_without_creating_again() {
    let (_dir, c, history, runtime) = controller_setup();
    let (store, _, _) = c.into_parts();
    runtime.0.borrow_mut().frozen_partial = true;
    let wire = Wire::default();
    let native = NativeWorker::new(wire.clone(), Host(runtime, None), Pool, Publications);
    let mut c = Controller::new(store, history, native, "controller".into()).unwrap();
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    expect_that!(
        wire.0
            .borrow()
            .iter()
            .filter(|(m, _)| m == "session.create")
            .count(),
        eq(1)
    );
    expect_that!(
        wire.0.borrow().iter().any(|(m, _)| m == "prompt.submit"),
        eq(false)
    );
}

#[gtest]
fn isolation_and_credential_attestations_cannot_be_missing_or_reuse_controller_uid() {
    let history = super::History::default();
    let mut runtime = Runtime::new(history);
    let valid = runtime.prepare(&request(), "fixture").unwrap();
    for failure in 0..4 {
        let mut evidence = valid.clone();
        match failure {
            0 => evidence.worker_uid = evidence.controller_uid,
            1 => evidence.worker_uid = 0,
            2 => evidence.isolation_receipt.clear(),
            _ => evidence.credential_receipt.clear(),
        }
        expect_that!(evidence.validate(&request()).is_err(), eq(true));
    }
}

#[derive(Clone, Default)]
struct PrWire(Rc<RefCell<Vec<PrRequest>>>);
struct PrRequest {
    endpoint: String,
    body: Option<Value>,
}
fn pr() -> Value {
    json!({"number":8,"node_id":"PR8","html_url":"https://github.com/LowkeyLab/bazel-repo/pull/8","body":"fixture-marker","state":"open","merged":false,"head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feat/worker-2","repo":{"full_name":"LowkeyLab/bazel-repo"}},"base":{"ref":"main","repo":{"full_name":"LowkeyLab/bazel-repo"}}})
}
impl software_factory::github::Transport for PrWire {
    fn request(&mut self, endpoint: &str, body: Option<Value>, _: bool) -> Result<Value> {
        self.0.borrow_mut().push(PrRequest {
            endpoint: endpoint.into(),
            body: body.clone(),
        });
        if endpoint.contains("git/ref/") {
            return Ok(
                json!({"object":{"type":"commit","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}),
            );
        }
        if let Some(body) = body {
            let mut result = pr();
            result["body"] = body["body"].clone();
            return Ok(result);
        }
        let posted = self.0.borrow().iter().find_map(|r| r.body.clone());
        if endpoint.ends_with("pulls/8") {
            let mut result = pr();
            result["body"] = posted.unwrap()["body"].clone();
            return Ok(result);
        }
        Ok(posted.map_or_else(
            || json!([]),
            |b| {
                let mut result = pr();
                result["body"] = b["body"].clone();
                json!([result])
            },
        ))
    }
}
#[gtest]
fn supported_github_publication_reads_exact_marker_branch_head_and_remote_result() {
    let wire = PrWire::default();
    let mut api = GitHubPublications::new(wire.clone(), 3).unwrap();
    let intent = Identity::new();
    let request = PublicationRequest {
        branch: "feat/worker-2".into(),
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        title: "Implement 2".into(),
        body: "Closes #2".into(),
    };
    api.publish(&request, &intent).unwrap();
    let result = api.inspect(&request, &intent).unwrap().unwrap();
    expect_that!(result.number, eq(8));
    expect_that!(result.intent == intent, eq(true));
    expect_that!(
        wire.0.borrow().iter().filter(|r| r.body.is_some()).count(),
        eq(1)
    );
    expect_that!(api.publish(&request, &intent).is_err(), eq(true));
    expect_that!(
        wire.0
            .borrow()
            .iter()
            .any(|r| r.endpoint.contains("git/ref/heads/feat%2Fworker-2")),
        eq(true)
    );
}

#[gtest]
fn pool_adapter_retains_acquisition_capture_and_only_inspects_exact_assignment() {
    use software_factory::worker_io::CapturedPoolInspector;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original");
    std::fs::create_dir(&original).unwrap();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).unwrap();
    let selected = serde_json::to_value(request().pool).unwrap()["selected"].clone();
    let summary = json!({"capture_complete":true,"wrapper_exit":0,"pool_exit":0,"pool_outcome":"completed","known_catalog_blocker":false,"receipt_directory":original});
    for (name, value) in [
        ("selected-assignment.json", selected.clone()),
        (
            "exit-status.json",
            json!({"started":true,"code":0,"signal":null}),
        ),
        (
            "invocation.json",
            json!({"version":1,"program":"worktree-pool","arguments":[{"bytes":[97,99,113,117,105,114,101]}]}),
        ),
        (
            "stdout",
            json!({"schema_version":1,"outcome":"completed","command":"acquire","context":{"catalog_id":"55555555-5555-4555-8555-555555555555"},"data":{"assignment":selected}}),
        ),
    ] {
        let path = original.join(name);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let receipt_root = dir.path().join("inspections");
    std::fs::create_dir(&receipt_root).unwrap();
    std::fs::set_permissions(&receipt_root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let catalog = dir.path().join("approved-catalog");
    std::fs::create_dir(&catalog).unwrap();
    let command = std::fs::canonicalize(std::env::var("POOL_WRAPPER").unwrap()).unwrap();
    let pool = dir.path().join("worktree-pool");
    let args = dir.path().join("args");
    std::fs::write(
        &pool,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n[ -z \"$HOME$XDG_STATE_HOME\" ] || exit 2\nprintf '%s' '{}'\n",
            args.display(),
            json!({"schema_version":1,"outcome":"completed","command":"assignment inspect","reason_code":"controlled","context":{"catalog_id":"55555555-5555-4555-8555-555555555555"},"data":{"assignment":selected}})
        ),
    )
    .unwrap();
    std::fs::set_permissions(&pool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let originals: Vec<_> = std::fs::read_dir(&original)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let receipt = PoolReceipt::from_capture(original.clone(), &summary, selected).unwrap();
    let mut adapter = CapturedPoolInspector::new(
        command,
        "issue-2".into(),
        format!("{}:/usr/bin:/bin", dir.path().display()),
        receipt_root.clone(),
        catalog.clone(),
    )
    .unwrap();
    adapter.verify(&receipt).unwrap_or_else(|error| {
        let captures: Vec<_> = std::fs::read_dir(&receipt_root)
            .unwrap()
            .map(|entry| {
                let directory = entry.unwrap().path();
                (
                    directory.clone(),
                    std::fs::read_to_string(directory.join("wrapper-error.json")),
                    std::fs::read_to_string(directory.join("stderr")),
                )
            })
            .collect();
        panic!("{error:?}; captures={captures:?}")
    });
    let sent = std::fs::read_to_string(args).unwrap();
    expect_that!(
        sent.contains("assignment\ninspect\n11111111-1111-4111-8111-111111111111"),
        eq(true)
    );
    expect_that!(sent.contains("acquire"), eq(false));
    expect_that!(
        sent,
        eq(&format!(
            "--json\n--catalog-dir\n{}\nassignment\ninspect\n11111111-1111-4111-8111-111111111111\n",
            catalog.display()
        ))
    );
    let captures: Vec<_> = std::fs::read_dir(&receipt_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    expect_that!(captures.len(), eq(1));
    expect_that!(
        std::fs::metadata(&captures[0])
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        eq(0o700)
    );
    for entry in std::fs::read_dir(&captures[0]).unwrap() {
        expect_that!(
            entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            eq(0o600)
        );
    }
    // Same assignment IDs in another catalog must not establish the original authority.
    let script = std::fs::read_to_string(&pool).unwrap();
    std::fs::write(
        &pool,
        script.replace(
            "55555555-5555-4555-8555-555555555555",
            "66666666-6666-4666-8666-666666666666",
        ),
    )
    .unwrap();
    expect_that!(adapter.verify(&receipt).is_err(), eq(true));
    for (path, bytes) in originals {
        expect_that!(std::fs::read(path).unwrap(), eq(&bytes));
    }
    std::fs::write(original.join("wrapper-error.json"), "{}").unwrap();
    expect_that!(adapter.verify(&receipt).is_err(), eq(true));
}

#[gtest]
fn terminal_result_stays_private_even_when_submit_finishes_before_ack_readback() {
    struct Immediate(Wire);
    impl NativeRpc for Immediate {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            let mut value = self.0.call(method, params)?;
            if matches!(method, "prompt.submit" | "prompt.receipt") {
                value["status"] = json!("terminal");
                value["outcome"] = json!("completed");
                value["result"] = json!({"final_response":"PRIVATE_TERMINAL_FIXTURE"});
            }
            Ok(value)
        }
    }
    let (_dir, c, history, runtime) = controller_setup();
    let (store, _, _) = c.into_parts();
    let native = NativeWorker::new(
        Immediate(Wire::default()),
        Host(runtime, None),
        Pool,
        Publications,
    );
    let mut c = Controller::new(store, history.clone(), native, "controller".into()).unwrap();
    let worker = c.execute_worker(request()).unwrap();
    expect_that!(worker.phase, eq(&WorkerPhase::Implemented));
    expect_that!(
        worker.admission.unwrap().result.unwrap()["final_response"].as_str(),
        eq(Some("PRIVATE_TERMINAL_FIXTURE"))
    );
    expect_that!(
        history
            .0
            .borrow()
            .iter()
            .any(|c| c.body.contains("PRIVATE_TERMINAL_FIXTURE")),
        eq(false)
    );
}

#[gtest]
fn submission_readback_rejects_changed_acknowledged_execution_generation() {
    inspect_submission("started", Some(4), "started", Some(4), 7).unwrap();
    expect_that!(
        inspect_submission("started", Some(4), "started", Some(5), 7).is_err(),
        eq(true)
    );
}

#[gtest]
fn queued_submission_can_start_but_unknown_submission_cannot_adopt_a_generation() {
    use software_factory::{controller::Observation, worker::Admission};
    let Observation::Confirmed { evidence, .. } =
        inspect_submission("queued", None, "started", Some(4), 7).unwrap()
    else {
        panic!("queued transition not confirmed")
    };
    let observed: Admission = serde_json::from_str(&evidence).unwrap();
    expect_that!(observed.execution_generation, eq(Some(4)));
    expect_that!(observed.authority_epoch, eq(7));
    expect_that!(
        inspect_submission("unknown", None, "started", Some(4), 7).is_err(),
        eq(true)
    );
    expect_that!(
        inspect_submission("started", Some(4), "queued", None, 7).is_err(),
        eq(true)
    );
    expect_that!(
        inspect_submission("started", Some(4), "started", Some(4), 8).is_err(),
        eq(true)
    );
}

fn inspect_submission(
    acknowledged_status: &str,
    acknowledged_generation: Option<u64>,
    observed_status: &str,
    observed_generation: Option<u64>,
    observed_epoch: u64,
) -> Result<software_factory::controller::Observation> {
    use software_factory::{controller::Effects, worker::WorkerCommand};
    struct Admissions {
        acknowledged: Value,
        observed: Value,
    }
    impl NativeRpc for Admissions {
        fn call(&mut self, method: &str, _: Value) -> Result<Value> {
            match method {
                "prompt.submit" => Ok(self.acknowledged.clone()),
                "prompt.receipt" => Ok(self.observed.clone()),
                _ => Err(Error::Stop("unexpected RPC".into())),
            }
        }
    }
    let receipt = |status: &str, generation: Option<u64>, epoch: u64| {
        json!({
            "admission_id":"admission-1", "ref":{"session_id":"session-1","profile_id":"worker"},
            "sequence":0, "status":status, "outcome":null,
            "authority_epoch":epoch, "execution_generation":generation
        })
    };
    let wire = Admissions {
        acknowledged: receipt(acknowledged_status, acknowledged_generation, 7),
        observed: receipt(observed_status, observed_generation, observed_epoch),
    };
    let (_dir, _controller, _history, runtime) = controller_setup();
    let mut native = NativeWorker::new(wire, Host(runtime, None), Pool, Publications);
    let id = Identity::new();
    let action = WorkerCommand::Submit {
        session_id: "session-1".into(),
        submission_id: id.clone(),
        text: "implement".into(),
    }
    .action(&id, &super::issue_key())
    .unwrap();
    native.send(&action).unwrap();
    native.inspect_exact(&action)
}

#[gtest]
fn native_cancel_ack_requires_separate_exact_cessation_and_reconciled_effects() {
    let (_dir, c, history, runtime) = controller_setup();
    let (store, _, _) = c.into_parts();
    let wire = Wire::default();
    let native = NativeWorker::new(
        wire.clone(),
        Host(runtime.clone(), None),
        Pool,
        Publications,
    );
    let mut c = Controller::new(store, history, native, "controller".into()).unwrap();
    c.execute_worker(request()).unwrap();
    let cancelling = c.cancel_worker("operator stop".into()).unwrap();
    expect_that!(cancelling.phase, eq(&WorkerPhase::Cancelling));
    expect_that!(cancelling.cessation.is_none(), eq(true));
    runtime.0.borrow_mut().stopped = true;
    expect_that!(
        c.cancel_worker("operator stop".into()).unwrap().phase,
        eq(&WorkerPhase::Cancelling)
    );
    runtime.0.borrow_mut().reconciled = true;
    let stopped = c.cancel_worker("operator stop".into()).unwrap();
    expect_that!(stopped.phase, eq(&WorkerPhase::Stopped));
    expect_that!(
        stopped.cessation.as_deref(),
        eq(Some("supervisor:exact-cessation-and-effects-1"))
    );
    let calls = wire.0.borrow();
    let cancellations: Vec<_> = calls.iter().filter(|(m, _)| m == "prompt.cancel").collect();
    expect_that!(cancellations.len(), eq(1));
    expect_that!(
        cancellations[0].1 == json!({"session_id":"session-1","admission_id":"admission-1"}),
        eq(true)
    );
    expect_that!(calls.iter().any(|(m, p)| m == "prompt.receipt" && p == &json!({"session_id":"session-1","admission_id":"admission-1","include_result":true})), eq(true));
}

#[gtest]
fn native_cancellation_readback_rejects_another_worker_or_execution_generation() {
    use software_factory::{
        controller::{Effects, Observation},
        worker::{Admission, WorkerCommand},
    };
    struct ReceiptWire(Wire, Value);
    impl NativeRpc for ReceiptWire {
        fn call(&mut self, method: &str, params: Value) -> Result<Value> {
            let reply = self.0.call(method, params)?;
            Ok(if method == "prompt.receipt" {
                self.1.clone()
            } else {
                reply
            })
        }
    }
    let worker = WorkerIdentity {
        run: Identity::new(),
        session_id: "session-1".into(),
        admission_id: "admission-1".into(),
        profile: "worker".into(),
        principal: "worker-owner".into(),
        execution_generation: Some(4),
    };
    let id = Identity::new();
    let action = WorkerCommand::Cancel { worker }
        .action(&id, &super::issue_key())
        .unwrap();
    let valid = json!({"admission_id":"admission-1","ref":{"session_id":"session-1","profile_id":"worker"},"sequence":0,"status":"terminal","outcome":"cancelled","authority_epoch":7,"execution_generation":4,"result":{"final_response":"PRIVATE_CANCEL_RESULT"}});
    for field in [
        "valid",
        "admission_id",
        "session_id",
        "profile_id",
        "execution_generation",
    ] {
        let mut receipt = valid.clone();
        match field {
            "admission_id" => receipt[field] = json!("another-admission"),
            "session_id" | "profile_id" => receipt["ref"][field] = json!("another-worker"),
            "execution_generation" => receipt[field] = json!(5),
            _ => {}
        }
        let wire = Wire::default();
        let runtime = Runtime::new(super::History::default());
        let mut native = NativeWorker::new(
            ReceiptWire(wire.clone(), receipt),
            Host(runtime, None),
            Pool,
            Publications,
        );
        native.send(&action).unwrap();
        let observed = native.inspect_exact(&action);
        if field == "valid" {
            let Observation::Confirmed {
                action: exact,
                evidence,
            } = observed.unwrap()
            else {
                panic!("cancellation not confirmed")
            };
            expect_that!(exact, eq(&action));
            let admitted: Admission = serde_json::from_str(&evidence).unwrap();
            expect_that!(admitted.execution_generation, eq(Some(4)));
            expect_that!(admitted.outcome.as_deref(), eq(Some("cancelled")));
            expect_that!(admitted.result.is_none(), eq(true));
            expect_that!(evidence.contains("PRIVATE_CANCEL_RESULT"), eq(false));
        } else {
            expect_that!(observed.is_err(), eq(true));
        }
        expect_that!(wire.0.borrow().len(), eq(2));
    }
}

#[gtest]
fn native_publication_routes_to_github_and_confirms_only_exact_marker_readback() {
    use software_factory::{
        controller::{Effects, Observation},
        worker::WorkerCommand,
    };
    let wire = Wire::default();
    let github = PrWire::default();
    let runtime = Runtime::new(super::History::default());
    let mut native = NativeWorker::new(
        wire.clone(),
        Host(runtime, None),
        Pool,
        GitHubPublications::new(github.clone(), 3).unwrap(),
    );
    let id = Identity::new();
    let request = PublicationRequest {
        branch: "feat/worker-2".into(),
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        title: "Implement 2".into(),
        body: "Closes #2".into(),
    };
    let action = WorkerCommand::Publish {
        request: request.clone(),
        intent: id.clone(),
    }
    .action(&id, &super::issue_key())
    .unwrap();
    expect_that!(
        matches!(native.inspect_exact(&action).unwrap(), Observation::Unknown),
        eq(true)
    );
    native.send(&action).unwrap();
    let Observation::Confirmed {
        action: exact,
        evidence,
    } = native.inspect_exact(&action).unwrap()
    else {
        panic!("publication not confirmed")
    };
    expect_that!(exact, eq(&action));
    let result: PublicationResult = serde_json::from_str(&evidence).unwrap();
    expect_that!(
        result,
        eq(&PublicationResult {
            number: 8,
            node_id: "PR8".into(),
            head: request.head,
            base: "main".into(),
            branch: request.branch,
            intent: id.clone(),
            url: "https://github.com/LowkeyLab/bazel-repo/pull/8".into()
        })
    );
    let calls = github.0.borrow();
    let writes: Vec<_> = calls.iter().filter_map(|r| r.body.as_ref()).collect();
    expect_that!(writes.len(), eq(1));
    expect_that!(
        writes[0]["body"].as_str().unwrap().contains(id.as_str()),
        eq(true)
    );
    expect_that!(
        calls
            .iter()
            .any(|r| r.endpoint.ends_with("pulls/8") && r.body.is_none()),
        eq(true)
    );
    expect_that!(wire.0.borrow().is_empty(), eq(true));
}

#[gtest]
fn restarted_native_adapter_requires_original_host_reply_without_resending() {
    use software_factory::{
        controller::{Effects, Observation},
        worker::{Admission, WorkerCommand},
    };
    let wire = Wire::default();
    let id = Identity::new();
    let action = WorkerCommand::Submit {
        session_id: "session-1".into(),
        submission_id: id.clone(),
        text: "implement".into(),
    }
    .action(&id, &super::issue_key())
    .unwrap();
    let runtime = Runtime::new(super::History::default());
    let mut unknown = NativeWorker::new(
        wire.clone(),
        Host(runtime.clone(), None),
        Pool,
        Publications,
    );
    expect_that!(
        matches!(
            unknown.inspect_exact(&action).unwrap(),
            Observation::Unknown
        ),
        eq(true)
    );
    expect_that!(wire.0.borrow().is_empty(), eq(true));
    let original = json!({"admission_id":"admission-1","ref":{"session_id":"session-1","profile_id":"worker"},"sequence":0,"status":"started","outcome":null,"authority_epoch":7,"execution_generation":4});
    let mut recovered = NativeWorker::new(
        wire.clone(),
        Host(runtime, Some(original)),
        Pool,
        Publications,
    );
    let Observation::Confirmed {
        action: exact,
        evidence,
    } = recovered.inspect_exact(&action).unwrap()
    else {
        panic!("original reply not recovered")
    };
    expect_that!(exact, eq(&action));
    let admitted: Admission = serde_json::from_str(&evidence).unwrap();
    expect_that!(admitted.admission_id.as_str(), eq("admission-1"));
    expect_that!(
        wire.0.borrow().as_slice(),
        eq(&[(
            "prompt.receipt".into(),
            json!({"session_id":"session-1","admission_id":"admission-1","include_result":true})
        )][..])
    );
}

#[gtest]
fn unprovisioned_host_rejects_every_effect_and_supervision_path_without_wire_send() {
    use software_factory::{controller::Effects, worker::WorkerCommand};
    let wire = Wire::default();
    let github = PrWire::default();
    let mut native = NativeWorker::new(
        wire.clone(),
        UnprovisionedHost,
        Pool,
        GitHubPublications::new(github.clone(), 3).unwrap(),
    );
    let id = Identity::new();
    let worker = WorkerIdentity {
        run: Identity::new(),
        session_id: "session-1".into(),
        admission_id: "admission-1".into(),
        profile: "worker".into(),
        principal: "worker-owner".into(),
        execution_generation: Some(4),
    };
    let commands = [
        WorkerCommand::Create {
            request_id: id.clone(),
            profile: "worker".into(),
            principal: "worker-owner".into(),
            cwd: "/w".into(),
            skills: request().mandatory_skills,
            launch_digest: "protected-launch".into(),
        },
        WorkerCommand::Submit {
            session_id: worker.session_id.clone(),
            submission_id: id.clone(),
            text: "implement".into(),
        },
        WorkerCommand::Cancel {
            worker: worker.clone(),
        },
        WorkerCommand::Publish {
            request: PublicationRequest {
                branch: "feat/worker-2".into(),
                head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                title: "Implement 2".into(),
                body: "Closes #2".into(),
            },
            intent: id.clone(),
        },
    ];
    for command in commands {
        let action = command.action(&id, &super::issue_key()).unwrap();
        expect_that!(native.send(&action).is_err(), eq(true));
        if !matches!(command, WorkerCommand::Publish { .. }) {
            expect_that!(native.inspect_exact(&action).is_err(), eq(true));
        }
        let mut host = UnprovisionedHost;
        expect_that!(host.authorize(&action).is_err(), eq(true));
        expect_that!(host.frozen_session(&action, &json!({})).is_err(), eq(true));
        expect_that!(host.exact_reply(&action).is_err(), eq(true));
    }
    expect_that!(native.boundary(&worker).is_err(), eq(true));
    expect_that!(native.stopped_effects(&worker).is_err(), eq(true));
    expect_that!(wire.0.borrow().is_empty(), eq(true));
    expect_that!(github.0.borrow().is_empty(), eq(true));
}

#[gtest]
fn stock_gateway_without_trusted_review_gate_cannot_admit() {
    let (_dir, c, history, _) = controller_setup();
    let (store, _, _) = c.into_parts();
    let wire = Wire::default();
    let native = NativeWorker::new(wire.clone(), UnprovisionedHost, Pool, Publications);
    let mut c = Controller::new(store, history, native, "controller".into()).unwrap();
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    expect_that!(
        wire.0.borrow().iter().any(|(m, _)| m == "session.create"),
        eq(false)
    );
}
