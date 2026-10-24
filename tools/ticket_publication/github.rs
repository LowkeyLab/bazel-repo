//! Concrete GitHub process adapter. Request bodies travel through standard input.
use crate::plan::{IssueId, IssueMapping, IssueNumber, NodeId};
use crate::progress::{Context as OperationContext, Operation, Phase, Progress, Step};
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
const REPOSITORY_URL: &str = "https://api.github.com/repos/LowkeyLab/bazel-repo";
#[derive(Clone, Deserialize)]
pub struct Issue {
    number: IssueNumber,
    id: IssueId,
    node_id: NodeId,
    title: String,
    body: Option<String>,
    repository_url: String,
    labels: Option<Vec<Label>>,
    #[serde(default)]
    pull_request: Option<Value>,
}
#[derive(Clone, Deserialize)]
struct Label {
    name: String,
}
impl Issue {
    pub fn labels(&self) -> Result<Vec<String>> {
        Ok(self
            .labels
            .as_ref()
            .context("incomplete issue labels")?
            .iter()
            .map(|label| label.name.clone())
            .collect())
    }
    pub fn has_marker(&self, marker: &str) -> bool {
        self.body
            .as_deref()
            .is_some_and(|body| body.contains(marker))
    }
    pub fn mapping(&self) -> IssueMapping {
        IssueMapping {
            number: self.number.clone(),
            id: self.id.clone(),
            node_id: self.node_id.clone(),
        }
    }
    pub fn check_repository(&self) -> Result<()> {
        ensure!(
            self.repository_url == REPOSITORY_URL && self.pull_request.is_none(),
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

/// Related issue numbers are local to their repository. IDs are global.
pub struct Relationship {
    repository_url: String,
    mapping: IssueMapping,
}
impl Relationship {
    pub fn matches(&self, expected: &IssueMapping) -> Result<bool> {
        if self.mapping.id == expected.id
            || self.mapping.node_id == expected.node_id
            || (self.repository_url == REPOSITORY_URL && self.mapping.number == expected.number)
        {
            ensure!(
                self.repository_url == REPOSITORY_URL && self.mapping == *expected,
                Conflict
            );
            return Ok(true);
        }
        Ok(false)
    }
}

pub struct Client<'a> {
    pub progress: &'a Progress,
    context: OperationContext,
    command: &'a dyn Fn() -> Command,
}
impl<'a> Client<'a> {
    pub fn new(progress: &'a Progress, command: &'a dyn Fn() -> Command) -> Self {
        Self {
            progress,
            context: OperationContext::new(Phase::RemoteChecks),
            command,
        }
    }
    pub fn scope(&self, mut context: OperationContext) -> Self {
        if self.context.phase == Phase::FinalVerification {
            context.phase = Phase::FinalVerification;
        }
        Self {
            progress: self.progress,
            context,
            command: self.command,
        }
    }
    pub fn verified(&self, step: Step) {
        self.progress.verified(self.context.clone(), step);
    }
    pub fn read(&self, number: &IssueNumber) -> Result<Issue> {
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(number.clone())),
        );

        let issue: Issue = serde_json::from_value(scoped.request(
            Operation::ReadIssue,
            &format!("{REPOSITORY}/issues/{number}"),
            None,
            false,
        )?)?;
        issue.check_repository()?;
        ensure!(issue.number == *number, Conflict);
        Ok(issue)
    }
    pub fn create(&self, title: &str, body: &str) -> Result<Issue> {
        let issue: Issue = serde_json::from_value(self.request(
            Operation::CreateIssue,
            &format!("{REPOSITORY}/issues"),
            Some(json!({"title":title,"body":body})),
            false,
        )?)?;
        issue.check_content(title, body)?;
        Ok(issue)
    }
    pub fn find(&self, marker: &str) -> Result<Vec<Issue>> {
        Ok(self
            .enumerate()?
            .into_iter()
            .filter(|issue| issue.has_marker(marker))
            .collect())
    }
    pub fn enumerate(&self) -> Result<Vec<Issue>> {
        let pages = self.request(
            Operation::EnumerateIssues,
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
                matches.push(issue);
            }
        }
        Ok(matches)
    }
    fn request(
        &self,
        operation: Operation,
        endpoint: &str,
        body: Option<Value>,
        paginate: bool,
    ) -> Result<Value> {
        self.progress.started(&self.context, operation);
        let mut command = (self.command)();
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
    /// Enumerate every relationship page. Malformed pages cannot establish absence.
    pub fn relationships(&self, number: &IssueNumber, relation: &str) -> Result<Vec<Relationship>> {
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(number.clone())),
        );

        let pages = scoped.request(
            Operation::ReadRelationships,
            &format!("{REPOSITORY}/issues/{number}/{relation}?per_page=100"),
            None,
            true,
        )?;
        let pages = pages
            .as_array()
            .context("relationship read did not return pages")?;
        ensure!(!pages.is_empty(), "relationship read returned no pages");
        let mut mappings = Vec::new();
        for page in pages {
            for value in page.as_array().context("incomplete relationship page")? {
                let issue: Issue =
                    serde_json::from_value(value.clone()).context("incomplete related issue")?;
                ensure!(issue.pull_request.is_none(), Conflict);
                let repository_path = issue
                    .repository_url
                    .strip_prefix("https://api.github.com/repos/")
                    .context("invalid related repository URL")?;
                let parts: Vec<_> = repository_path.split('/').collect();
                ensure!(
                    parts.len() == 2 && parts.iter().all(|part| !part.is_empty()),
                    "invalid related repository URL"
                );
                let mapping = issue.mapping();
                ensure!(
                    !mappings.iter().any(|m: &Relationship| (m.repository_url
                        == issue.repository_url
                        && m.mapping.number == mapping.number)
                        || m.mapping.id == mapping.id
                        || m.mapping.node_id == mapping.node_id),
                    "duplicate relationship identity"
                );
                mappings.push(Relationship {
                    repository_url: issue.repository_url,
                    mapping,
                });
            }
        }
        Ok(mappings)
    }
    /// An explicit nullable parent avoids treating a REST 404 as proof of absence.
    pub fn parent(&self, mapping: &IssueMapping) -> Result<Option<NodeId>> {
        #[derive(Deserialize)]
        struct Parent {
            id: NodeId,
        }
        #[derive(Deserialize)]
        struct Node {
            id: NodeId,
            parent: Option<Parent>,
        }
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(mapping.number.clone())),
        );

        let response = scoped.request(
            Operation::ReadParent,
            "graphql",
            Some(json!({
                "query":"query($id:ID!) { node(id:$id) { ... on Issue { id parent { id } } } }",
                "variables":{"id":mapping.node_id}
            })),
            false,
        )?;
        ensure!(
            response.get("errors").is_none(),
            "parent query returned errors"
        );
        let value = response
            .pointer("/data/node")
            .context("parent query has no node")?;
        ensure!(value.get("parent").is_some(), "incomplete parent query");
        let node: Node = serde_json::from_value(value.clone())?;
        ensure!(node.id == mapping.node_id, Conflict);
        Ok(node.parent.map(|p| p.id))
    }
    pub fn add_child(&self, parent: &IssueNumber, child: &IssueMapping) -> Result<()> {
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(parent.clone())),
        );

        scoped.request(
            Operation::AddParent,
            &format!("{REPOSITORY}/issues/{parent}/sub_issues"),
            Some(json!({"sub_issue_id":child.id,"replace_parent":false})),
            false,
        )?;
        Ok(())
    }
    pub fn add_blocker(&self, issue: &IssueNumber, blocker: &IssueMapping) -> Result<()> {
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(issue.clone())),
        );

        scoped.request(
            Operation::AddBlocker,
            &format!("{REPOSITORY}/issues/{issue}/dependencies/blocked_by"),
            Some(json!({"issue_id":blocker.id})),
            false,
        )?;
        Ok(())
    }
    pub fn add_labels(&self, number: &IssueNumber, labels: &[String]) -> Result<()> {
        let scoped = self.scope(
            self.context
                .clone()
                .related(crate::progress::Subject::Issue(number.clone())),
        );

        scoped.request(
            Operation::AddLabels,
            &format!("{REPOSITORY}/issues/{number}/labels"),
            Some(json!({"labels":labels})),
            false,
        )?;
        Ok(())
    }
}
