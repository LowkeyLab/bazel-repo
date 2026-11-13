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
struct Host(Runtime);
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
        Ok(None)
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
    let native = NativeWorker::new(wire.clone(), Host(runtime), Pool, Publications);
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
    let native = NativeWorker::new(wire.clone(), Host(runtime), Pool, Publications);
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
            json!({"schema_version":1,"outcome":"completed","command":"acquire","data":{"assignment":selected}}),
        ),
    ] {
        let path = original.join(name);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let command = dir.path().join("pool-receipt");
    let args = dir.path().join("args");
    std::fs::write(
        &command,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s' '{}'\n",
            args.display(),
            summary
        ),
    )
    .unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o700)).unwrap();
    let receipt = PoolReceipt::from_capture(original.clone(), &summary, selected).unwrap();
    let mut adapter =
        CapturedPoolInspector::new(command, "issue-2".into(), "/usr/bin:/bin".into()).unwrap();
    adapter.verify(&receipt).unwrap();
    let sent = std::fs::read_to_string(args).unwrap();
    expect_that!(
        sent.contains("assignment\ninspect\n11111111-1111-4111-8111-111111111111"),
        eq(true)
    );
    expect_that!(sent.contains("acquire"), eq(false));
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
        Host(runtime),
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
