//! Verify command outcomes through controlled API reads and real receipt files.
use std::{fs, path::PathBuf, process::Command};

use anyhow::{Context, Result, ensure};
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
const HEAD: &str = "1111111111111111111111111111111111111111";
const BASE: &str = "2222222222222222222222222222222222222222";
const MERGE: &str = "3333333333333333333333333333333333333333";
const PARENT: &str = "4444444444444444444444444444444444444444";
const HEAD_TREE: &str = "5555555555555555555555555555555555555555";
const MERGE_TREE: &str = "6666666666666666666666666666666666666666";
const PARENT_TREE: &str = "7777777777777777777777777777777777777777";
const OLD: &str = "8888888888888888888888888888888888888888";
const NEW: &str = "9999999999999999999999999999999999999999";
const EXTRA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REPO: &str = "repos/LowkeyLab/bazel-repo";
fn hosted(head: &str, state: &str, findings: bool) -> Value {
    json!({"id":10,"created_at":"2026-10-28T00:00:00Z","user":{"id":199_175_422,"login":"chatgpt-codex-connector[bot]","type":"Bot"},"body":format!("<!-- codex-review-state:v1 {} -->",json!({"code_review":{"commit_sha":head,"status":state,"has_any_findings":findings,"has_unresolved_findings":findings}}))})
}
fn response(body: Value, pages: bool) -> Value {
    let mut value = json!({"pages":pages});
    value["body"] = body;
    value
}
struct Fixture {
    directory: tempfile::TempDir,
    tool: PathBuf,
}
impl Fixture {
    fn new() -> Result<Self> {
        let files = Runfiles::create()?;
        let tool = rlocation!(files, std::env::var("VERIFICATION_TOOL")?).context("tool")?;
        let fake = rlocation!(files, std::env::var("VERIFICATION_FAKE_GH")?).context("fake gh")?;
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("bin"))?;
        std::os::unix::fs::symlink(fake, directory.path().join("bin/gh"))?;
        let f = Self { directory, tool };
        f.write("remote.json", &json!({"routes":{},"counts":{},"calls":[]}))?;
        f.write("files.json", &json!(["code.rs"]))?;
        f.write(
            "review.json",
            &json!({"head":HEAD,"standards_findings":0,"spec_findings":0}),
        )?;
        f.route("pulls/7",response(json!({"number":7,"state":"open","merged":false,"mergeable":true,"head":{"sha":HEAD},"base":{"ref":"main","sha":BASE,"repo":{"full_name":"LowkeyLab/bazel-repo"}},"changed_files":1,"merge_commit_sha":null}),false))?;
        f.route(
            "pulls/7/files?per_page=100",
            response(json!([[{"filename":"code.rs","status":"modified"}]]), true),
        )?;
        f.route(&format!("commits/{HEAD}/check-runs?per_page=100"),response(json!([{"total_count":4,"check_runs":[check("test",15368),check("lint",15368),check("format",15368),check("codecov/patch",254)]}]),true))?;
        f.route(
            &format!("commits/{HEAD}/statuses?per_page=100"),
            response(json!([[]]), true),
        )?;
        f.route("rules/branches/main?per_page=100",response(json!([[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"test","integration_id":15368}]}}]]),true))?;
        f.route(
            "branches/main/protection/required_status_checks",
            response(json!({"contexts":[],"checks":[]}), false),
        )?;
        f.route(&format!("actions/runs?head_sha={HEAD}&per_page=100"),response(json!([{"total_count":2,"workflow_runs":[{"name":"Bazel Tests","head_sha":HEAD,"status":"completed","conclusion":"success"},{"name":"Coverage","head_sha":HEAD,"status":"completed","conclusion":"success"}]}]),true))?;
        f.route("pulls/7/reviews?per_page=100", response(json!([[]]), true))?;
        f.route(
            "issues/7/comments?per_page=100",
            response(json!([[hosted(HEAD, "completed", false)]]), true),
        )?;
        f.graph(response(threads(&json!([]), 0, false, &Value::Null), false))?;
        Ok(f)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
    fn write(&self, name: &str, value: &Value) -> Result<()> {
        fs::write(self.path(name), serde_json::to_vec(value)?)?;
        Ok(())
    }
    fn remote(&self) -> Result<Value> {
        Ok(serde_json::from_slice(&fs::read(
            self.path("remote.json"),
        )?)?)
    }
    fn route(&self, path: &str, value: Value) -> Result<()> {
        let mut d = self.remote()?;
        d["routes"][format!("{REPO}/{path}")] = value;
        d["counts"][format!("{REPO}/{path}")] = json!(0);
        self.write("remote.json", &d)
    }
    fn graph(&self, value: Value) -> Result<()> {
        let mut d = self.remote()?;
        d["routes"]["graphql"] = value;
        self.write("remote.json", &d)
    }
    fn get(&self, path: &str) -> Result<Value> {
        Ok(self.remote()?["routes"][format!("{REPO}/{path}")]["body"].clone())
    }
    fn run(&self, phase: &str) -> Result<(Value, bool)> {
        let mut command = Command::new(&self.tool);
        command
            .args([
                phase,
                "--repo",
                "LowkeyLab/bazel-repo",
                "--pr",
                "7",
                "--head",
                HEAD,
                "--reviewed-files",
            ])
            .arg(self.path("files.json"))
            .arg("--source-review")
            .arg(self.path("review.json"))
            .arg("--receipt-dir")
            .arg(self.path("receipts"));
        if phase == "after" {
            command
                .arg("--before-receipt")
                .arg(self.path("before.json"));
        }
        let path = std::env::join_paths(std::iter::once(self.path("bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").context("PATH")?),
        ))?;
        let output = command
            .env("PATH", path)
            .env("VERIFICATION_REMOTE", self.path("remote.json"))
            .current_dir(self.directory.path())
            .output()?;
        let value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "invalid result: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        assert_that!(value["passed"].as_bool(), eq(Some(output.status.success())));
        Ok((value, output.status.success()))
    }
    fn accepted(&self, phase: &str) -> Result<Value> {
        let (v, ok) = self.run(phase)?;
        ensure!(ok, "unexpected rejection: {v}");
        Ok(v)
    }
    fn rejected(&self, phase: &str, reason: &str) -> Result<Value> {
        let (v, ok) = self.run(phase)?;
        assert_that!(ok, eq(false));
        assert_that!(v["reasons"].to_string().contains(reason), eq(true));
        Ok(v)
    }
    fn prepare_after(&self) -> Result<()> {
        self.write("before.json", &self.accepted("before")?)?;
        let mut pull = self.get("pulls/7")?;
        pull["state"] = json!("closed");
        pull["merged"] = json!(true);
        pull["merge_commit_sha"] = json!(MERGE);
        self.route("pulls/7", response(pull, false))?;
        self.commit_tree(HEAD, HEAD_TREE, &json!([]), &[blob("code.rs", NEW)])?;
        self.commit_tree(
            PARENT,
            PARENT_TREE,
            &json!([]),
            &[blob("code.rs", OLD), blob("unrelated.md", EXTRA)],
        )?;
        self.commit_tree(
            MERGE,
            MERGE_TREE,
            &json!([{"sha":PARENT}]),
            &[blob("code.rs", NEW), blob("unrelated.md", EXTRA)],
        )?;
        Ok(())
    }
    fn commit_tree(&self, sha: &str, tree: &str, parents: &Value, rows: &[Value]) -> Result<()> {
        self.route(
            &format!("git/commits/{sha}"),
            response(
                json!({"sha":sha,"tree":{"sha":tree},"parents":parents}),
                false,
            ),
        )?;
        self.route(
            &format!("git/trees/{tree}?recursive=1"),
            response(json!({"sha":tree,"tree":rows,"truncated":false}), false),
        )
    }
}
fn check(name: &str, app: u64) -> Value {
    json!({"name":name,"head_sha":HEAD,"status":"completed","conclusion":"success","app":{"id":app}})
}
fn threads(nodes: &Value, total: u64, next: bool, cursor: &Value) -> Value {
    json!({"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":nodes,"totalCount":total,"pageInfo":{"hasNextPage":next,"endCursor":cursor}}}}}})
}
fn blob(path: &str, sha: &str) -> Value {
    json!({"path":path,"sha":sha,"mode":"100644","type":"blob"})
}

#[googletest::test]
fn accepts_complete_current_evidence_and_retains_private_raw_receipts() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let result = f.accepted("before")?;
    let dir = PathBuf::from(
        result["receipt_directory"]
            .as_str()
            .context("receipt directory")?,
    );
    assert_that!(fs::metadata(&dir)?.permissions().mode() & 0o077, eq(0));
    let files = fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
    assert_that!(
        files.iter().any(|e| e.file_name() == "0001-stdout"),
        eq(true)
    );
    assert_that!(
        files.iter().any(|e| e.file_name() == "0001-stderr"),
        eq(true)
    );
    let exit: Value = serde_json::from_slice(&fs::read(dir.join("0001-exit.json"))?)?;
    assert_that!(exit["code"].as_i64(), eq(Some(0)));
    assert_that!(
        fs::metadata(dir.join("0001-stdout"))?.permissions().mode() & 0o077,
        eq(0)
    );
    let saved: Value = serde_json::from_slice(&fs::read(dir.join("result.json"))?)?;
    assert_that!(saved, eq(&result));
    Ok(())
}
#[googletest::test]
fn rejects_changed_closed_and_unconfirmed_heads() -> Result<()> {
    for (key, value, reason) in [
        ("state", json!("closed"), "closed"),
        ("merged", json!(true), "merged"),
        ("mergeable", Value::Null, "mergeable"),
    ] {
        let f = Fixture::new()?;
        let mut p = f.get("pulls/7")?;
        p[key] = value;
        f.route("pulls/7", response(p, false))?;
        f.rejected("before", reason)?;
    }
    let f = Fixture::new()?;
    let p = f.get("pulls/7")?;
    let mut changed = p.clone();
    changed["head"]["sha"] = json!(BASE);
    f.route(
        "pulls/7",
        json!({"sequence":[response(p,false),response(changed,false)]}),
    )?;
    f.rejected("before", "head changed")?;
    Ok(())
}
#[googletest::test]
fn rejects_incomplete_failed_missing_and_wrong_app_checks() -> Result<()> {
    let endpoint = format!("commits/{HEAD}/check-runs?per_page=100");
    for (key, value) in [
        ("status", json!("in_progress")),
        ("conclusion", json!("failure")),
        ("head_sha", json!(BASE)),
    ] {
        let f = Fixture::new()?;
        let mut p = f.get(&endpoint)?;
        p[0]["check_runs"][0][key] = value;
        f.route(&endpoint, response(p, true))?;
        f.rejected(
            "before",
            if key == "head_sha" {
                "head differs"
            } else {
                "incomplete or failed"
            },
        )?;
    }
    for remove in [true, false] {
        let f = Fixture::new()?;
        let mut p = f.get(&endpoint)?;
        if remove {
            p[0]["check_runs"].as_array_mut().context("checks")?.pop();
            p[0]["total_count"] = json!(3);
        } else {
            p[0]["check_runs"][3]["app"]["id"] = json!(1);
        }
        f.route(&endpoint, response(p, true))?;
        f.rejected("before", "required check is missing: codecov/patch")?;
    }
    Ok(())
}
#[googletest::test]
fn documentation_only_changes_allow_an_absent_patch_check_but_not_a_failed_one() -> Result<()> {
    for filename in ["README.md", "README.MD"] {
        for present in [false, true] {
            let f = Fixture::new()?;
            f.write("files.json", &json!([filename]))?;
            f.route(
                "pulls/7/files?per_page=100",
                response(json!([[{"filename":filename,"status":"modified"}]]), true),
            )?;
            let endpoint = format!("commits/{HEAD}/check-runs?per_page=100");
            let mut p = f.get(&endpoint)?;
            if present {
                p[0]["check_runs"][3]["conclusion"] = json!("failure");
            } else {
                p[0]["check_runs"].as_array_mut().context("checks")?.pop();
                p[0]["total_count"] = json!(3);
            }
            f.route(&endpoint, response(p, true))?;
            if present {
                f.rejected("before", "incomplete or failed")?;
            } else {
                let v = f.accepted("before")?;
                assert_that!(v["checks"]["documentation_only"].as_bool(), eq(Some(true)));
            }
        }
    }
    Ok(())
}
#[googletest::test]
fn checks_all_status_workflow_and_ruleset_requirements() -> Result<()> {
    let f = Fixture::new()?;
    f.route(
        &format!("commits/{HEAD}/statuses?per_page=100"),
        response(
            json!([[{"id":1,"context":"external","state":"pending"}]]),
            true,
        ),
    )?;
    f.rejected("before", "commit status")?;
    let f = Fixture::new()?;
    let endpoint = format!("actions/runs?head_sha={HEAD}&per_page=100");
    let mut runs = f.get(&endpoint)?;
    runs[0]["workflow_runs"][0]["conclusion"] = json!("failure");
    f.route(&endpoint, response(runs, true))?;
    f.rejected("before", "incomplete or failed")?;
    let f = Fixture::new()?;
    f.route("rules/branches/main?per_page=100",response(json!([[{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"additional","integration_id":1}]}}]]),true))?;
    f.rejected("before", "required check is missing: additional")?;
    Ok(())
}
#[googletest::test]
fn only_latest_authenticated_exact_head_reviews_can_accept() -> Result<()> {
    for comment in [
        hosted(BASE, "completed", false),
        hosted(HEAD, "running", false),
        hosted(HEAD, "completed", true),
    ] {
        let f = Fixture::new()?;
        f.route(
            "issues/7/comments?per_page=100",
            response(json!([[comment]]), true),
        )?;
        f.rejected("before", "review")?;
    }
    let f = Fixture::new()?;
    let mut forged = hosted(HEAD, "completed", false);
    forged["user"]["id"] = json!(42);
    f.route(
        "issues/7/comments?per_page=100",
        response(json!([[forged]]), true),
    )?;
    f.rejected("before", "authenticated review")?;
    let f = Fixture::new()?;
    let mut later = hosted(HEAD, "running", false);
    later["id"] = json!(11);
    f.route(
        "issues/7/comments?per_page=100",
        response(json!([[hosted(HEAD, "completed", false)], [later]]), true),
    )?;
    f.rejected("before", "incomplete or has findings")?;
    Ok(())
}
#[googletest::test]
fn change_requests_and_every_review_thread_page_block_acceptance() -> Result<()> {
    let f = Fixture::new()?;
    f.route("pulls/7/reviews?per_page=100",response(json!([[{"id":1,"state":"CHANGES_REQUESTED","user":{"id":42}}],[{"id":2,"state":"COMMENTED","user":{"id":42}}]]),true))?;
    f.rejected("before", "active change request")?;
    let f = Fixture::new()?;
    f.graph(json!({"sequence":[response(threads(&json!([{"isResolved":true}]),2,true,&json!("cursor")),false),response(threads(&json!([{"isResolved":false}]),2,false,&Value::Null),false)]}))?;
    f.rejected("before", "unresolved review thread")?;
    Ok(())
}
#[googletest::test]
fn failed_malformed_and_incomplete_api_reads_preserve_failure_receipts() -> Result<()> {
    for route in [
        json!({"exit":1,"body":{"message":"controlled read failure"}}),
        json!({"raw":"partial{"}),
        response(json!({}), false),
    ] {
        let f = Fixture::new()?;
        f.route("pulls/7", route)?;
        let result = f.rejected("before", "")?;
        let dir = PathBuf::from(result["receipt_directory"].as_str().context("receipt")?);
        assert_that!(dir.join("0001-stdout").is_file(), eq(true));
        assert_that!(dir.join("0001-exit.json").is_file(), eq(true));
    }
    let f = Fixture::new()?;
    let endpoint = format!("commits/{HEAD}/check-runs?per_page=100");
    let mut checks = f.get(&endpoint)?;
    checks[0]["total_count"] = json!(5);
    f.route(&endpoint, response(checks, true))?;
    f.rejected("before", "incomplete")?;
    let f = Fixture::new()?;
    f.graph(response(threads(&json!([]), 1, false, &Value::Null), false))?;
    f.rejected("before", "incomplete")?;
    Ok(())
}
#[googletest::test]
fn rejects_unreviewed_paths_and_unaccepted_source_reviews() -> Result<()> {
    let f = Fixture::new()?;
    f.write("files.json", &json!(["other.rs"]))?;
    f.rejected("before", "files differ")?;
    let f = Fixture::new()?;
    f.write(
        "review.json",
        &json!({"head":BASE,"standards_findings":0,"spec_findings":0}),
    )?;
    f.rejected("before", "source reviews")?;
    let f = Fixture::new()?;
    f.write("files.json", &json!(["../private"]))?;
    f.rejected("before", "invalid reviewed path")?;
    Ok(())
}
#[googletest::test]
fn advanced_target_squash_accepts_only_reviewed_blob_contents() -> Result<()> {
    let f = Fixture::new()?;
    f.prepare_after()?;
    let v = f.accepted("after")?;
    assert_that!(v["merge_commit"].as_str(), eq(Some(MERGE)));
    assert_that!(v["merged_blobs_match"].as_bool(), eq(Some(true)));
    let f = Fixture::new()?;
    f.prepare_after()?;
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[blob("code.rs", OLD), blob("unrelated.md", EXTRA)],
    )?;
    f.rejected("after", "merged file differs")?;
    Ok(())
}
#[googletest::test]
fn already_present_changes_and_deletions_compare_against_reviewed_contents() -> Result<()> {
    let f = Fixture::new()?;
    f.prepare_after()?;
    f.commit_tree(
        PARENT,
        PARENT_TREE,
        &json!([]),
        &[blob("code.rs", NEW), blob("unrelated.md", EXTRA)],
    )?;
    let v = f.accepted("after")?;
    assert_that!(v["already_present"], eq(&json!(["code.rs"])));
    let f = Fixture::new()?;
    f.route(
        "pulls/7/files?per_page=100",
        response(json!([[{"filename":"code.rs","status":"removed"}]]), true),
    )?;
    f.prepare_after()?;
    f.commit_tree(HEAD, HEAD_TREE, &json!([]), &[])?;
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[blob("unrelated.md", EXTRA)],
    )?;
    f.accepted("after")?;
    Ok(())
}
#[googletest::test]
fn post_merge_rejects_extra_changes_truncated_trees_and_wrong_receipts() -> Result<()> {
    let f = Fixture::new()?;
    f.prepare_after()?;
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[blob("code.rs", NEW), blob("unrelated.md", OLD)],
    )?;
    f.rejected("after", "outside reviewed scope")?;
    let f = Fixture::new()?;
    f.prepare_after()?;
    let endpoint = format!("git/trees/{HEAD_TREE}?recursive=1");
    let mut tree = f.get(&endpoint)?;
    tree["truncated"] = json!(true);
    f.route(&endpoint, response(tree, false))?;
    f.rejected("after", "incomplete")?;
    let f = Fixture::new()?;
    f.prepare_after()?;
    f.write("before.json", &json!({"passed":true,"head":BASE}))?;
    f.rejected("after", "before receipt")?;
    Ok(())
}

#[googletest::test]
fn receipt_location_errors_stop_before_github_reads() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    let output = Command::new("git")
        .args(["init", "--quiet"])
        .arg(f.directory.path())
        .output()?;
    ensure!(
        output.status.success(),
        "test repository initialization failed"
    );
    f.rejected("before", "outside Git")?;
    assert_that!(f.path("receipts").exists(), eq(false));
    assert_that!(
        f.remote()?["calls"].as_array().context("calls")?.len(),
        eq(0)
    );
    let f = Fixture::new()?;
    fs::create_dir(f.path("receipts"))?;
    fs::set_permissions(f.path("receipts"), fs::Permissions::from_mode(0o755))?;
    f.rejected("before", "root must be private")?;
    Ok(())
}
#[googletest::test]
fn only_explicit_absent_classic_protection_is_an_accepted_read_failure() -> Result<()> {
    let f = Fixture::new()?;
    f.route(
        "branches/main/protection/required_status_checks",
        json!({"exit":1,"body":{"status":"404","message":"Branch not protected"}}),
    )?;
    f.accepted("before")?;
    let f = Fixture::new()?;
    f.route(
        "branches/main/protection/required_status_checks",
        json!({"exit":1,"body":{"status":"403","message":"Forbidden"}}),
    )?;
    f.rejected("before", "protection read failed")?;
    let f = Fixture::new()?;
    f.route(
        "branches/main/protection/required_status_checks",
        response(
            json!({"contexts":["additional"],"checks":[{"context":"additional","app_id":null}]}),
            false,
        ),
    )?;
    f.rejected("before", "required check is missing: additional")?;
    Ok(())
}
#[googletest::test]
fn accepts_later_status_success_and_released_change_requests() -> Result<()> {
    let f = Fixture::new()?;
    f.route(&format!("commits/{HEAD}/statuses?per_page=100"),response(json!([[{"id":2,"context":"external","state":"success"}],[{"id":1,"context":"external","state":"failure"}]]),true))?;
    f.route("pulls/7/reviews?per_page=100",response(json!([[{"id":1,"state":"CHANGES_REQUESTED","user":{"id":42}}],[{"id":2,"state":"DISMISSED","user":{"id":42}},{"id":3,"state":"APPROVED","user":{"id":43}}]]),true))?;
    f.accepted("before")?;
    Ok(())
}
#[googletest::test]
fn empty_or_changing_collections_cannot_accept() -> Result<()> {
    let f = Fixture::new()?;
    f.route("pulls/7/files?per_page=100", response(json!([]), true))?;
    f.rejected("before", "pages are empty")?;
    let f = Fixture::new()?;
    let mut p = f.get("pulls/7")?;
    p["changed_files"] = json!(2);
    f.route("pulls/7", response(p, false))?;
    f.rejected("before", "collection is incomplete")?;
    let f = Fixture::new()?;
    f.graph(json!({"sequence":[response(threads(&json!([]),2,true,&json!("cursor")),false),response(threads(&json!([]),2,true,&json!("cursor")),false)]}))?;
    f.rejected("before", "did not advance")?;
    let f = Fixture::new()?;
    f.graph(response(
        json!({"errors":[{"message":"controlled partial read"}]}),
        false,
    ))?;
    f.rejected("before", "thread read failed")?;
    let f = Fixture::new()?;
    let endpoint = format!("actions/runs?head_sha={HEAD}&per_page=100");
    let mut runs = f.get(&endpoint)?;
    runs[0]["workflow_runs"]
        .as_array_mut()
        .context("runs")?
        .pop();
    runs[0]["total_count"] = json!(1);
    f.route(&endpoint, response(runs, true))?;
    f.rejected("before", "required workflow is missing: Coverage")?;
    Ok(())
}
#[googletest::test]
fn renamed_paths_and_file_modes_preserve_the_review_boundary() -> Result<()> {
    let f = Fixture::new()?;
    f.write("files.json", &json!(["old.rs", "code.rs"]))?;
    f.route(
        "pulls/7/files?per_page=100",
        response(
            json!([[{"filename":"code.rs","previous_filename":"old.rs","status":"renamed"}]]),
            true,
        ),
    )?;
    f.prepare_after()?;
    f.commit_tree(
        PARENT,
        PARENT_TREE,
        &json!([]),
        &[blob("old.rs", OLD), blob("unrelated.md", EXTRA)],
    )?;
    f.accepted("after")?;
    let f = Fixture::new()?;
    f.prepare_after()?;
    let mut changed = blob("code.rs", NEW);
    changed["mode"] = json!("100755");
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[changed, blob("unrelated.md", EXTRA)],
    )?;
    f.rejected("after", "merged file differs")?;
    Ok(())
}
#[googletest::test]
fn invalid_input_and_unmerged_prs_cannot_accept() -> Result<()> {
    for paths in [json!([]), json!(["code.rs", "code.rs"])] {
        let f = Fixture::new()?;
        f.write("files.json", &paths)?;
        f.rejected("before", "reviewed")?;
    }
    let f = Fixture::new()?;
    let v = f.accepted("before")?;
    f.write("before.json", &v)?;
    f.rejected("after", "not merged")?;
    let f = Fixture::new()?;
    f.prepare_after()?;
    let mut v: Value = serde_json::from_slice(&fs::read(f.path("before.json"))?)?;
    v["reviewed_files"] = json!(["other.rs"]);
    f.write("before.json", &v)?;
    f.rejected("after", "files differ")?;
    Ok(())
}

#[googletest::test]
fn incomplete_tree_contents_cannot_accept_a_modified_path_absent_from_both_heads() -> Result<()> {
    let f = Fixture::new()?;
    f.prepare_after()?;
    f.commit_tree(HEAD, HEAD_TREE, &json!([]), &[])?;
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[blob("unrelated.md", EXTRA)],
    )?;
    f.rejected("after", "reviewed tree does not match")?;
    Ok(())
}
#[googletest::test]
fn a_rename_can_reuse_its_old_path_as_an_explicitly_reviewed_addition() -> Result<()> {
    let f = Fixture::new()?;
    f.write("files.json", &json!(["old.rs", "code.rs"]))?;
    let mut p = f.get("pulls/7")?;
    p["changed_files"] = json!(2);
    f.route("pulls/7", response(p, false))?;
    f.route("pulls/7/files?per_page=100",response(json!([[{"filename":"code.rs","previous_filename":"old.rs","status":"renamed"},{"filename":"old.rs","status":"added"}]]),true))?;
    f.prepare_after()?;
    f.commit_tree(
        HEAD,
        HEAD_TREE,
        &json!([]),
        &[blob("code.rs", NEW), blob("old.rs", EXTRA)],
    )?;
    f.commit_tree(
        PARENT,
        PARENT_TREE,
        &json!([]),
        &[blob("old.rs", OLD), blob("unrelated.md", EXTRA)],
    )?;
    f.commit_tree(
        MERGE,
        MERGE_TREE,
        &json!([{"sha":PARENT}]),
        &[
            blob("code.rs", NEW),
            blob("old.rs", EXTRA),
            blob("unrelated.md", EXTRA),
        ],
    )?;
    f.accepted("after")?;
    Ok(())
}
