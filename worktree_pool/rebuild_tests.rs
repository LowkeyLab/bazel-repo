use googletest::{assert_that, matchers::eq};
use serde_json::json;

use crate::{
    acquisition::{AcquisitionEvent, Assignment, AssignmentState},
    domain::{CatalogId, management_event, management_event_caused_by},
    management::{AssignmentHandle, ManagementEvent, OperationId},
    rebuild::{RecordedEvent, reconstruct},
    release::{ReleaseEvent, ReleaseOperation, ReleaseState},
};

// Historical envelopes are literal wire fixtures, independent of the live encoder.
fn historical() -> (CatalogId, Vec<(u64, RecordedEvent)>) {
    let catalog = "11111111-1111-4111-8111-111111111111";
    let repository = "22222222-2222-4222-8222-222222222222";
    let path = json!({"encoding":"unix-bytes","bytes":b"/history".to_vec(),"display":"/history"});
    let mut records = Vec::new();
    for (index, (kind, subject, data, stream, expected)) in [
        ("catalog.initialized.v1", format!("catalogs/{catalog}"), json!({"catalog_id":catalog,"store_version":1,"projection_version":1}), None, 0),
        ("repository.registered.v1", format!("repositories/{repository}"), json!({"kind":"repository_registered","record":{"repository_id":repository,"common_directory":path,"context_path":path,"capacity":4,"revision":0}}), Some(format!("catalogs/{catalog}")), 1),
        ("worktree.registered.v1", "worktrees/33333333-3333-4333-8333-333333333333".into(), json!({"kind":"worktree_registered","record":{"worktree_id":"33333333-3333-4333-8333-333333333333","repository_id":repository,"path":path,"git_directory":{"encoding":"unix-bytes","bytes":b"/history/admin".to_vec(),"display":"/history/admin"}}}), Some(format!("repositories/{repository}")), 0),
        ("worktree.registered.v2", "worktrees/44444444-4444-4444-8444-444444444444".into(), json!({"kind":"worktree_enrolled","record":{"worktree_id":"44444444-4444-4444-8444-444444444444","repository_id":repository,"path":{"encoding":"unix-bytes","bytes":b"/second".to_vec(),"display":"/second"},"git_directory":{"encoding":"unix-bytes","bytes":b"/second/admin".to_vec(),"display":"/second/admin"}}}), Some(format!("catalogs/{catalog}")), 2),
    ].into_iter().enumerate() {
        let position = index as u64 + 1;
        let event = json!({"specversion":"1.0","operationid":"66666666-6666-4666-8666-666666666666","id":format!("55555555-5555-4555-8555-{position:012}"),"source":format!("urn:uuid:{catalog}"),"type":format!("io.lowkeylab.worktreepool.{kind}"),"subject":subject,"time":"2025-01-01T00:00:00Z","datacontenttype":"application/json","data":data});
        records.push((position, RecordedEvent { position, expected_revision:expected, stream_id:stream, event }));
    }
    (catalog.parse().unwrap(), records)
}

#[googletest::test]
fn historical_wire_versions_retain_distinct_streams_and_ignore_recording_time_order() {
    let (id, mut records) = historical();
    let before = records.clone();
    let state = reconstruct(id, &records).unwrap().projection;
    assert_that!(state.revision, eq(4));
    assert_that!(state.catalog_stream_revision, eq(Some(3)));
    assert_that!(state.repositories[0].revision, eq(1));
    assert_that!(state.repositories[0].capacity, eq(4));
    assert_that!(state.worktrees.len(), eq(2));
    assert_that!(
        state.worktrees[0].path.bytes.as_slice(),
        eq(b"/history".as_slice())
    );
    assert_that!(records, eq(&before));
    records[0].1.event["time"] = json!("2030-01-01T00:00:00Z");
    records[3].1.event["time"] = json!("2020-01-01T00:00:00Z");
    assert_that!(reconstruct(id, &records).unwrap().projection, eq(&state));
}

fn push_repository_fact(
    id: CatalogId,
    records: &mut Vec<(u64, RecordedEvent)>,
    revision: &mut u64,
    fact: ManagementEvent,
    cause: Option<&str>,
) -> String {
    let event = match cause {
        Some(cause) => management_event_caused_by(id, &fact, cause).unwrap(),
        None => management_event(id, &fact).unwrap(),
    };
    let event_id = event["id"].as_str().unwrap().to_owned();
    let repository = "22222222-2222-4222-8222-222222222222";
    let position = records.len() as u64 + 1;
    records.push((
        position,
        RecordedEvent {
            position,
            expected_revision: *revision,
            stream_id: Some(format!("repositories/{repository}")),
            event,
        },
    ));
    *revision += 1;
    event_id
}

#[googletest::test]
fn generated_ownership_capacity_and_revision_sequences_reconstruct_without_duplicate_transitions() {
    use proptest::{
        collection::vec,
        prelude::*,
        test_runner::{Config, TestCaseError, TestRunner},
    };
    TestRunner::new(Config {
        cases: 32,
        failure_persistence: None,
        ..Config::default()
    })
    .run(
        &(
            vec((any::<bool>(), 2_u32..16), 0..12),
            any::<bool>(),
            0_usize..100,
        ),
        |(cycles, active, damage)| {
            let (id, mut records) = historical();
            let baseline = reconstruct(id, &records).unwrap().projection;
            let worktree = baseline.worktrees[0].clone();
            let repository_id = worktree.repository_id;
            let worktree_id = worktree.worktree_id;
            let mut revision = 1;
            let mut last_release = None;
            let mut capacity = 4;
            for (detached, maximum) in &cycles {
                capacity = *maximum;
                push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::CapacityConfigured {
                        repository_id,
                        maximum: *maximum,
                    },
                    None,
                );
                let assignment = Assignment {
                    assignment_handle: AssignmentHandle::new(),
                    operation_id: OperationId::new(),
                    repository_id,
                    worktree_id,
                    path: worktree.path.clone(),
                    resolved_commit: "1".repeat(40),
                    branch: None,
                    state: AssignmentState::Preparing,
                };
                let cause = push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Acquisition(AcquisitionEvent::Reserved(assignment.clone())),
                    None,
                );
                let cause = push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Acquisition(AcquisitionEvent::CheckoutIntended {
                        operation_id: assignment.operation_id,
                        repository_id,
                        worktree_id,
                    }),
                    Some(&cause),
                );
                push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Acquisition(AcquisitionEvent::Finished {
                        operation_id: assignment.operation_id,
                        repository_id,
                        worktree_id,
                        successful: true,
                    }),
                    Some(&cause),
                );
                let operation_id = OperationId::new();
                let operation = ReleaseOperation {
                    operation_id,
                    repository_id,
                    worktree_id,
                    assignment_handle: assignment.assignment_handle,
                    state: ReleaseState::Intended,
                    tip: "1".repeat(40),
                    branch: (!*detached).then(|| "refs/heads/caller".into()),
                    preservation_reference: detached
                        .then(|| format!("refs/worktree-pool/{operation_id}")),
                    intent_event_id: String::new(),
                    last_checkpoint: "release_intended".into(),
                };
                let mut cause = push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Release(ReleaseEvent::Started(operation)),
                    None,
                );
                if *detached {
                    cause = push_repository_fact(
                        id,
                        &mut records,
                        &mut revision,
                        ManagementEvent::Release(ReleaseEvent::PreservationFinished {
                            operation_id,
                            repository_id,
                            worktree_id,
                            successful: true,
                        }),
                        Some(&cause),
                    );
                }
                push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Release(ReleaseEvent::Finished {
                        operation_id,
                        repository_id,
                        worktree_id,
                        successful: true,
                    }),
                    Some(&cause),
                );
                last_release = Some(records.len() as u64);
            }
            let assignment = Assignment {
                assignment_handle: AssignmentHandle::new(),
                operation_id: OperationId::new(),
                repository_id,
                worktree_id,
                path: worktree.path,
                resolved_commit: "2".repeat(40),
                branch: None,
                state: AssignmentState::Preparing,
            };
            let cause = push_repository_fact(
                id,
                &mut records,
                &mut revision,
                ManagementEvent::Acquisition(AcquisitionEvent::Reserved(assignment.clone())),
                None,
            );
            if active {
                let cause = push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Acquisition(AcquisitionEvent::CheckoutIntended {
                        operation_id: assignment.operation_id,
                        repository_id,
                        worktree_id,
                    }),
                    Some(&cause),
                );
                push_repository_fact(
                    id,
                    &mut records,
                    &mut revision,
                    ManagementEvent::Acquisition(AcquisitionEvent::Finished {
                        operation_id: assignment.operation_id,
                        repository_id,
                        worktree_id,
                        successful: true,
                    }),
                    Some(&cause),
                );
            }
            let before = records.clone();
            let state = reconstruct(id, &records).unwrap().projection;
            let expected_owner = if active {
                AssignmentState::Active
            } else {
                AssignmentState::Preparing
            };
            let verified = googletest::verify_that!(
                (
                    state.assignments.len(),
                    state
                        .assignments
                        .iter()
                        .filter(|a| a.state == AssignmentState::Released)
                        .count(),
                    state.assignments.last().unwrap().assignment_handle,
                    state.assignments.last().unwrap().state,
                    state.repositories[0].revision,
                    state.repositories[0].capacity,
                    state.worktrees[0].last_release_position,
                    state.revision
                ),
                eq((
                    cycles.len() + 1,
                    cycles.len(),
                    assignment.assignment_handle,
                    expected_owner,
                    revision,
                    capacity,
                    last_release,
                    records.len() as u64
                ))
            );
            verified.map_err(|e| TestCaseError::fail(e.to_string()))?;
            googletest::verify_that!(reconstruct(id, &records).unwrap().projection, eq(&state))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            googletest::verify_that!(records, eq(&before))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let selected = damage % records.len();
            records[selected].1.expected_revision += 1;
            googletest::verify_that!(reconstruct(id, &records).is_err(), eq(true))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let mut records = before;
            records[1].1.event["id"] = records[0].1.event["id"].clone();
            googletest::verify_that!(reconstruct(id, &records).is_err(), eq(true))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            Ok(())
        },
    )
    .unwrap();
}
