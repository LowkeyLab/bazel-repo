use super::*;

fn at(fixture: &Fixture, directory: &std::path::Path, args: &[&str]) -> Value {
    let mut command = fixture.command();
    command.arg("--catalog-dir").arg(directory).args(args);
    serde_json::from_slice(&output(command).stdout).unwrap()
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
    let source_bytes = fs::read(fixture.database()).unwrap();
    let directory = fixture.root.path().join("relocated");
    let mut command = fixture.command();
    command.args(["catalog", "relocate"]).arg(&directory);
    let result: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    assert_that!(result["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        &result["context"]["catalog_id"],
        eq(&acquired["context"]["catalog_id"])
    );
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&source_bytes));
    assert_that!(
        fs::read(directory.join("catalog.redb")).unwrap(),
        eq(&source_bytes)
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
    assert_that!(
        &at(&fixture, &directory, &["events", "list"])["data"],
        eq(&history)
    );
    assert_that!(
        &at(&fixture, &directory, &["assignment", "inspect", handle])["data"],
        eq(&assignment["data"])
    );
    assert_that!(
        &at(&fixture, &directory, &["worktree", "list"])["data"],
        eq(&worktrees)
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
        "intent",
        "directory",
        "created",
        "copy-started",
        "copied",
        "prepared",
        "switched",
        "completed",
    ] {
        let fixture = Fixture::new();
        let (_, _, repository) = fixture.empty_pool();
        let acquired = fixture.json(&["acquire", "--repo", &repository]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let assignment = fixture.json(&["assignment", "inspect", handle])["data"].clone();
        let history = fixture.json(&["events", "list"])["data"].clone();
        let original = fs::read(fixture.database()).unwrap();
        let mut process = fixture.paused_catalog("relocate", checkpoint, false);
        process.kill().unwrap();
        process.wait().unwrap();
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
            let selected = if checkpoint == "switched" {
                destination.clone()
            } else {
                fixture.database().parent().unwrap().to_path_buf()
            };
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
        assert_that!(
            fs::read(destination.join("catalog.redb")).unwrap(),
            eq(&original)
        );
        assert_that!(
            &at(&fixture, &destination, &["assignment", "inspect", handle])["data"],
            eq(&assignment)
        );
        assert_that!(
            &at(&fixture, &destination, &["events", "list"])["data"],
            eq(&history)
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
    let assignment = at(&fixture, &catalog, &["assignment", "inspect", handle])["data"].clone();
    let history = at(&fixture, &catalog, &["events", "list"])["data"].clone();
    let bytes = fs::read(catalog.join("catalog.redb")).unwrap();
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
            eq(Some(bytes.len() as u64))
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
        assert_that!(
            &at(&fixture, &destination, &["assignment", "inspect", handle])["data"],
            eq(&assignment)
        );
        assert_that!(
            &at(&fixture, &destination, &["events", "list"])["data"],
            eq(&history)
        );
        assert_that!(fs::read(previous.join("catalog.redb")).unwrap(), eq(&bytes));
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
            let source = fs::read(fixture.database()).unwrap();
            let mut process = fixture.paused_catalog("relocate", checkpoint, false);
            process.kill().unwrap();
            process.wait().unwrap();
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
                    serde_json::json!("completed")
            }
            "identity" => {
                broken["relocation_history"][1]["relocation"]["operation_id"] =
                    serde_json::json!("11111111-1111-4111-8111-111111111111")
            }
            "current" => broken["relocation"]["source_revision"] = serde_json::json!(987),
            "hash-algorithm" => {
                broken["relocation_history"][0]["relocation"]["source_content"]["algorithm"] =
                    serde_json::json!("sha1")
            }
            "hash-length" => {
                broken["relocation_history"][0]["relocation"]["source_content"]["digest"] =
                    serde_json::json!([0])
            }
            "hash-current" => {
                broken["relocation"]["source_content"]["digest"] = serde_json::json!(vec![0; 32])
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
}
