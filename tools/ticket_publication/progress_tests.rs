//! Tests at the approved publication-event and listener-record boundaries.
use crate::{
    github,
    plan::{Batch, BatchId, Plan},
    progress::{Event, Level, Operation, Outcome, Phase, Progress, Record, Step, Subject},
    publish,
    store::Store,
};
use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::json;
use std::{cell::RefCell, fs, process::Command, rc::Rc};

#[googletest::test]
fn publication_events_follow_durable_checks_and_preserve_request_order() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let fake = rlocation!(files, std::env::var("FAKE_GH")?).context("fake gh")?;
    for mode in [
        "",
        "lose-response",
        "permission",
        "mapping-save",
        "final-save",
        "label-final-content",
    ] {
        let directory = tempfile::tempdir()?;
        let remote = directory.path().join("remote.json");
        fs::write(
            &remote,
            serde_json::to_vec(&json!({"mode":mode,"creations":0,"issues":[]}))?,
        )?;
        let plan: Plan = serde_json::from_value(
            json!({"schema_version":1,"repository":"LowkeyLab/bazel-repo","parent":2401,"tickets":[
                {"key":"first","title":"PRIVATE TITLE","body":"PRIVATE BODY","labels":["ready-for-agent"],"blockers":[]},
                {"key":"second","title":"SECOND PRIVATE TITLE","body":"SECOND PRIVATE BODY","labels":["ready-for-agent"],"blockers":[{"ticket":"first"},{"issue":2392}]}
            ]}),
        )?;
        let mut batch = Batch::prepare(BatchId::new(), plan)?;
        let store = Store::open(&directory.path().join("state"), &batch.id, true)?;
        store.save_new(&batch)?;
        let saved = store.plan_path();
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let progress = Progress::new(move |event| {
            let inject = match &event {
                Event::RemoteOperationStarted(remote) => {
                    (mode == "mapping-save" && remote.operation == Operation::CreateIssue)
                        || (mode == "final-save"
                            && remote.context.phase == Phase::FinalVerification)
                }
                _ => false,
            };
            if inject && saved.is_file() {
                fs::remove_file(&saved).unwrap();
                fs::create_dir(&saved).unwrap();
            }
            if let Event::StepVerified {
                context,
                step: Step::Issue,
            } = &event
            {
                let snapshot: Batch = serde_json::from_slice(&fs::read(&saved).unwrap()).unwrap();
                assert_that!(
                    snapshot.tickets.iter().any(|ticket| context
                        .subjects
                        .contains(&Subject::Ticket(ticket.key.clone()))
                        && matches!(ticket.status, crate::plan::TicketStatus::Created(_))),
                    eq(true)
                );
            }
            if let Event::PublicationCompleted { .. } = &event {
                let snapshot: Batch = serde_json::from_slice(&fs::read(&saved).unwrap()).unwrap();
                assert_that!(snapshot.graph.verified, eq(true));
                assert_that!(snapshot.labels.verified, eq(true));
            }
            captured.borrow_mut().push(event);
        });
        let request_log = directory.path().join("requests.log");
        let command = || {
            let mut command = Command::new(&fake);
            command
                .env("REMOTE_STATE", &remote)
                .env(
                    "REMOTE_ENUMERATIONS",
                    directory.path().join("enumerations.log"),
                )
                .env("REMOTE_REQUESTS", &request_log)
                .env("REMOTE_REQUEST_LIMIT", "2500");
            command
        };
        let github = github::Client::new(&progress, &command);
        let (outcome, result) = publish::publication(&github, &mut batch, &store);
        let events = events.borrow();
        let requests: Vec<_> = events
            .iter()
            .filter_map(|event| {
                if let Event::RemoteOperationStarted(remote) = event {
                    Some(remote)
                } else {
                    None
                }
            })
            .collect();
        assert_that!(
            requests.len(),
            eq(fs::read_to_string(&request_log)?.lines().count())
        );
        assert_that!(
            requests
                .iter()
                .map(|remote| remote.sequence)
                .collect::<Vec<_>>(),
            eq(&(1..=requests.len() as u64).collect::<Vec<_>>())
        );
        if mode.is_empty() {
            let first = crate::plan::TicketKey::try_from("first".to_owned())?;
            let second = crate::plan::TicketKey::try_from("second".to_owned())?;
            let link = requests
                .iter()
                .find(|remote| remote.operation == Operation::AddBlocker)
                .context("blocker request")?;
            assert_that!(
                link.context.subjects.contains(&Subject::Ticket(first)),
                eq(true)
            );
            assert_that!(
                link.context.subjects.contains(&Subject::Ticket(second)),
                eq(true)
            );
            assert_that!(requests[0].operation, eq(Operation::ReadIssue));
            assert_that!(requests[1].operation, eq(Operation::EnumerateIssues));
            assert_that!(requests[2].operation, eq(Operation::ReadRelationships));
        }
        let expected = match mode {
            "" => Outcome::Complete,
            "lose-response" | "mapping-save" => Outcome::Uncertain,
            "label-final-content" => Outcome::Conflict,
            _ => Outcome::Partial,
        };
        assert_that!(outcome, eq(expected));
        assert_that!(result.is_ok(), eq(mode.is_empty()));
        if mode.is_empty() {
            assert_that!(
                events.last(),
                eq(Some(&Event::PublicationCompleted {
                    outcome: Outcome::Complete
                }))
            );
            assert_that!(
                requests
                    .iter()
                    .any(|remote| remote.context.phase == Phase::FinalVerification),
                eq(true)
            );
        } else {
            assert_that!(
                matches!(events.last(), Some(Event::PublicationStopped { outcome: actual, last_operation: Some(_) }) if *actual == expected),
                eq(true)
            );
            assert_that!(
                events
                    .iter()
                    .any(|event| matches!(event, Event::PublicationCompleted { .. })),
                eq(false)
            );
        }
        if mode == "mapping-save" || mode == "lose-response" {
            assert_that!(
                events.iter().any(|event| matches!(
                    event,
                    Event::StepVerified {
                        step: Step::Issue,
                        ..
                    }
                )),
                eq(false)
            );
        }
        // Rendering is deliberately outside these assertions.
        for event in events.iter().cloned() {
            let warning = matches!(event, Event::PublicationStopped { .. });
            let record = Record::from(event.clone());
            assert_that!(record.event, eq(&event));
            assert_that!(
                record.level,
                eq(if warning { Level::Warning } else { Level::Info })
            );
        }
    }
    Ok(())
}
