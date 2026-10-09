use super::*;

fn at(fixture: &Fixture, directory: &std::path::Path, args: &[&str]) -> Value {
    let mut command = fixture.command();
    command.arg("--catalog-dir").arg(directory).args(args);
    serde_json::from_slice(&output(command).stdout).unwrap()
}

fn event_rows(path: &std::path::Path) -> Vec<String> {
    use redb::{ReadableDatabase, ReadableTable, TableDefinition};
    let database = redb::ReadOnlyDatabase::open(path).unwrap();
    let transaction = database.begin_read().unwrap();
    transaction
        .open_table(TableDefinition::<u64, &str>::new("events"))
        .unwrap()
        .iter()
        .unwrap()
        .map(|r| r.unwrap().1.value().to_owned())
        .collect()
}
fn assert_raw_prefix(path: &std::path::Path, prior: &[String], extra: usize) {
    let rows = event_rows(path);
    assert_that!(rows.len(), eq(prior.len() + extra));
    assert_that!(&rows[..prior.len()], eq(prior));
}
fn assert_relocation_suffix(actual: &Value, before: &Value, count: usize) {
    let events = actual["events"].as_array().unwrap();
    let prefix = before["events"].as_array().unwrap();
    assert_that!(events.len(), eq(prefix.len() + count * 2));
    assert_that!(&events[..prefix.len()], eq(prefix.as_slice()));
    for pair in events[prefix.len()..].chunks_exact(2) {
        assert_that!(
            pair[0]["type"].as_str(),
            eq(Some(
                "io.lowkeylab.worktreepool.catalog.relocation.started.v1"
            ))
        );
        assert_that!(
            pair[1]["type"].as_str(),
            eq(Some(
                "io.lowkeylab.worktreepool.catalog.relocation.completed.v1"
            ))
        );
        assert_that!(&pair[1]["causationid"], eq(&pair[0]["id"]));
        assert_that!(&pair[0]["operationid"], eq(&pair[1]["operationid"]));
        for event in pair {
            assert_that!(&event["source"], eq(&prefix[0]["source"]));
            assert_that!(event["specversion"].as_str(), eq(Some("1.0")));
            assert_that!(
                event["datacontenttype"].as_str(),
                eq(Some("application/json"))
            );
        }
    }
}
fn assert_revision_after_relocation(before: &Value, after: &Value) {
    let mut expected = before.clone();
    expected["revision"] = serde_json::json!(before["revision"].as_u64().unwrap() + 2);
    assert_that!(after, eq(&expected));
}

fn assert_projection_relocation(before: &Value, after: &Value) {
    let mut prior = catalog_projection_data(before);
    let mut actual = catalog_projection_data(after);
    assert_that!(
        actual["revision"].as_u64(),
        eq(Some(prior["revision"].as_u64().unwrap() + 2))
    );
    let catalog_revision = prior["catalog_stream_revision"]
        .as_u64()
        .unwrap_or_else(|| 1 + prior["repositories"].as_array().unwrap().len() as u64);
    assert_that!(
        actual["catalog_stream_revision"].as_u64(),
        eq(Some(catalog_revision + 2))
    );
    let prior_moves = prior["relocations"].as_array().unwrap();
    let moves = actual["relocations"].as_array().unwrap();
    assert_that!(moves.len(), eq(prior_moves.len() + 1));
    assert_that!(&moves[..prior_moves.len()], eq(prior_moves.as_slice()));
    assert_that!(
        moves.last().unwrap()["state"].as_str(),
        eq(Some("completed"))
    );
    for field in ["revision", "catalog_stream_revision", "relocations"] {
        prior.as_object_mut().unwrap().remove(field);
        actual.as_object_mut().unwrap().remove(field);
    }
    assert_that!(&actual, eq(&prior));
}

#[googletest::test]
fn relocation_preserves_active_handles_history_capacity_and_absolute_checkout_paths() {
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.empty_pool();
    fixture.json(&[
        "pool",
        "configure",
        "--repo",
        &repository,
        "--max-worktrees",
        "2",
    ]);
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let initial_info = fixture.json(&["catalog", "info"]);
    assert_that!(
        initial_info["data"]["next_action"]
            .as_str()
            .is_some_and(|action| !action.contains("retained") && action.contains("configuration")),
        eq(true)
    );
    let assignment = fixture.json(&["assignment", "inspect", handle]);
    let worktrees = fixture.json(&["worktree", "list"])["data"].clone();
    let history = fixture.json(&["events", "list"])["data"].clone();
    let source_rows = event_rows(&fixture.database());
    let directory = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&directory);
    let result: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    assert_that!(result["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        &result["context"]["catalog_id"],
        eq(&acquired["context"]["catalog_id"])
    );
    assert_raw_prefix(&fixture.database(), &source_rows, 1);
    assert_raw_prefix(&directory.join("catalog.redb"), &source_rows, 2);
    assert_projection_relocation(
        &initial_info["data"],
        &at(&fixture, &directory, &["catalog", "info"])["data"],
    );
    for command in ["info", "check"] {
        let observed = at(&fixture, &directory, &["catalog", command]);
        assert_that!(
            observed["data"]["catalog_mutations_available"].as_bool(),
            eq(Some(true))
        );
        assert_that!(
            observed["data"]["worktrees_moved"].as_bool(),
            eq(Some(false))
        );
        assert_that!(observed["data"]["next_action"].as_str().is_some_and(|action| action.contains("select") && action.contains("configuration")), eq(true));
    }
    assert_relocation_suffix(
        &at(&fixture, &directory, &["events", "list"])["data"],
        &history,
        1,
    );
    assert_revision_after_relocation(
        &assignment["data"],
        &at(&fixture, &directory, &["assignment", "inspect", handle])["data"],
    );
    assert_revision_after_relocation(
        &worktrees,
        &at(&fixture, &directory, &["worktree", "list"])["data"],
    );
    assert_that!(at(&fixture, &directory, &["repo", "inspect", &repository])["data"]["repository"]["capacity"].as_u64(), eq(Some(2)));
    assert_that!(
        fixture.json(&["catalog", "info"])["reason_code"].as_str(),
        eq(Some("catalog_conflict"))
    );
    assert_that!(
        at(&fixture, &directory, &["release", handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
}

#[googletest::test]
fn relocation_recovers_each_real_crash_boundary_without_losing_handles_or_adopting_partial_copies()
{
    for checkpoint in [
        "journal",
        "started-committed",
        "intent",
        "directory",
        "created",
        "copy-started",
        "copied",
        "prepared",
        "switched",
        "completion-recorded",
        "completion-committed",
        "completed",
    ] {
        let fixture = Fixture::new();
        let (_, _, repository) = fixture.empty_pool();
        let acquired = fixture.json(&["acquire", "--repo", &repository]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let assignment = fixture.json(&["assignment", "inspect", handle])["data"].clone();
        let history = fixture.json(&["events", "list"])["data"].clone();
        let prior_rows = event_rows(&fixture.database());
        let mut process = fixture.paused_catalog("relocate", checkpoint, false);
        process.kill().unwrap();
        process.wait().unwrap();
        let original = fs::read(fixture.database()).unwrap();
        let listing = fixture.json(&["operation", "list"]);
        let operations = listing["data"]["operations"].as_array().unwrap();
        assert_that!(
            operations
                .iter()
                .filter(|o| o["kind"] == "relocation")
                .count(),
            eq(1)
        );
        let operation = operations
            .iter()
            .find(|o| o["kind"] == "relocation")
            .unwrap();
        let id = operation["operation_id"].as_str().unwrap();
        let destination = fixture.root.path().join("relocated");
        let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
        let locator = fs::read(&locator_path).unwrap();
        let preview = fixture.json(&["recover", "preview", "--operation", id]);
        assert_that!(
            preview["data"]["catalog_mutations_available"].as_bool(),
            eq(Some(checkpoint == "completed"))
        );
        assert_that!(
            fixture.json(&["operation", "inspect", id])["context"]["operation_id"].as_str(),
            eq(Some(id))
        );
        let info = fixture.json(&["catalog", "info"]);
        if checkpoint != "completed" {
            assert_that!(
                info["data"]["catalog_mutations_available"].as_bool(),
                eq(Some(false))
            );
            assert_that!(info["data"]["worktrees_moved"].as_bool(), eq(Some(false)));
            assert_that!(
                info["data"]["next_action"]
                    .as_str()
                    .unwrap()
                    .contains("relocation"),
                eq(true)
            );
            assert_that!(
                fixture.json(&["acquire", "--repo", &repository])["outcome"] != "completed",
                eq(true)
            );
            let selected = if ["switched", "completion-recorded", "completion-committed"]
                .contains(&checkpoint)
            {
                destination.clone()
            } else {
                fixture.database().parent().unwrap().to_path_buf()
            };
            let bytes_before_rebuild = fs::read(selected.join("catalog.redb")).unwrap();
            assert_that!(
                at(&fixture, &selected, &["catalog", "rebuild"])["reason_code"].as_str(),
                eq(Some("catalog_relocation_pending"))
            );
            assert_that!(
                fs::read(selected.join("catalog.redb")).unwrap(),
                eq(&bytes_before_rebuild)
            );
            assert_that!(
                at(
                    &fixture,
                    &selected,
                    &[
                        "pool",
                        "configure",
                        "--repo",
                        &repository,
                        "--max-worktrees",
                        "8"
                    ]
                )["reason_code"]
                    .as_str(),
                eq(Some("catalog_relocation_pending"))
            );
        }
        assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&original));
        if ["created", "copy-started"].contains(&checkpoint) {
            let partial = fs::read(destination.join("catalog.redb")).unwrap();
            let refusal = fixture.json(&["recover", "apply", "--operation", id]);
            assert_that!(refusal["outcome"].as_str(), eq(Some("rejected")));
            assert_that!(
                fs::read(destination.join("catalog.redb")).unwrap(),
                eq(&partial)
            );
            assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
            // The explicit manual route preserves incomplete tool data outside the destination.
            fs::rename(
                destination.join("catalog.redb"),
                fixture.root.path().join("preserved-partial-copy"),
            )
            .unwrap();
        }
        let applied = fixture.json(&["recover", "apply", "--operation", id]);
        assert_that!(applied["outcome"].as_str(), eq(Some("completed")));
        assert_that!(applied["context"]["operation_id"].as_str(), eq(Some(id)));
        assert_raw_prefix(&fixture.database(), &prior_rows, 1);
        assert_raw_prefix(&destination.join("catalog.redb"), &prior_rows, 2);
        assert_revision_after_relocation(
            &assignment,
            &at(&fixture, &destination, &["assignment", "inspect", handle])["data"],
        );
        assert_relocation_suffix(
            &at(&fixture, &destination, &["events", "list"])["data"],
            &history,
            1,
        );
        let before = fs::read(&locator_path).unwrap();
        assert_that!(
            at(
                &fixture,
                &destination,
                &["recover", "apply", "--operation", id]
            )["reason_code"]
                .as_str(),
            eq(Some("catalog_relocation_already_completed"))
        );
        assert_that!(fs::read(&locator_path).unwrap(), eq(&before));
    }
}

#[googletest::test]
fn earlier_catalog_recovery_receipts_remain_readonly_during_relocation_and_repeated_moves() {
    let fixture = Fixture::new();
    let mut initialization = fixture.paused_catalog("init", "intent", false);
    initialization.kill().unwrap();
    initialization.wait().unwrap();
    let recovered = fixture.json(&["recover", "apply", "--catalog"]);
    let prior = recovered["context"]["operation_id"].as_str().unwrap();
    let prior_receipt = recovered["data"]["operation"].clone();
    let recovery_history = recovered["data"]["authority"]["recovery_history"].clone();
    let mut relocating = fixture.paused_catalog("relocate", "intent", false);
    relocating.kill().unwrap();
    relocating.wait().unwrap();
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let before = fs::read(&locator_path).unwrap();
    let old = fixture.json(&["recover", "apply", "--operation", prior]);
    assert_that!(old["outcome"].as_str(), eq(Some("completed")));
    assert_that!(&old["data"]["operation"], eq(&prior_receipt));
    assert_that!(fs::read(&locator_path).unwrap(), eq(&before));
    let finished = fixture.json(&["recover", "apply", "--catalog"]);
    let first = finished["context"]["operation_id"].as_str().unwrap();
    let destination = fixture.root.path().join("relocated");
    let later = fixture.root.path().join("relocated-again");
    let second = at(
        &fixture,
        &destination,
        &["catalog", "relocate", later.to_str().unwrap()],
    );
    assert_that!(second["outcome"].as_str(), eq(Some("completed")));
    let snapshot = fs::read(&locator_path).unwrap();
    assert_that!(
        at(
            &fixture,
            &later,
            &["recover", "apply", "--operation", first]
        )["reason_code"]
            .as_str(),
        eq(Some("catalog_relocation_already_completed"))
    );
    let observed = at(
        &fixture,
        &later,
        &["recover", "apply", "--operation", prior],
    );
    assert_that!(&observed["data"]["operation"], eq(&prior_receipt));
    assert_that!(
        &observed["data"]["authority"]["recovery_history"],
        eq(&recovery_history)
    );
    assert_that!(fs::read(&locator_path).unwrap(), eq(&snapshot));
}

#[googletest::test]
fn invalid_relocation_destinations_never_change_authority_or_existing_bytes() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let occupied = fixture.root.path().join("occupied");
    fs::create_dir(&occupied).unwrap();
    fs::write(occupied.join("protected"), b"unrelated data").unwrap();
    fs::set_permissions(&occupied, fs::Permissions::from_mode(0o777)).unwrap();
    let alias = fixture.root.path().join("alias");
    symlink(&occupied, &alias).unwrap();
    let parent_alias = fixture.root.path().join("parent-alias");
    symlink(fixture.root.path().join("data"), &parent_alias).unwrap();
    let database = fs::read(fixture.database()).unwrap();
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let locator = fs::read(&locator_path).unwrap();
    for destination in [
        occupied.clone(),
        alias,
        parent_alias.join("new"),
        fixture.database().parent().unwrap().to_path_buf(),
        fixture.root.path().join("state/worktree-pool/new"),
        fixture.root.path().join("missing-parent/new"),
        fixture.root.path().join("../escape"),
    ] {
        let mut command = fixture.command();
        command.args(["catalog", "relocate"]).arg(destination);
        let rejected: Value = serde_json::from_slice(&output(command).stdout).unwrap();
        assert_that!(rejected["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&database));
    }
    assert_that!(
        fs::read(occupied.join("protected")).unwrap().as_slice(),
        eq(b"unrelated data".as_slice())
    );
}

#[googletest::test]
fn relocation_refuses_every_unresolved_repository_lifecycle_without_preparing_a_destination() {
    for kind in [
        "refresh",
        "acquisition",
        "creation",
        "release",
        "recovery",
        "repository-recovery",
        "catalog-recovery",
    ] {
        let fixture = Fixture::new();
        let mut process = if kind == "catalog-recovery" {
            let mut initialization = fixture.paused_catalog("init", "intent", false);
            initialization.kill().unwrap();
            initialization.wait().unwrap();
            fixture.paused_catalog("recover-catalog", "intent", false)
        } else {
            let (_, _, repository) = fixture.empty_pool();
            match kind {
                "refresh" => fixture.paused_refresh(&repository, "intent"),
                "creation" => fixture.paused_acquire(&repository, "creation-intent", false),
                "acquisition" => fixture.paused_acquire(&repository, "reservation", false),
                "release" | "recovery" => {
                    let acquired = fixture.json(&["acquire", "--repo", &repository]);
                    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
                    if kind == "release" {
                        fixture.paused_release(handle, "intent", false)
                    } else {
                        fixture.paused_recovery(handle, "intent", false)
                    }
                }
                "repository-recovery" => {
                    let mut refresh = fixture.paused_refresh(&repository, "intent");
                    refresh.kill().unwrap();
                    refresh.wait().unwrap();
                    let operations = fixture.json(&["operation", "list"]);
                    let original = operations["data"]["operations"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|o| o["state"] == "pending")
                        .unwrap()["operation_id"]
                        .as_str()
                        .unwrap();
                    fixture.paused_repository_recovery(original, "intent")
                }
                _ => unreachable!(),
            }
        };
        process.kill().unwrap();
        process.wait().unwrap();
        let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
        let before = fs::read(&locator_path).unwrap();
        let database = fs::read(fixture.database()).ok();
        let destination = fixture.root.path().join("relocated");
        let mut command = fixture.command();
        command.args(["catalog", "relocate"]).arg(&destination);
        let result: Value = serde_json::from_slice(&output(command).stdout).unwrap();
        assert_that!(result["outcome"] != "completed", eq(true));
        assert_that!(destination.exists(), eq(false));
        assert_that!(fs::read(&locator_path).unwrap(), eq(&before));
        assert_that!(fs::read(fixture.database()).ok(), eq(&database));
    }
}

#[googletest::test]
fn non_utf8_catalog_and_worktree_paths_remain_lossless_across_repeated_relocation() {
    let fixture = Fixture::new();
    let catalog = fixture
        .root
        .path()
        .join(OsString::from_vec(b"catalog-\xff\n".to_vec()));
    at(&fixture, &catalog, &["catalog", "init"]);
    let source = fixture.repository();
    let checkout = fixture
        .root
        .path()
        .join(OsString::from_vec(b"checkout-\xfe ".to_vec()));
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
    );
    let invoke_path = |args: &[&str]| {
        let mut command = fixture.command();
        command
            .arg("--catalog-dir")
            .arg(&catalog)
            .args(args)
            .arg(&checkout);
        serde_json::from_slice::<Value>(&output(command).stdout).unwrap()
    };
    let registered = invoke_path(&["repo", "register"]);
    let repository = registered["context"]["repository_id"].as_str().unwrap();
    invoke_path(&["worktree", "register", "--repo", repository]);
    let acquired = at(&fixture, &catalog, &["acquire", "--repo", repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let mut assignment = at(&fixture, &catalog, &["assignment", "inspect", handle])["data"].clone();
    let mut history = at(&fixture, &catalog, &["events", "list"])["data"].clone();
    let mut prior_rows = event_rows(&catalog.join("catalog.redb"));
    let mut previous = catalog;
    for name in [
        b"destination-\xfd\n".as_slice(),
        b"another-\xfc ".as_slice(),
    ] {
        let destination = fixture.root.path().join(OsString::from_vec(name.to_vec()));
        let mut command = fixture.command();
        command
            .arg("--catalog-dir")
            .arg(&previous)
            .args(["catalog", "relocate"])
            .arg(&destination);
        let result: Value = serde_json::from_slice(&output(command).stdout).unwrap();
        assert_that!(result["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            &result["context"]["catalog_path"]["bytes"],
            eq(&serde_json::json!(
                destination.join("catalog.redb").as_os_str().as_bytes()
            ))
        );
        assert_that!(
            result["data"]["retained_source_active"].as_bool(),
            eq(Some(false))
        );
        assert_that!(
            result["data"]["retained_source_bytes"].as_u64(),
            eq(Some(
                fs::metadata(previous.join("catalog.redb")).unwrap().len()
            ))
        );
        assert_that!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            eq(0o700)
        );
        assert_that!(
            fs::metadata(destination.join("catalog.redb"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            eq(0o600)
        );
        assert_revision_after_relocation(
            &assignment,
            &at(&fixture, &destination, &["assignment", "inspect", handle])["data"],
        );
        let current_history = at(&fixture, &destination, &["events", "list"])["data"].clone();
        assert_relocation_suffix(&current_history, &history, 1);
        assert_raw_prefix(&previous.join("catalog.redb"), &prior_rows, 1);
        assert_raw_prefix(&destination.join("catalog.redb"), &prior_rows, 2);
        assignment = at(&fixture, &destination, &["assignment", "inspect", handle])["data"].clone();
        history = current_history;
        prior_rows = event_rows(&destination.join("catalog.redb"));
        let rejected = at(&fixture, &previous, &["catalog", "init"]);
        assert_that!(
            rejected["reason_code"].as_str(),
            eq(Some("catalog_conflict"))
        );
        assert_that!(
            &rejected["context"]["catalog_path"],
            eq(&result["context"]["catalog_path"])
        );
        previous = destination;
    }
    assert_that!(
        at(&fixture, &previous, &["release", handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
}

#[googletest::test]
fn relocation_excludes_competing_processes_and_keeps_stable_coordination_identity() {
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.empty_pool();
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let maintenance_path = fixture
        .root
        .path()
        .join("state/worktree-pool/maintenance.lock");
    let metadata = fs::metadata(&maintenance_path).unwrap();
    let mut relocating = fixture.paused_catalog("relocate", "intent", true);
    let lock = fs::File::open(&maintenance_path).unwrap();
    assert_that!(lock.try_lock_shared().is_err(), eq(true));
    let mut mutation = fixture.command();
    mutation.args([
        "pool",
        "configure",
        "--repo",
        &repository,
        "--max-worktrees",
        "5",
    ]);
    let mutation = mutation.spawn().unwrap();
    let mut competitor = fixture.command();
    competitor
        .args(["catalog", "relocate"])
        .arg(fixture.root.path().join("competing-destination"));
    let competitor = competitor.spawn().unwrap();
    relocating
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_that!(wait(relocating).status.code(), eq(Some(0)));
    for process in [mutation, competitor] {
        let result: Value = serde_json::from_slice(&wait(process).stdout).unwrap();
        assert_that!(result["reason_code"].as_str(), eq(Some("catalog_conflict")));
    }
    let destination = fixture.root.path().join("relocated");
    assert_that!(at(&fixture, &destination, &["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(), eq(Some("active")));
    assert_that!(at(&fixture, &destination, &["repo", "inspect", &repository])["data"]["repository"]["capacity"].as_u64(), eq(Some(4)));
    assert_that!(
        fixture.root.path().join("competing-destination").exists(),
        eq(false)
    );
    let after = fs::metadata(&maintenance_path).unwrap();
    assert_that!(
        (after.dev(), after.ino()),
        eq((metadata.dev(), metadata.ino()))
    );
    lock.try_lock_shared().unwrap();
    lock.unlock().unwrap();
}

#[googletest::test]
fn ambiguous_copies_after_preparation_or_switch_refuse_mutation_and_require_exact_manual_reconciliation()
 {
    for checkpoint in ["prepared", "switched"] {
        for damage in ["missing", "corrupt", "foreign"] {
            let fixture = Fixture::new();
            fixture.json(&["catalog", "init"]);
            let mut process = fixture.paused_catalog("relocate", checkpoint, false);
            process.kill().unwrap();
            process.wait().unwrap();
            let source = fs::read(fixture.database()).unwrap();
            let operation = fixture.json(&["operation", "list"])["data"]["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["kind"] == "relocation")
                .unwrap()
                .clone();
            let id = operation["operation_id"].as_str().unwrap();
            let directory = fixture.root.path().join("relocated");
            let target = directory.join("catalog.redb");
            let original = fs::read(&target).unwrap();
            let preserved = fixture.root.path().join("manually-preserved-copy");
            fs::rename(&target, &preserved).unwrap();
            let foreign = Fixture::new();
            foreign.json(&["catalog", "init"]);
            match damage {
                "corrupt" => fs::write(&target, b"unrecognized tool data").unwrap(),
                "foreign" => fs::write(&target, fs::read(foreign.database()).unwrap()).unwrap(),
                _ => (),
            }
            if target.exists() {
                fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let damaged = fs::read(&target).ok();
            let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
            let locator = fs::read(&locator_path).unwrap();
            let observed = at(
                &fixture,
                &directory,
                &["recover", "preview", "--operation", id],
            );
            assert_that!(
                observed["data"]["catalog_mutations_available"].as_bool(),
                eq(Some(false))
            );
            assert_that!(
                at(&fixture, &directory, &["catalog", "check"])["outcome"] != "completed",
                eq(true)
            );
            assert_that!(
                fixture.json(&["recover", "apply", "--operation", id])["outcome"].as_str(),
                eq(Some("rejected"))
            );
            assert_that!(
                fixture.json(&[
                    "recover",
                    "apply",
                    "--catalog",
                    "--operation",
                    "11111111-1111-4111-8111-111111111111"
                ])["outcome"]
                    .as_str(),
                eq(Some("rejected"))
            );
            assert_that!(fixture.json(&["recover", "apply", "--repo", "unknown", "--operation", id])["reason_code"].as_str(), eq(Some("selector_conflict")));
            assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
            assert_that!(fs::read(fixture.database()).unwrap(), eq(&source));
            assert_that!(fs::read(&target).ok(), eq(&damaged));
            // Explicit human reconciliation preserves conflict bytes, then restores the exact proved copy.
            if target.exists() {
                fs::rename(&target, fixture.root.path().join("preserved-conflict")).unwrap();
            }
            fs::rename(&preserved, &target).unwrap();
            assert_that!(fs::read(&target).unwrap(), eq(&original));
            assert_that!(
                fixture.json(&["recover", "apply", "--operation", id])["outcome"].as_str(),
                eq(Some("completed"))
            );
        }
    }
}

#[googletest::test]
fn unsupported_gapped_forged_or_mismatched_relocation_history_never_becomes_authority() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let directory = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&directory);
    assert_that!(output(command).status.code(), eq(Some(0)));
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let original = fs::read(&locator_path).unwrap();
    let baseline: Value = serde_json::from_slice(&original).unwrap();
    let database = fs::read(directory.join("catalog.redb")).unwrap();
    for fault in [
        "version",
        "gap",
        "checkpoint",
        "identity",
        "current",
        "active-path",
        "hash-algorithm",
        "hash-length",
        "hash-current",
    ] {
        let mut broken = baseline.clone();
        match fault {
            "version" => broken["relocation_history"][0]["schema_version"] = serde_json::json!(99),
            "gap" => broken["relocation_history"][1]["position"] = serde_json::json!(7),
            "checkpoint" => {
                broken["relocation_history"][1]["relocation"]["checkpoint"] =
                    serde_json::json!("completed");
            }
            "identity" => {
                broken["relocation_history"][1]["relocation"]["operation_id"] =
                    serde_json::json!("11111111-1111-4111-8111-111111111111");
            }
            "current" => broken["relocation"]["source_revision"] = serde_json::json!(987),
            "hash-algorithm" => {
                broken["relocation_history"][0]["relocation"]["source_content"]["algorithm"] =
                    serde_json::json!("sha1");
            }
            "hash-length" => {
                broken["relocation_history"][0]["relocation"]["source_content"]["digest"] =
                    serde_json::json!([0]);
            }
            "hash-current" => {
                broken["relocation"]["source_content"]["digest"] = serde_json::json!(vec![0; 32]);
            }
            _ => broken["catalog_path"] = broken["relocation"]["source"].clone(),
        }
        let corrupted = serde_json::to_vec(&broken).unwrap();
        fs::write(&locator_path, &corrupted).unwrap();
        assert_that!(
            at(&fixture, &directory, &["catalog", "check"])["outcome"].as_str(),
            eq(Some("rejected"))
        );
        assert_that!(
            at(&fixture, &directory, &["recover", "apply", "--catalog"])["outcome"].as_str(),
            eq(Some("rejected"))
        );
        assert_that!(fs::read(&locator_path).unwrap(), eq(&corrupted));
        assert_that!(
            fs::read(directory.join("catalog.redb")).unwrap(),
            eq(&database)
        );
    }
    fs::write(&locator_path, &original).unwrap();
    assert_that!(
        at(&fixture, &directory, &["catalog", "check"])["outcome"].as_str(),
        eq(Some("completed"))
    );
}

#[googletest::test]
fn legacy_catalog_operation_identity_cannot_alias_a_relocation() {
    let fixture = Fixture::new();
    let mut initialization = fixture.paused_catalog("init", "intent", false);
    initialization.kill().unwrap();
    initialization.wait().unwrap();
    let recovered = fixture.json(&["recover", "apply", "--catalog"]);
    let historical_id = recovered["context"]["operation_id"].clone();
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let mut legacy: Value = serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("recovery_history");
    fs::write(&locator_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let destination = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&destination);
    assert_that!(output(command).status.code(), eq(Some(0)));
    let mut aliased: Value = serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
    for fact in aliased["relocation_history"].as_array_mut().unwrap() {
        fact["relocation"]["operation_id"] = historical_id.clone();
    }
    aliased["relocation"]["operation_id"] = historical_id;
    let bytes = serde_json::to_vec(&aliased).unwrap();
    fs::write(&locator_path, &bytes).unwrap();
    assert_that!(
        at(&fixture, &destination, &["catalog", "check"])["reason_code"].as_str(),
        eq(Some("catalog_corrupt"))
    );
    assert_that!(fs::read(&locator_path).unwrap(), eq(&bytes));
}

#[googletest::test]
fn relocation_history_cannot_shadow_an_existing_repository_operation_or_enable_mutation() {
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.empty_pool();
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let id = acquired["context"]["operation_id"].clone();
    let directory = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&directory);
    assert_that!(output(command).status.code(), eq(Some(0)));
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let mut aliased: Value = serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
    for fact in aliased["relocation_history"].as_array_mut().unwrap() {
        fact["relocation"]["operation_id"] = id.clone();
    }
    aliased["relocation"]["operation_id"] = id;
    let locator = serde_json::to_vec(&aliased).unwrap();
    fs::write(&locator_path, &locator).unwrap();
    let database = fs::read(directory.join("catalog.redb")).unwrap();
    assert_that!(
        at(&fixture, &directory, &["catalog", "check"])["reason_code"].as_str(),
        eq(Some("catalog_corrupt"))
    );
    assert_that!(
        at(&fixture, &directory, &["release", handle])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    assert_that!(
        fs::read(directory.join("catalog.redb")).unwrap(),
        eq(&database)
    );
    assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
    for damaged in [false, true] {
        if damaged {
            damage_derived_catalog(&directory.join("catalog.redb"), "revision");
        }
        let before = fs::read(directory.join("catalog.redb")).unwrap();
        assert_that!(
            at(&fixture, &directory, &["catalog", "rebuild"])["reason_code"].as_str(),
            eq(Some("catalog_corrupt"))
        );
        assert_that!(
            fs::read(directory.join("catalog.redb")).unwrap(),
            eq(&before)
        );
        assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
    }
}

#[googletest::test]
fn relocation_check_and_rebuild_preserve_complete_history_live_handles_and_protected_git_state() {
    let fixture = Fixture::new();
    let (_, checkout, repository) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let assignment = fixture.json(&["assignment", "inspect", handle])["data"].clone();
    let history = fixture.json(&["events", "list"])["data"].clone();
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let index = fs::read(checkout.join(".git/index")).unwrap();
    fs::write(
        checkout.join("protected-artifact"),
        b"retained after relocation and rebuild",
    )
    .unwrap();
    let worktrees = fixture.json(&["worktree", "list"])["data"].clone();
    let prior_rows = event_rows(&fixture.database());
    let destination = fixture.root.path().join("relocated");
    let moved = fixture.json(&["catalog", "relocate", destination.to_str().unwrap()]);
    assert_that!(moved["outcome"].as_str(), eq(Some("completed")));
    let id = moved["context"]["operation_id"].as_str().unwrap();
    assert_raw_prefix(&fixture.database(), &prior_rows, 1);
    let source = fs::read(fixture.database()).unwrap();
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let locator = fs::read(&locator_path).unwrap();
    let before = at(&fixture, &destination, &["catalog", "info"]);
    damage_derived_catalog(&destination.join("catalog.redb"), "ownership");
    assert_that!(
        at(&fixture, &destination, &["catalog", "check"])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    let rebuilt = at(&fixture, &destination, &["catalog", "rebuild"]);
    assert_that!(rebuilt["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        &rebuilt["data"],
        eq(&catalog_projection_data(&before["data"]))
    );
    assert_that!(
        at(&fixture, &destination, &["catalog", "check"])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_relocation_suffix(
        &at(&fixture, &destination, &["events", "list"])["data"],
        &history,
        1,
    );
    assert_revision_after_relocation(
        &assignment,
        &at(&fixture, &destination, &["assignment", "inspect", handle])["data"],
    );
    assert_revision_after_relocation(
        &worktrees,
        &at(&fixture, &destination, &["worktree", "list"])["data"],
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&head)
    );
    assert_that!(fs::read(checkout.join(".git/index")).unwrap(), eq(&index));
    assert_that!(
        fs::read(checkout.join("protected-artifact")).unwrap(),
        eq(b"retained after relocation and rebuild")
    );
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&source));
    assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
    let bytes = fs::read(destination.join("catalog.redb")).unwrap();
    assert_that!(
        at(
            &fixture,
            &destination,
            &["recover", "apply", "--operation", id]
        )["reason_code"]
            .as_str(),
        eq(Some("catalog_relocation_already_completed"))
    );
    assert_that!(
        fs::read(destination.join("catalog.redb")).unwrap(),
        eq(&bytes)
    );
}

#[googletest::test]
fn relocation_blocks_pending_retirement_and_retains_completed_tombstones_history_and_ids() {
    let fixture = Fixture::new();
    let (repository, path, repo, worktree) = fixture.retirement_candidate();
    let reconciled = fixture.json(&["recover", "apply", "--worktree", &worktree]);
    let proof = reconciled["context"]["operation_id"].as_str().unwrap();
    let reference = reconciled["data"]["preservation_reference"]
        .as_str()
        .unwrap();
    let tip = fixture.git_stdout(&repository, &["rev-parse", reference]);
    fixture.git(
        &repository,
        &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
    );
    let mut retiring = fixture.paused_recovery_checkpoint(
        "retire",
        "intent",
        false,
        Some(&format!("{worktree}:{proof}")),
    );
    retiring.kill().unwrap();
    retiring.wait().unwrap();
    let state = fixture.json(&["catalog", "info"]);
    let retirement_id = state["data"]["retirements"][0]["operation_id"]
        .as_str()
        .unwrap();
    let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
    let locator = fs::read(&locator_path).unwrap();
    let bytes = fs::read(fixture.database()).unwrap();
    let destination = fixture.root.path().join("relocated");
    let refused = fixture.json(&["catalog", "relocate", destination.to_str().unwrap()]);
    assert_that!(
        refused["reason_code"].as_str(),
        eq(Some("operation_pending"))
    );
    assert_that!(destination.exists(), eq(false));
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&bytes));
    assert_that!(fs::read(&locator_path).unwrap(), eq(&locator));
    assert_that!(
        fixture.json(&["recover", "apply", "--operation", retirement_id])["outcome"].as_str(),
        eq(Some("completed"))
    );
    let history = fixture.json(&["events", "list"])["data"].clone();
    let projection = catalog_projection_data(&fixture.json(&["catalog", "info"])["data"]);
    assert_that!(
        fixture.json(&["catalog", "relocate", destination.to_str().unwrap()])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_projection_relocation(
        &projection,
        &at(&fixture, &destination, &["catalog", "rebuild"])["data"],
    );
    assert_relocation_suffix(
        &at(&fixture, &destination, &["events", "list"])["data"],
        &history,
        1,
    );
    assert_that!(
        at(
            &fixture,
            &destination,
            &["repo", "inspect", "--repo", &repo]
        )["data"]["registered_count"]
            .as_u64(),
        eq(Some(0))
    );
    let bytes = fs::read(destination.join("catalog.redb")).unwrap();
    assert_that!(
        at(
            &fixture,
            &destination,
            &["recover", "apply", "--operation", retirement_id]
        )["data"]["already_retired"]
            .as_bool(),
        eq(Some(true))
    );
    assert_that!(
        at(
            &fixture,
            &destination,
            &["recover", "apply", "--operation", proof]
        )["outcome"]
            .as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fs::read(destination.join("catalog.redb")).unwrap(),
        eq(&bytes)
    );
    assert_that!(
        fixture.git_stdout(&repository, &["rev-parse", reference]),
        eq(&tip)
    );
    assert_that!(path.exists(), eq(false));
    let mut aliased: Value = serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
    for fact in aliased["relocation_history"].as_array_mut().unwrap() {
        fact["relocation"]["operation_id"] = serde_json::json!(retirement_id);
    }
    aliased["relocation"]["operation_id"] = serde_json::json!(retirement_id);
    let corrupted = serde_json::to_vec(&aliased).unwrap();
    fs::write(&locator_path, &corrupted).unwrap();
    for command in ["check", "rebuild"] {
        assert_that!(
            at(&fixture, &destination, &["catalog", command])["reason_code"].as_str(),
            eq(Some("catalog_corrupt"))
        );
        assert_that!(
            fs::read(destination.join("catalog.redb")).unwrap(),
            eq(&bytes)
        );
        assert_that!(fs::read(&locator_path).unwrap(), eq(&corrupted));
    }
}

#[googletest::test]
fn relocation_semantic_cloud_events_retain_prefix_and_rebuild_complete_state() {
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.empty_pool();
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let before = fixture.json(&["events", "list"]);
    let prior_rows = event_rows(&fixture.database());
    let prefix = before["data"]["events"].as_array().unwrap();
    let before_state = fixture.json(&["catalog", "info"]);
    let destination = fixture.root.path().join("relocated");
    let moved = fixture.path_json(&["catalog", "relocate"], &destination);
    assert_that!(moved["outcome"].as_str(), eq(Some("completed")));
    let events = at(&fixture, &destination, &["events", "list"]);
    let complete = events["data"]["events"].as_array().unwrap();
    assert_that!(complete.len(), eq(prefix.len() + 2));
    assert_that!(&complete[..prefix.len()], eq(prefix.as_slice()));
    assert_raw_prefix(&fixture.database(), &prior_rows, 1);
    assert_raw_prefix(&destination.join("catalog.redb"), &prior_rows, 2);
    let suffix = event_rows(&destination.join("catalog.redb"));
    let catalog_revision = before_state["data"]["catalog_stream_revision"]
        .as_u64()
        .unwrap_or_else(|| {
            1 + before_state["data"]["repositories"]
                .as_array()
                .unwrap()
                .len() as u64
        });
    for (index, row) in suffix[prior_rows.len()..].iter().enumerate() {
        let record: Value = serde_json::from_str(row).unwrap();
        assert_that!(
            record["position"].as_u64(),
            eq(Some(prior_rows.len() as u64 + index as u64 + 1))
        );
        assert_that!(
            record["expected_revision"].as_u64(),
            eq(Some(catalog_revision + index as u64))
        );
        assert_that!(
            record["stream_id"].as_str(),
            eq(Some(
                format!(
                    "catalogs/{}",
                    moved["context"]["catalog_id"].as_str().unwrap()
                )
                .as_str()
            ))
        );
    }
    let started = &complete[prefix.len()];
    let completed = &complete[prefix.len() + 1];
    assert_that!(
        started["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.catalog.relocation.started.v1"
        ))
    );
    assert_that!(
        completed["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.catalog.relocation.completed.v1"
        ))
    );
    assert_that!(&completed["causationid"], eq(&started["id"]));
    for event in [started, completed] {
        assert_that!(event["specversion"].as_str(), eq(Some("1.0")));
        assert_that!(&event["source"], eq(&prefix[0]["source"]));
        assert_that!(&event["operationid"], eq(&moved["context"]["operation_id"]));
        assert_that!(
            event["datacontenttype"].as_str(),
            eq(Some("application/json"))
        );
    }
    let info = at(&fixture, &destination, &["catalog", "info"]);
    assert_that!(
        info["data"]["relocations"][0]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        info["data"]["revision"].as_u64(),
        eq(Some(before_state["data"]["revision"].as_u64().unwrap() + 2))
    );
    assert_that!(
        &info["data"]["assignments"],
        eq(&before_state["data"]["assignments"])
    );
    assert_that!(
        &info["data"]["repositories"],
        eq(&before_state["data"]["repositories"])
    );
    let rebuilt = at(&fixture, &destination, &["catalog", "rebuild"]);
    assert_that!(
        &rebuilt["data"],
        eq(&catalog_projection_data(&info["data"]))
    );
    assert_that!(
        &at(&fixture, &destination, &["events", "list"])["data"],
        eq(&events["data"])
    );
    assert_that!(at(&fixture, &destination, &["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(), eq(Some("active")));
}

#[googletest::test]
fn relocation_pending_cloud_event_without_journal_preserves_bytes_and_names_exact_manual_recovery()
{
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.empty_pool();
    let acquired = fixture.json(&["acquire", "--repo", &repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let mut child = fixture.paused_catalog("relocate", "intent", false);
    child.kill().unwrap();
    child.wait().unwrap();
    let path = fixture.root.path().join("state/worktree-pool/active.json");
    let original = fs::read(&path).unwrap();
    let mut journal: Value = serde_json::from_slice(&original).unwrap();
    let id = journal["relocation"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    journal.as_object_mut().unwrap().remove("relocation");
    journal
        .as_object_mut()
        .unwrap()
        .remove("relocation_history");
    fs::write(&path, serde_json::to_vec(&journal).unwrap()).unwrap();
    let bytes = fs::read(fixture.database()).unwrap();
    let missing = fs::read(&path).unwrap();
    let fresh = fixture.root.path().join("forbidden-fresh-relocation");
    for args in [
        vec!["catalog", "check"],
        vec!["catalog", "rebuild"],
        vec!["acquire", "--repo", &repository],
        vec!["catalog", "relocate", fresh.to_str().unwrap()],
    ] {
        let result = fixture.json(&args);
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&bytes));
        assert_that!(fs::read(&path).unwrap(), eq(&missing));
        assert_that!(fresh.exists(), eq(false));
        assert_that!(result["outcome"].as_str(), eq(Some("pending")));
        assert_that!(
            result["reason_code"].as_str(),
            eq(Some("catalog_relocation_pending"))
        );
    }
    let info = fixture.json(&["catalog", "info"]);
    assert_that!(
        info["data"]["catalog_mutations_available"].as_bool(),
        eq(Some(false))
    );
    let action = info["data"]["next_action"].as_str().unwrap();
    assert_that!(
        action.contains(&id) && action.contains("journal") && action.contains("restore"),
        eq(true)
    );
    for action in ["preview", "apply"] {
        let result = fixture.json(&["recover", action, "--operation", &id]);
        assert_that!(result["outcome"].as_str(), eq(Some("pending")));
        assert_that!(
            result["data"]["next_action"]
                .as_str()
                .is_some_and(|s| s.contains(&id) && s.contains("restore")),
            eq(true)
        );
    }
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&bytes));
    assert_that!(fs::read(&path).unwrap(), eq(&missing));
    fs::write(&path, original).unwrap();
    assert_that!(
        fixture.json(&["recover", "apply", "--operation", &id])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        at(
            &fixture,
            &fixture.root.path().join("relocated"),
            &["assignment", "inspect", handle]
        )["data"]["assignment"]["state"]
            .as_str(),
        eq(Some("active"))
    );
}

#[googletest::test]
fn relocation_legacy_locator_only_history_preserves_bytes_and_requires_manual_reconciliation() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let mut child = fixture.paused_catalog("relocate", "intent", false);
    child.kill().unwrap();
    child.wait().unwrap();
    let path = fixture.root.path().join("state/worktree-pool/active.json");
    let mut legacy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let id = legacy["relocation"]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut operation = legacy["relocation"].clone();
    for field in [
        "intent_event",
        "completion_event",
        "prefix_content",
        "destination_content",
    ] {
        operation.as_object_mut().unwrap().remove(field);
    }
    legacy["relocation"] = operation.clone();
    legacy["relocation_history"] =
        serde_json::json!([{"schema_version":1,"position":1,"relocation":operation}]);
    for version in [1, 2] {
        legacy["relocation_history"][0]["schema_version"] = serde_json::json!(version);
        let journal = serde_json::to_vec(&legacy).unwrap();
        fs::write(&path, &journal).unwrap();
        let bytes = fs::read(fixture.database()).unwrap();
        for args in [
            vec!["catalog", "check"],
            vec!["recover", "apply", "--operation", &id],
        ] {
            let result = fixture.json(&args);
            assert_that!(
                result["reason_code"].as_str(),
                eq(Some("unsupported_version"))
            );
            let action = result["data"]["next_action"].as_str().unwrap();
            assert_that!(
                action.contains(if version == 1 {
                    "locator-only"
                } else {
                    "decoded"
                }) && action.contains("manual")
                    && action.contains("preserve"),
                eq(true)
            );
            assert_that!(fs::read(&path).unwrap(), eq(&journal));
            assert_that!(fs::read(fixture.database()).unwrap(), eq(&bytes));
        }
    }
}

#[googletest::test]
fn relocation_historical_receipt_retains_recorded_size_when_inactive_path_is_replaced_or_missing() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let destination = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&destination);
    let moved: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    let id = moved["context"]["operation_id"].as_str().unwrap();
    let recorded = moved["data"]["retained_source_bytes"].clone();
    let source = fixture.database();
    let archive = fixture.root.path().join("preserved-original-source");
    fs::rename(&source, &archive).unwrap();
    for replacement in [true, false] {
        if replacement {
            fs::write(&source, b"unrelated user sentinel").unwrap();
        } else {
            fs::remove_file(&source).unwrap();
        }
        let before = fs::read(destination.join("catalog.redb")).unwrap();
        let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
        let journal = fs::read(&locator_path).unwrap();
        for args in [
            vec!["operation", "inspect", id],
            vec!["recover", "preview", "--operation", id],
            vec!["recover", "apply", "--operation", id],
        ] {
            let receipt = at(&fixture, &destination, &args);
            assert_that!(receipt["outcome"].as_str(), eq(Some("completed")));
            assert_that!(&receipt["data"]["retained_source_bytes"], eq(&recorded));
            assert_that!(
                receipt["data"]["retained_source_bytes_checkpoint"].as_str(),
                eq(Some("intended"))
            );
            assert_that!(
                fs::read(destination.join("catalog.redb")).unwrap(),
                eq(&before)
            );
            assert_that!(fs::read(&locator_path).unwrap(), eq(&journal));
            if replacement {
                assert_that!(
                    fs::read(&source).unwrap().as_slice(),
                    eq(b"unrelated user sentinel".as_slice())
                );
            } else {
                assert_that!(source.exists(), eq(false));
            }
        }
    }
    assert_that!(
        fs::metadata(archive).unwrap().len(),
        eq(recorded.as_u64().unwrap())
    );
}
