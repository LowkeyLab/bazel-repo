//! Concrete GitHub process adapter. Request bodies travel through standard input.
use crate::plan::{IssueId, IssueMapping, IssueNumber, NodeId};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("remote issue identity or content conflicts with the saved batch")
    }
}
impl std::error::Error for Conflict {}

const REPOSITORY: &str = "repos/LowkeyLab/bazel-repo";
#[derive(Deserialize)]
pub struct Issue {
    number: IssueNumber,
    id: IssueId,
    node_id: NodeId,
    title: String,
    body: Option<String>,
    repository_url: String,
    #[serde(default)]
    pull_request: Option<Value>,
}
impl Issue {
    pub fn mapping(&self) -> IssueMapping {
        IssueMapping {
            number: self.number.clone(),
            id: self.id.clone(),
            node_id: self.node_id.clone(),
        }
    }
    pub fn check_repository(&self) -> Result<()> {
        ensure!(
            self.repository_url == "https://api.github.com/repos/LowkeyLab/bazel-repo"
                && self.pull_request.is_none(),
            Conflict
        );
        Ok(())
    }
    pub fn check_content(&self, title: &str, body: &str) -> Result<()> {
        self.check_repository()?;
        ensure!(
            self.title == title && self.body.as_deref() == Some(body),
            Conflict
        );
        Ok(())
    }
}

pub fn read(number: &IssueNumber) -> Result<Issue> {
    let issue: Issue = serde_json::from_value(request(
        &format!("{REPOSITORY}/issues/{number}"),
        None,
        false,
    )?)?;
    issue.check_repository()?;
    ensure!(issue.number == *number, Conflict);
    Ok(issue)
}
pub fn create(title: &str, body: &str) -> Result<Issue> {
    let issue: Issue = serde_json::from_value(request(
        &format!("{REPOSITORY}/issues"),
        Some(json!({"title":title,"body":body})),
        false,
    )?)?;
    issue.check_content(title, body)?;
    Ok(issue)
}
pub fn find(marker: &str) -> Result<Vec<Issue>> {
    let pages = request(
        &format!("{REPOSITORY}/issues?state=all&per_page=100"),
        None,
        true,
    )?;
    let pages = pages
        .as_array()
        .context("issue enumeration did not return pages")?;
    ensure!(!pages.is_empty(), "issue enumeration returned no pages");
    let mut matches = Vec::new();
    for page in pages {
        for value in page
            .as_array()
            .context("incomplete issue enumeration page")?
        {
            // GitHub shares the issue enumeration endpoint with pull requests.
            if value.get("pull_request").is_some() {
                continue;
            }
            let issue: Issue =
                serde_json::from_value(value.clone()).context("incomplete enumerated issue")?;
            issue.check_repository()?;
            if issue
                .body
                .as_deref()
                .is_some_and(|body| body.contains(marker))
            {
                matches.push(issue);
            }
        }
    }
    Ok(matches)
}
fn request(endpoint: &str, body: Option<Value>, paginate: bool) -> Result<Value> {
    let mut command = Command::new("gh");
    command.args([
        "api",
        "--hostname",
        "github.com",
        "--method",
        if body.is_some() { "POST" } else { "GET" },
        endpoint,
    ]);
    if paginate {
        command.args(["--paginate", "--slurp"]);
    }
    if body.is_some() {
        command.args(["--input", "-"]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot start gh api")?;
    let input = if let Some(body) = body {
        serde_json::to_writer(child.stdin.take().context("missing gh input")?, &body)
            .context("cannot write gh request")
    } else {
        drop(child.stdin.take());
        Ok(())
    };
    let output = child.wait_with_output()?;
    input?;
    // Do not copy process output into diagnostics or durable state.
    ensure!(output.status.success(), "gh api request failed");
    serde_json::from_slice(&output.stdout).context("gh api returned invalid JSON")
}
