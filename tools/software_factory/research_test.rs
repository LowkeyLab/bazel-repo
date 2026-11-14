use googletest::{expect_that, gtest, matchers::eq};
use serde_json::json;
use software_factory::{
    controller::{Controller, GitHub},
    model::Identity,
    store::Store,
    worker::{WorkerCommand, WorkerPhase, WorkerRequest},
};

use super::{Handoff, History, Runtime, fixture_app, issue_key, request};

pub fn research_request() -> WorkerRequest {
    WorkerRequest {
        mandatory_skills: vec!["research".into(), "ask-matt".into()],
        ..request()
    }
}
pub fn research_setup() -> (
    tempfile::TempDir,
    Controller<History, Runtime>,
    History,
    Runtime,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut history = History::default();
    let mut snapshot = history.snapshot().unwrap();
    snapshot.issues.get_mut(&issue_key()).unwrap().labels = vec!["wayfinder:research".into()];
    *history.1.borrow_mut() = Some(snapshot);
    let runtime = Runtime::new(history.clone());
    let store =
        Store::initialize(&dir.path().join("private"), fixture_app(), Identity::new()).unwrap();
    let mut controller =
        Controller::new(store, history.clone(), runtime.clone(), "controller".into()).unwrap();
    controller.poll().unwrap();
    (dir, controller, history, runtime)
}

pub fn findings() -> serde_json::Value {
    json!({"findings":"# Findings\nThe supported contract requires explicit skills [source](https://example.test/primary).", "evidence":["https://example.test/primary"], "human_question":null})
}

#[gtest]
fn findings_are_verified_on_originating_issue_before_research_completion() {
    let (_dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(findings());
    let completed = c.execute_worker(research_request()).unwrap();
    expect_that!(
        serde_json::to_value(&completed.phase).unwrap(),
        eq(&json!("ResearchComplete"))
    );
    expect_that!(completed.publication.is_none(), eq(true));
    let records = history.0.borrow();
    expect_that!(
        records
            .iter()
            .filter(|c| c.body.contains("ResearchFindings"))
            .count(),
        eq(1)
    );
    expect_that!(records.iter().all(|c| c.issue == issue_key()), eq(true));
    drop(records);
    let count = history.0.borrow().len();
    c.execute_worker(research_request()).unwrap();
    expect_that!(history.0.borrow().len(), eq(count));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn unanswered_decision_invokes_loaded_ask_matt_without_accepting_a_worker_answer() {
    let (dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Which deployment authority do you approve?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    let waiting = c.execute_worker(research_request()).unwrap();
    expect_that!(
        serde_json::to_value(&waiting.phase).unwrap(),
        eq(&json!("HumanInputRequired"))
    );
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    expect_that!(
        waiting.identity().unwrap().admission_id.as_str(),
        eq("handoff-1")
    );
    expect_that!(
        waiting.identity().unwrap().execution_generation,
        eq(Some(5))
    );
    let command: WorkerCommand = serde_json::from_str(&runtime.0.borrow().sent[2].payload).unwrap();
    let WorkerCommand::Submit { text, .. } = command else {
        panic!("expected handoff")
    };
    expect_that!(
        text.contains("ask-matt") && text.contains("unanswered"),
        eq(true)
    );
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    runtime.0.borrow_mut().research_result = Some(json!({"human_answer":"approved"}));
    expect_that!(c.execute_worker(research_request()).unwrap(), eq(&waiting));
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
}

#[gtest]
fn research_rejects_generic_implementation_pr_and_merge_effects() {
    let (_dir, mut c, _, runtime) = research_setup();
    let running = c.execute_worker(research_request()).unwrap();
    let publish = WorkerCommand::Publish {
        request: software_factory::worker::PublicationRequest {
            branch: "feat/forbidden".into(),
            head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            title: "unauthorized implementation".into(),
            body: String::new(),
        },
        intent: running.plan.publication.clone(),
    }
    .action(&running.plan.publication, &issue_key())
    .unwrap();
    expect_that!(c.perform(&publish).is_err(), eq(true));
    let mut merge = publish.clone();
    merge.id = Identity::new();
    merge.kind = software_factory::model::EffectKind::Merge;
    expect_that!(c.perform(&merge).is_err(), eq(true));
    let submit = WorkerCommand::Submit {
        session_id: "session-1".into(),
        submission_id: Identity::new(),
        text: "Implement and merge instead".into(),
    };
    let action = submit.action(&Identity::new(), &issue_key()).unwrap();
    expect_that!(c.perform(&action).is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn interrupted_findings_and_confirmation_publication_reconcile_exact_comments_after_restart() {
    for marker in ["ResearchFindings", "ResearchPublished"] {
        let (dir, mut c, mut history, runtime) = research_setup();
        let running = c.execute_worker(research_request()).unwrap();
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(findings());
        *history.2.borrow_mut() = Some(marker.into());
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        let (store, _, _) = c.into_parts();
        drop(store);
        let store =
            Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
        let mut c =
            Controller::new(store, history.clone(), runtime.clone(), "controller".into()).unwrap();
        let mut snapshot = history.snapshot().unwrap();
        snapshot
            .issues
            .get_mut(&issue_key())
            .unwrap()
            .labels
            .push("needs-info".into());
        *history.1.borrow_mut() = Some(snapshot);
        let completed = c.execute_worker(research_request()).unwrap();
        expect_that!(completed.plan, eq(&running.plan));
        expect_that!(completed.phase, eq(&WorkerPhase::ResearchComplete));
        expect_that!(
            history
                .0
                .borrow()
                .iter()
                .filter(|c| c.body.contains("ResearchFindings"))
                .count(),
            eq(1)
        );
        expect_that!(
            history
                .0
                .borrow()
                .iter()
                .filter(|c| c.body.contains("ResearchPublished"))
                .count(),
            eq(1)
        );
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
        expect_that!(c.poll().is_err(), eq(true));
    }
}

#[gtest]
fn research_waiting_labels_and_missing_required_skills_never_admit() {
    for label in [
        "needs-info",
        "needs-triage",
        "ready-for-human",
        "blocked",
        "waiting",
        "wayfinder:grilling",
    ] {
        let (_dir, mut c, mut history, runtime) = research_setup();
        let mut snapshot = history.snapshot().unwrap();
        snapshot
            .issues
            .get_mut(&issue_key())
            .unwrap()
            .labels
            .extend([label.into(), "ready-for-agent".into()]);
        *history.1.borrow_mut() = Some(snapshot);
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(0));
    }
    for skill in ["research", "ask-matt"] {
        let (_dir, mut c, _, runtime) = research_setup();
        let mut partial = research_request();
        partial.mandatory_skills.retain(|s| s != skill);
        expect_that!(c.execute_worker(partial).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(0));
    }
}

#[gtest]
fn research_rejects_implementation_and_fabricated_answer_results_durably() {
    for key in ["implementation_pr", "human_answer", "merge_authority"] {
        let (dir, mut c, history, runtime) = research_setup();
        c.execute_worker(research_request()).unwrap();
        let mut result = findings();
        result[key] = json!("agent supplied");
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(result);
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(
            history
                .0
                .borrow()
                .iter()
                .any(|c| c.body.contains("ResearchFindings")),
            eq(false)
        );
        let (store, _, _) = c.into_parts();
        drop(store);
        runtime.0.borrow_mut().research_result = Some(findings());
        let store =
            Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
        let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
    }
}

#[gtest]
fn interrupted_ask_matt_handoff_resumes_only_original_unanswered_handoff() {
    let (dir, mut c, mut history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Who authorizes activation?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    runtime.0.borrow_mut().lose_at = Some(3);
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    let handoff = runtime.0.borrow().sent[2].clone();
    let (store, _, _) = c.into_parts();
    drop(store);
    let mut snapshot = history.snapshot().unwrap();
    snapshot
        .issues
        .get_mut(&issue_key())
        .unwrap()
        .labels
        .push("needs-info".into());
    *history.1.borrow_mut() = Some(snapshot);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    runtime.0.borrow_mut().absent = true;
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    runtime.0.borrow_mut().absent = false;
    let waiting = c.execute_worker(research_request()).unwrap();
    expect_that!(waiting.phase, eq(&WorkerPhase::HumanInputRequired));
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    expect_that!(runtime.0.borrow().sent.last(), eq(Some(&handoff)));
    expect_that!(c.prepare_admission().is_err(), eq(true));
}

#[gtest]
fn missing_resolved_research_skill_stops_before_any_native_send() {
    let (_dir, mut c, _, runtime) = research_setup();
    runtime.0.borrow_mut().missing_skill = true;
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(0));
    runtime.0.borrow_mut().missing_skill = false;
    expect_that!(
        c.execute_worker(research_request()).unwrap().phase,
        eq(&WorkerPhase::Running)
    );
}

#[gtest]
fn research_lost_create_and_submit_acknowledgements_retain_original_run_and_pool() {
    for lost_at in [1, 2] {
        let (dir, mut c, history, runtime) = research_setup();
        runtime.0.borrow_mut().lose_at = Some(lost_at);
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        let (store, _, _) = c.into_parts();
        let retained = store.load().unwrap().worker.unwrap();
        drop(store);
        let store =
            Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
        let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
        let running = c.execute_worker(research_request()).unwrap();
        expect_that!(running.plan, eq(&retained.plan));
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
    }
}

#[gtest]
fn absent_uncertain_findings_never_republish_or_complete_until_exact_record_returns() {
    let (dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(findings());
    *history.2.borrow_mut() = Some("ResearchFindings".into());
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    let original = history.0.borrow_mut().pop().unwrap();
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c =
        Controller::new(store, history.clone(), runtime.clone(), "controller".into()).unwrap();
    let count = history.0.borrow().len();
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    expect_that!(history.0.borrow().len(), eq(count));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    history.0.borrow_mut().push(original);
    expect_that!(
        c.execute_worker(research_request()).unwrap().phase,
        eq(&WorkerPhase::ResearchComplete)
    );
}

#[gtest]
fn unanswered_handoff_cancellation_uses_its_own_exact_admission_generation() {
    let (_dir, mut c, _, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Which option?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    let waiting = c.execute_worker(research_request()).unwrap();
    let cancelling = c.cancel_worker("operator cancellation".into()).unwrap();
    expect_that!(cancelling.phase, eq(&WorkerPhase::Cancelling));
    let last = runtime.0.borrow().sent.last().unwrap().clone();
    let WorkerCommand::Cancel { worker } = serde_json::from_str(&last.payload).unwrap() else {
        panic!("expected cancellation")
    };
    expect_that!(worker, eq(&waiting.identity().unwrap()));
    expect_that!(worker.admission_id.as_str(), eq("handoff-1"));
}

#[gtest]
fn incomplete_findings_cannot_complete_or_publish() {
    for result in [
        json!({"findings":"", "evidence":["source"]}),
        json!({"findings":"Uncited", "evidence":[]}),
        json!({"findings":"Some finding", "evidence":["source"], "human_question":" "}),
    ] {
        let (_dir, mut c, history, runtime) = research_setup();
        c.execute_worker(research_request()).unwrap();
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(result);
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(
            history
                .0
                .borrow()
                .iter()
                .any(|c| c.body.contains("ResearchFindings")),
            eq(false)
        );
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
    }
}

#[gtest]
fn research_uses_native_admission_without_implementation_authority() {
    let (_dir, mut c, _, runtime) = research_setup();
    let running = c.execute_worker(research_request()).unwrap();
    expect_that!(running.phase, eq(&WorkerPhase::Running));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
    let command: WorkerCommand = serde_json::from_str(&runtime.0.borrow().sent[1].payload).unwrap();
    let WorkerCommand::Submit { text, .. } = command else {
        panic!("expected submit")
    };
    expect_that!(text.contains("research"), eq(true));
    expect_that!(text.contains("Do not implement"), eq(true));
    expect_that!(c.execute_worker(request()).is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(2));
}

#[gtest]
fn lost_handoff_ack_restart_cancellation_targets_handoff_without_execute() {
    let (dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Which authority?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    runtime.0.borrow_mut().lose_at = Some(3);
    expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    runtime.0.borrow_mut().absent = true;
    expect_that!(c.cancel_worker("operator stop".into()).is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
    runtime.0.borrow_mut().absent = false;
    let cancelled = c.cancel_worker("operator stop".into()).unwrap();
    expect_that!(
        cancelled.identity().unwrap().admission_id.as_str(),
        eq("handoff-1")
    );
    let last = runtime.0.borrow().sent.last().unwrap().clone();
    let WorkerCommand::Cancel { worker } = serde_json::from_str(&last.payload).unwrap() else {
        panic!("expected cancellation")
    };
    expect_that!(worker.admission_id.as_str(), eq("handoff-1"));
    expect_that!(worker.execution_generation, eq(Some(5)));
}

#[gtest]
fn human_question_sets_waiting_labels_before_handoff_and_preserves_other_labels() {
    let (_dir, mut c, mut history, runtime) = research_setup();
    let mut snapshot = history.snapshot().unwrap();
    snapshot
        .issues
        .get_mut(&issue_key())
        .unwrap()
        .labels
        .extend(["ready-for-agent".into(), "area:factory".into()]);
    *history.1.borrow_mut() = Some(snapshot);
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Who approves?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    c.execute_worker(research_request()).unwrap();
    let labels = history.snapshot().unwrap().issues[&issue_key()]
        .labels
        .clone();
    expect_that!(labels.contains(&"needs-info".into()), eq(true));
    expect_that!(labels.contains(&"ready-for-agent".into()), eq(false));
    expect_that!(labels.contains(&"wayfinder:research".into()), eq(true));
    expect_that!(labels.contains(&"area:factory".into()), eq(true));
}

#[gtest]
fn cancellation_refreshes_queued_handoff_generation_after_restart() {
    let (dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Which option?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    let mut queued = super::admission("queued");
    queued.admission_id = "handoff-1".into();
    queued.execution_generation = None;
    runtime.0.borrow_mut().handoff_observation = Some(queued.clone());
    let waiting = c.execute_worker(research_request()).unwrap();
    expect_that!(waiting.handoff, eq(&Some(queued.clone())));
    let (store, _, _) = c.into_parts();
    drop(store);
    queued.status = "started".into();
    queued.execution_generation = Some(5);
    runtime.0.borrow_mut().handoff_observation = Some(queued);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    let cancelled = c.cancel_worker("operator stop".into()).unwrap();
    expect_that!(
        cancelled.identity().unwrap().execution_generation,
        eq(Some(5))
    );
    let WorkerCommand::Cancel { worker } =
        serde_json::from_str(&runtime.0.borrow().sent.last().unwrap().payload).unwrap()
    else {
        panic!("cancellation expected")
    };
    expect_that!(worker.execution_generation, eq(Some(5)));
}

#[gtest]
fn uncertain_pause_mutation_recovers_only_by_exact_labels_without_resend() {
    for fault in ["lost", "partial", "readback-fail", "fail-before"] {
        let (dir, mut c, mut history, runtime) = research_setup();
        c.execute_worker(research_request()).unwrap();
        let mut snapshot = history.snapshot().unwrap();
        snapshot
            .issues
            .get_mut(&issue_key())
            .unwrap()
            .labels
            .push("ready-for-agent".into());
        *history.1.borrow_mut() = Some(snapshot);
        let mut result = findings();
        result["human_question"] = json!("Which option?");
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(result);
        history.3.borrow_mut().fault = fault.into();
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(2));
        let (store, _, _) = c.into_parts();
        let action = store.load().unwrap().pending_effect.unwrap().action;
        drop(store);
        let store =
            Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
        let mut c =
            Controller::new(store, history.clone(), runtime.clone(), "controller".into()).unwrap();
        if ["partial", "fail-before", "readback-fail"].contains(&fault) {
            expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
            expect_that!(runtime.0.borrow().sent.len(), eq(2));
        }
        history.3.borrow_mut().fault.clear();
        // A maintainer resolves partial/absent effects on the original exact target.
        let mut snapshot = history.snapshot().unwrap();
        let labels = &mut snapshot.issues.get_mut(&issue_key()).unwrap().labels;
        labels.retain(|s| s != "ready-for-agent");
        if !labels.contains(&"needs-info".into()) {
            labels.push("needs-info".into());
        }
        *history.1.borrow_mut() = Some(snapshot);
        let waiting = c.execute_worker(research_request()).unwrap();
        expect_that!(waiting.phase, eq(&WorkerPhase::HumanInputRequired));
        expect_that!(history.3.borrow().sends, eq(1));
        expect_that!(runtime.0.borrow().sent.len(), eq(3));
        expect_that!(
            history
                .0
                .borrow()
                .iter()
                .filter(|c| c.body.contains("Intent") && c.body.contains(action.id.as_str()))
                .count(),
            eq(1)
        );
    }
}

#[gtest]
fn completed_handoff_reports_delivery_but_never_answers_or_restores_readiness() {
    let (dir, mut c, history, runtime) = research_setup();
    c.execute_worker(research_request()).unwrap();
    let mut result = findings();
    result["human_question"] = json!("Which option?");
    runtime.0.borrow_mut().terminal = true;
    runtime.0.borrow_mut().research_result = Some(result);
    let waiting = c.execute_worker(research_request()).unwrap();
    expect_that!(
        waiting.handoff_delivery,
        eq(&Some(software_factory::worker::HandoffDelivery::Pending))
    );
    let WorkerCommand::Submit { text, .. } =
        serde_json::from_str(&runtime.0.borrow().sent[2].payload).unwrap()
    else {
        panic!("handoff expected")
    };
    expect_that!(text.contains("question_delivered"), eq(true));
    let mut terminal = waiting.handoff.unwrap();
    terminal.status = "terminal".into();
    terminal.outcome = Some("completed".into());
    terminal.result = Some(
        json!({"final_response":"{\"question_delivered\":true,\"evidence\":\"originating issue question comment\"}"}),
    );
    runtime.0.borrow_mut().handoff_observation = Some(terminal);
    let delivered = c.execute_worker(research_request()).unwrap();
    expect_that!(
        delivered.handoff_delivery,
        eq(&Some(software_factory::worker::HandoffDelivery::Delivered))
    );
    expect_that!(delivered.phase, eq(&WorkerPhase::HumanInputRequired));
    let (store, _, _) = c.into_parts();
    drop(store);
    let store = Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
    let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
    expect_that!(
        c.execute_worker(research_request()).unwrap(),
        eq(&delivered)
    );
    expect_that!(c.prepare_admission().is_err(), eq(true));
    expect_that!(runtime.0.borrow().sent.len(), eq(3));
}

#[gtest]
fn incomplete_handoff_results_and_changed_targets_stop_without_replay() {
    for fault in [
        "missing-result",
        "unfinished",
        "empty-evidence",
        "machine-answer",
        "wrong-target",
        "wrong-generation",
    ] {
        let (_dir, mut c, _, runtime) = research_setup();
        c.execute_worker(research_request()).unwrap();
        let mut findings = findings();
        findings["human_question"] = json!("Which option?");
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(findings);
        let waiting = c.execute_worker(research_request()).unwrap();
        let mut result = waiting.handoff.unwrap();
        result.status = "terminal".into();
        result.outcome = Some("completed".into());
        result.result = Some(json!({"question_delivered":true,"evidence":"question reference"}));
        match fault {
            "missing-result" => result.result = None,
            "unfinished" => {
                result.result = Some(json!({"question_delivered":false,"evidence":"not sent"}));
            }
            "empty-evidence" => {
                result.result = Some(json!({"question_delivered":true,"evidence":" "}));
            }
            "machine-answer" => {
                result.result = Some(
                    json!({"question_delivered":true,"evidence":"ref","human_answer":"approved"}),
                );
            }
            "wrong-target" => result.admission_id = "other-admission".into(),
            "wrong-generation" => result.execution_generation = Some(6),
            _ => unreachable!(),
        }
        runtime.0.borrow_mut().handoff_observation = Some(result.clone());
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        expect_that!(c.prepare_admission().is_err(), eq(true));
        expect_that!(runtime.0.borrow().sent.len(), eq(3));
        result.admission_id = "handoff-1".into();
        result.execution_generation = Some(5);
        result.result = Some(json!({"question_delivered":true,"evidence":"question reference"}));
        runtime.0.borrow_mut().handoff_observation = Some(result);
        let recovered = c.execute_worker(research_request()).unwrap();
        expect_that!(recovered.phase, eq(&WorkerPhase::HumanInputRequired));
        expect_that!(
            recovered.handoff_delivery,
            eq(&Some(software_factory::worker::HandoffDelivery::Delivered))
        );
        expect_that!(runtime.0.borrow().sent.len(), eq(3));
    }
}

#[gtest]
fn handoff_is_observed_fresh_and_failed_delivery_never_becomes_success() {
    for outcome in ["failed", "cancelled", "completed"] {
        let (dir, mut c, history, runtime) = research_setup();
        c.execute_worker(research_request()).unwrap();
        let mut result = findings();
        result["human_question"] = json!("Which option?");
        runtime.0.borrow_mut().terminal = true;
        runtime.0.borrow_mut().research_result = Some(result);
        let waiting = c.execute_worker(research_request()).unwrap();
        let mut terminal = waiting.handoff.clone().unwrap();
        terminal.status = "terminal".into();
        terminal.outcome = Some(outcome.into());
        terminal.result = Some(json!({"human_answer":"machine approval"}));
        runtime.0.borrow_mut().handoff_observation = Some(terminal.clone());
        let (store, _, _) = c.into_parts();
        drop(store);
        let store =
            Store::open(&dir.path().join("private"), Identity::new(), &mut Handoff).unwrap();
        let mut c = Controller::new(store, history, runtime.clone(), "controller".into()).unwrap();
        expect_that!(c.execute_worker(research_request()).is_err(), eq(true));
        let (store, _, _) = c.into_parts();
        let retained = store.load().unwrap().worker.unwrap();
        expect_that!(retained.handoff, eq(&Some(terminal)));
        expect_that!(retained.phase, eq(&WorkerPhase::HumanInputRequired));
        expect_that!(
            runtime.0.borrow().observed.last(),
            eq(Some(&waiting.identity().unwrap()))
        );
        expect_that!(runtime.0.borrow().sent.len(), eq(3));
    }
}
