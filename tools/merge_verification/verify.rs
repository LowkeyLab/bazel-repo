//! Fail-closed verification policy for the repository delivery workflow.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use crate::{Cli, github::Github};

pub fn commit(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid full commit identity"
    );
    Ok(())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("missing string field: {key}"))
}
fn count(v: &Value, key: &str) -> Result<u64> {
    v[key]
        .as_u64()
        .with_context(|| format!("missing count field: {key}"))
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v[key]
        .as_array()
        .with_context(|| format!("missing array field: {key}"))
}
fn accepted(v: &Value) -> Result<()> {
    ensure!(
        text(v, "status")? == "completed"
            && matches!(text(v, "conclusion")?, "success" | "neutral" | "skipped"),
        "check or workflow is incomplete or failed"
    );
    Ok(())
}
fn optional_app(v: &Value, key: &str) -> Result<Option<u64>> {
    let Some(value) = v.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    if value.as_i64() == Some(-1) {
        return Ok(None);
    }
    let id = value
        .as_u64()
        .context("invalid check application identity")?;
    ensure!(id > 0, "invalid check application identity");
    Ok(Some(id))
}
fn pull(cli: &Cli, gh: &mut Github) -> Result<Value> {
    let p = gh.read(&format!("repos/{}/pulls/{}", cli.repo, cli.pr))?;
    ensure!(
        count(&p, "number")? == cli.pr
            && text(&p["base"]["repo"], "full_name")?.eq_ignore_ascii_case(&cli.repo),
        "PR repository or number differs"
    );
    ensure!(text(&p["head"], "sha")? == cli.head, "PR head changed");
    ensure!(
        text(&p["base"], "ref")? == "main",
        "unsupported target branch policy"
    );
    commit(text(&p["base"], "sha")?)?;
    Ok(p)
}
fn files(cli: &Cli, gh: &mut Github, p: &Value, expected: &[String]) -> Result<Vec<Value>> {
    let rows = gh.pages(
        &format!("repos/{}/pulls/{}/files?per_page=100", cli.repo, cli.pr),
        None,
    )?;
    ensure!(
        count(p, "changed_files")? == rows.len() as u64,
        "changed file collection is incomplete"
    );
    let mut actual = BTreeSet::new();
    let mut names = BTreeSet::new();
    for row in &rows {
        let name = text(row, "filename")?.to_owned();
        ensure!(names.insert(name.clone()), "duplicate changed file");
        actual.insert(name);
        if text(row, "status")? == "renamed" {
            actual.insert(text(row, "previous_filename")?.to_owned());
        }
    }
    ensure!(
        actual == expected.iter().cloned().collect(),
        "reviewed changed files differ"
    );
    Ok(rows)
}
fn hosted(comments: &[Value], head: &str) -> Result<Value> {
    let mut latest = None;
    for c in comments {
        let user = &c["user"];
        if user["id"].as_u64() != Some(199_175_422)
            || user["login"].as_str() != Some("chatgpt-codex-connector[bot]")
            || user["type"].as_str() != Some("Bot")
        {
            continue;
        }
        let Some((_, rest)) = text(c, "body")?.split_once("<!-- codex-review-state:v1 ") else {
            continue;
        };
        let (raw, _) = rest
            .split_once(" -->")
            .context("invalid hosted review marker")?;
        let value: Value = serde_json::from_str(raw)?;
        let state = &value["code_review"];
        if text(state, "commit_sha")? != head {
            continue;
        }
        let order = (
            chrono::DateTime::parse_from_rfc3339(text(c, "created_at")?)?.timestamp(),
            count(c, "id")?,
        );
        if latest.as_ref().is_none_or(|(old, _)| order > *old) {
            latest = Some((order, state.clone()));
        }
    }
    let (_, state) = latest.context("authenticated review for this head is missing")?;
    ensure!(
        text(&state, "status")? == "completed"
            && state["has_any_findings"].as_bool() == Some(false)
            && state["has_unresolved_findings"].as_bool() == Some(false),
        "hosted review is incomplete or has findings"
    );
    Ok(state)
}
fn reviews(rows: &[Value]) -> Result<()> {
    let mut latest = BTreeMap::new();
    for review in rows {
        let state = text(review, "state")?;
        if state == "COMMENTED" || state == "PENDING" {
            continue;
        }
        ensure!(
            matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED"),
            "unknown submitted review state"
        );
        let id = count(review, "id")?;
        let user = count(&review["user"], "id")?;
        if latest.get(&user).is_none_or(|(previous, _)| id > *previous) {
            latest.insert(user, (id, state));
        }
    }
    ensure!(
        !latest
            .values()
            .any(|(_, state)| *state == "CHANGES_REQUESTED"),
        "active change request"
    );
    Ok(())
}
fn checks(cli: &Cli, gh: &mut Github, expected: &[String]) -> Result<Value> {
    let base = format!("repos/{}", cli.repo);
    let checks = gh.pages(
        &format!("{base}/commits/{}/check-runs?per_page=100", cli.head),
        Some("check_runs"),
    )?;
    ensure!(!checks.is_empty(), "published checks are missing");
    for check in &checks {
        ensure!(text(check, "head_sha")? == cli.head, "check head differs");
        accepted(check)?;
    }
    let statuses = gh.pages(
        &format!("{base}/commits/{}/statuses?per_page=100", cli.head),
        None,
    )?;
    let mut latest = BTreeMap::new();
    for status in &statuses {
        let context = text(status, "context")?;
        let id = count(status, "id")?;
        if latest.get(context).is_none_or(|(old, _)| id > *old) {
            latest.insert(context, (id, status));
        }
    }
    for (_, status) in latest.values() {
        ensure!(
            text(status, "state")? == "success",
            "commit status is incomplete or failed"
        );
    }
    let rules = gh.pages(&format!("{base}/rules/branches/main?per_page=100"), None)?;
    let classic = gh.classic(&format!(
        "{base}/branches/main/protection/required_status_checks"
    ))?;
    let mut required: Vec<(String, Option<u64>)> = vec![
        ("test".into(), Some(15_368)),
        ("lint".into(), Some(15_368)),
        ("format".into(), Some(15_368)),
    ];
    for rule in &rules {
        if text(rule, "type")? == "required_status_checks" {
            for check in list(&rule["parameters"], "required_status_checks")? {
                required.push((
                    text(check, "context")?.into(),
                    optional_app(check, "integration_id")?,
                ));
            }
        }
    }
    for context in list(&classic, "contexts")? {
        required.push((
            context.as_str().context("invalid required context")?.into(),
            None,
        ));
    }
    for check in list(&classic, "checks")? {
        required.push((
            text(check, "context")?.into(),
            optional_app(check, "app_id")?,
        ));
    }
    let docs = expected.iter().all(|f| {
        std::path::Path::new(f)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    });
    if !docs {
        required.push(("codecov/patch".into(), Some(254)));
    }
    for (context, app) in &required {
        let published = checks.iter().any(|c| {
            c["name"].as_str() == Some(context)
                && app.is_none_or(|id| c["app"]["id"].as_u64() == Some(id))
        });
        ensure!(
            published || (app.is_none() && latest.contains_key(context.as_str())),
            "required check is missing: {context}"
        );
    }
    let workflows = gh.pages(
        &format!("{base}/actions/runs?head_sha={}&per_page=100", cli.head),
        Some("workflow_runs"),
    )?;
    for run in &workflows {
        ensure!(text(run, "head_sha")? == cli.head, "workflow head differs");
        accepted(run)?;
    }
    for name in ["Bazel Tests", "Coverage"] {
        ensure!(
            workflows.iter().any(|v| v["name"].as_str() == Some(name)),
            "required workflow is missing: {name}"
        );
    }
    Ok(
        json!({"checks":checks.len(),"statuses":latest.len(),"workflows":workflows.len(),"documentation_only":docs,"required_checks":required}),
    )
}
pub fn before(cli: &Cli, expected: &[String], gh: &mut Github) -> Result<Value> {
    let initial = pull(cli, gh)?;
    ensure!(
        text(&initial, "state")? == "open"
            && initial["merged"].as_bool() == Some(false)
            && initial["mergeable"].as_bool() == Some(true),
        "PR is closed, merged, or not confirmed mergeable"
    );
    files(cli, gh, &initial, expected)?;
    let checks = checks(cli, gh, expected)?;
    let base = format!("repos/{}", cli.repo);
    reviews(&gh.pages(
        &format!("{base}/pulls/{}/reviews?per_page=100", cli.pr),
        None,
    )?)?;
    let review = hosted(
        &gh.pages(
            &format!("{base}/issues/{}/comments?per_page=100", cli.pr),
            None,
        )?,
        &cli.head,
    )?;
    for thread in gh.threads(cli.pr)? {
        ensure!(
            thread["isResolved"].as_bool() == Some(true),
            "unresolved review thread"
        );
    }
    let final_pull = pull(cli, gh)?;
    ensure!(
        final_pull["state"] == initial["state"]
            && final_pull["base"]["sha"] == initial["base"]["sha"]
            && final_pull["mergeable"].as_bool() == Some(true)
            && final_pull["merged"].as_bool() == Some(false),
        "PR changed during verification"
    );
    Ok(
        json!({"reviewed_files":expected,"checks":checks,"hosted_review":review,"target_base":initial["base"]["sha"]}),
    )
}
#[derive(PartialEq, Eq)]
struct Blob {
    mode: String,
    kind: String,
    sha: String,
}
fn tree(cli: &Cli, gh: &mut Github, sha: &str) -> Result<BTreeMap<String, Blob>> {
    commit(sha)?;
    let commit_value = gh.read(&format!("repos/{}/git/commits/{sha}", cli.repo))?;
    ensure!(
        text(&commit_value, "sha")? == sha,
        "commit response identity differs"
    );
    let tree_sha = text(&commit_value["tree"], "sha")?;
    commit(tree_sha)?;
    let value = gh.read(&format!(
        "repos/{}/git/trees/{tree_sha}?recursive=1",
        cli.repo
    ))?;
    ensure!(
        text(&value, "sha")? == tree_sha && value["truncated"].as_bool() == Some(false),
        "tree identity differs or tree is incomplete"
    );
    let mut result = BTreeMap::new();
    for row in list(&value, "tree")? {
        let kind = text(row, "type")?;
        if kind == "tree" {
            continue;
        }
        ensure!(
            matches!(kind, "blob" | "commit"),
            "unknown Git tree entry type"
        );
        let sha = text(row, "sha")?;
        commit(sha)?;
        let entry = Blob {
            mode: text(row, "mode")?.into(),
            kind: kind.into(),
            sha: sha.into(),
        };
        ensure!(
            result.insert(text(row, "path")?.into(), entry).is_none(),
            "duplicate tree path"
        );
    }
    Ok(result)
}
pub fn after(cli: &Cli, expected: &[String], gh: &mut Github) -> Result<Value> {
    let before: Value = serde_json::from_slice(&fs::read(
        cli.before_receipt
            .as_ref()
            .context("before receipt is required")?,
    )?)?;
    ensure!(
        before["schema_version"].as_u64() == Some(1)
            && before["passed"].as_bool() == Some(true)
            && before["phase"] == "before"
            && before["head"] == cli.head
            && before["repository"]
                .as_str()
                .is_some_and(|r| r.eq_ignore_ascii_case(&cli.repo))
            && before["pr"].as_u64() == Some(cli.pr),
        "before receipt does not accept this identity"
    );
    gh.save("before-result.json", &before)?;
    let mut saved: Vec<String> = serde_json::from_value(before["reviewed_files"].clone())?;
    saved.sort();
    let mut ordered = expected.to_vec();
    ordered.sort();
    ensure!(saved == ordered, "before receipt files differ");
    let initial = pull(cli, gh)?;
    ensure!(
        text(&initial, "state")? == "closed" && initial["merged"].as_bool() == Some(true),
        "PR is not merged"
    );
    let changes = files(cli, gh, &initial, expected)?;
    let merge = text(&initial, "merge_commit_sha")?;
    commit(merge)?;
    let commit_value = gh.read(&format!("repos/{}/git/commits/{merge}", cli.repo))?;
    ensure!(
        text(&commit_value, "sha")? == merge,
        "merge commit identity differs"
    );
    let parents = list(&commit_value, "parents")?;
    ensure!(
        parents.len() == 1,
        "repository policy requires a squash commit"
    );
    let reviewed = tree(cli, gh, &cli.head)?;
    for change in &changes {
        let present = reviewed.contains_key(text(change, "filename")?);
        ensure!(
            present != (text(change, "status")? == "removed"),
            "reviewed tree does not match the file change"
        );
    }
    let merged = tree(cli, gh, merge)?;
    let parent = tree(cli, gh, text(&parents[0], "sha")?)?;
    for file in expected {
        ensure!(
            reviewed.get(file) == merged.get(file),
            "merged file differs from reviewed file: {file}"
        );
    }
    let committed_paths: BTreeSet<_> = parent
        .keys()
        .chain(merged.keys())
        .filter(|path| parent.get(*path) != merged.get(*path))
        .cloned()
        .collect();
    let expected_set: BTreeSet<_> = expected.iter().cloned().collect();
    ensure!(
        committed_paths.is_subset(&expected_set),
        "merge changed files outside reviewed scope"
    );
    let final_pull = pull(cli, gh)?;
    ensure!(
        final_pull["merged"].as_bool() == Some(true) && final_pull["merge_commit_sha"] == merge,
        "merge identity changed during verification"
    );
    Ok(
        json!({"reviewed_files":expected,"merge_commit":merge,"changed_files":committed_paths,"already_present":expected_set.difference(&committed_paths).collect::<Vec<_>>(),"merged_blobs_match":true}),
    )
}
