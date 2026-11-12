//! Complete paginated GitHub reads. No default execution or publication authority.
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use serde_json::{Value, json};

use crate::{
    Result,
    controller::GitHub,
    model::{AppIdentity, Comment, Issue, IssueKey, PROJECT, Snapshot},
    policy, require,
};

pub trait Transport {
    fn request(&mut self, endpoint: &str, body: Option<Value>, paginate: bool) -> Result<Value>;
}
/// Explicit credential source; never inherits a worker's or planning session's token.
pub struct GhTransport {
    command: PathBuf,
    credential: Credential,
}
enum Credential {
    Environment(String),
    Installation(String),
}
impl GhTransport {
    pub fn new(command: PathBuf, token_variable: String) -> Result<Self> {
        require(
            command.is_absolute() && !token_variable.is_empty(),
            "missing approved GitHub command or token source",
        )?;
        Ok(Self {
            command,
            credential: Credential::Environment(token_variable),
        })
    }
}
impl Transport for GhTransport {
    fn request(&mut self, endpoint: &str, body: Option<Value>, paginate: bool) -> Result<Value> {
        let token = match &self.credential {
            Credential::Environment(name) => std::env::var(name).map_err(|_| {
                crate::Error::Stop("dedicated GitHub credential unavailable".into())
            })?,
            Credential::Installation(token) => token.clone(),
        };
        require(!token.is_empty(), "empty GitHub credential")?;
        let mut command = Command::new(&self.command);
        command
            .env("GH_TOKEN", token)
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_HOST")
            .args([
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
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| crate::Error::Stop("missing GitHub input".into()))?;
        if let Some(body) = body {
            let mut input = input;
            input.write_all(&serde_json::to_vec(&body)?)?;
        } else {
            drop(input);
        }
        let result = child.wait_with_output()?;
        require(
            result.status.success(),
            "GitHub request failed; response withheld",
        )?;
        Ok(serde_json::from_slice(&result.stdout)?)
    }
}
/// App JWT transport. The approved executable is curl, not gh: gh's default
/// token scheme cannot authenticate an App JWT. Credentials go through stdin,
/// never command arguments, temporary config files, or diagnostic output.
pub struct AppJwtTransport {
    command: PathBuf,
    token_variable: String,
}
impl AppJwtTransport {
    pub fn new(command: PathBuf, token_variable: String) -> Result<Self> {
        require(
            command.is_absolute() && !token_variable.is_empty(),
            "missing approved App HTTP command or JWT source",
        )?;
        Ok(Self {
            command,
            token_variable,
        })
    }
}
impl Transport for AppJwtTransport {
    fn request(&mut self, endpoint: &str, body: Option<Value>, paginate: bool) -> Result<Value> {
        require(
            !paginate
                && endpoint.starts_with("app")
                && endpoint.split('/').all(|part| {
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }),
            "unsupported App request",
        )?;
        let token = std::env::var(&self.token_variable)
            .map_err(|_| crate::Error::Stop("dedicated App JWT unavailable".into()))?;
        require(
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "invalid App JWT credential",
        )?;
        let mut config = format!(
            "header = \"Authorization: Bearer {token}\"\nheader = \"Accept: application/vnd.github+json\"\nheader = \"Content-Type: application/json\"\n"
        );
        if let Some(body) = &body {
            // Serialized JSON has no literal controls. Its JSON-quoted form uses
            // exactly curl config's quote and backslash escapes.
            config.push_str(&format!(
                "data-binary = {}\n",
                serde_json::to_string(&body.to_string())?
            ));
        }
        let mut child = Command::new(&self.command)
            .env_remove(&self.token_variable)
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .args([
                "--disable",
                "--silent",
                "--show-error",
                "--fail",
                "--connect-timeout",
                "10",
                "--max-time",
                "60",
                "--request",
                if body.is_some() { "POST" } else { "GET" },
                "--url",
                &format!("https://api.github.com/{endpoint}"),
                "--config",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| crate::Error::Stop("missing App HTTP input".into()))?;
        input.write_all(config.as_bytes())?;
        drop(input);
        let result = child.wait_with_output()?;
        require(
            result.status.success(),
            "App HTTP request failed; response withheld",
        )?;
        Ok(serde_json::from_slice(&result.stdout)?)
    }
}
/// Trusted native execution inventory, not a fact derived from labels or comments.
pub trait ExecutionInventory {
    fn clear(&mut self, issue: &IssueKey) -> Result<bool>;
}
pub struct Unprovisioned;
impl crate::store::StoppedOwner for Unprovisioned {
    fn verify_stopped(
        &mut self,
        _: &crate::model::Identity,
        _: &crate::model::Identity,
    ) -> Result<String> {
        Err(crate::Error::Stop(
            "supervised stopped-owner evidence unavailable".into(),
        ))
    }
}
impl crate::controller::Effects for Unprovisioned {
    fn send(&mut self, _: &crate::model::Action) -> Result<()> {
        Err(crate::Error::Stop(
            "native effect prerequisites unavailable".into(),
        ))
    }
    fn inspect_exact(
        &mut self,
        _: &crate::model::Action,
    ) -> Result<crate::controller::Observation> {
        Err(crate::Error::Stop(
            "native reconciliation prerequisites unavailable".into(),
        ))
    }
}
impl crate::controller::Hermes for Unprovisioned {}
impl crate::controller::Pool for Unprovisioned {}
impl crate::controller::Notifications for Unprovisioned {}
impl ExecutionInventory for Unprovisioned {
    fn clear(&mut self, _: &IssueKey) -> Result<bool> {
        Err(crate::Error::Stop(
            "native execution inventory prerequisite unavailable".into(),
        ))
    }
}
/// Provisioning must authenticate the configured App before publication. No broad-token fallback.
pub trait HistoryPublisher {
    fn publish(&mut self, issue: &IssueKey, body: &str, app: &AppIdentity) -> Result<String>;
}
impl HistoryPublisher for Unprovisioned {
    fn publish(&mut self, _: &IssueKey, _: &str, _: &AppIdentity) -> Result<String> {
        Err(crate::Error::Stop(
            "App publication and isolation prerequisites unavailable".into(),
        ))
    }
}
/// A token minted by the authenticated App, not an inherited CLI token.
pub trait InstallationTransportFactory {
    type Client: Transport;
    fn create(&mut self, token: String) -> Result<Self::Client>;
}
pub struct GhInstallationFactory {
    pub command: PathBuf,
}
impl InstallationTransportFactory for GhInstallationFactory {
    type Client = GhTransport;
    fn create(&mut self, token: String) -> Result<GhTransport> {
        require(
            self.command.is_absolute() && !token.is_empty(),
            "missing approved installation transport",
        )?;
        Ok(GhTransport {
            command: self.command.clone(),
            credential: Credential::Installation(token),
        })
    }
}
/// The operator supplies a short-lived App JWT via an explicit credential transport.
/// Each publication obtains a repository-scoped, issues-only installation token.
pub struct AppPublisher<T, F> {
    pub app_transport: T,
    pub installations: F,
    installation: u64,
}
impl<T: Transport, F: InstallationTransportFactory> AppPublisher<T, F> {
    pub fn new(app_transport: T, installations: F, installation: u64) -> Result<Self> {
        require(installation > 0, "missing approved App installation")?;
        Ok(Self {
            app_transport,
            installations,
            installation,
        })
    }
}
impl<T: Transport, F: InstallationTransportFactory> HistoryPublisher for AppPublisher<T, F> {
    fn publish(&mut self, issue: &IssueKey, body: &str, app: &AppIdentity) -> Result<String> {
        require(
            issue.repository() == crate::model::REPOSITORY
                && body.starts_with(crate::history::PREFIX),
            "publication outside history contract",
        )?;
        let authenticated = self.app_transport.request("app", None, false)?;
        require(
            positive_id(&authenticated["id"])? == app.app_id,
            "App JWT authenticated a different App",
        )?;
        let installation = self.app_transport.request(
            &format!("app/installations/{}", self.installation),
            None,
            false,
        )?;
        require(
            positive_id(&installation["app_id"])? == app.app_id
                && installation["id"].as_u64() == Some(self.installation)
                && installation["account"]["login"] == "LowkeyLab"
                && installation.get("suspended_at").is_some_and(Value::is_null),
            "App installation binding unresolved",
        )?;
        let token=self.app_transport.request(&format!("app/installations/{}/access_tokens",self.installation),Some(json!({"repositories":["bazel-repo"],"permissions":{"issues":"write","metadata":"read"}})),false)?;
        require(
            token["permissions"]["issues"] == "write"
                && token["repositories"]
                    .as_array()
                    .is_some_and(|r| r.len() == 1 && r[0]["full_name"] == crate::model::REPOSITORY),
            "installation token scope mismatch",
        )?;
        let permissions = token["permissions"]
            .as_object()
            .ok_or_else(|| crate::Error::Stop("missing installation permissions".into()))?;
        require(
            permissions.keys().all(|p| p == "issues" || p == "metadata"),
            "installation token grants unrelated authority",
        )?;
        let mut client = self.installations.create(text(&token, "token")?)?;
        let created = client.request(
            &format!(
                "repos/{}/issues/{}/comments",
                issue.repository(),
                issue.number()
            ),
            Some(json!({"body":body})),
            false,
        )?;
        require(
            created["body"] == body
                && created["issue_url"]
                    == format!(
                        "https://api.github.com/repos/{}/issues/{}",
                        issue.repository(),
                        issue.number()
                    )
                && positive_id(&created["performed_via_github_app"]["id"])? == app.app_id
                && positive_id(&created["user"]["id"])? == app.actor_id
                && created["user"]["login"] == app.login
                && created["user"]["type"] == "Bot",
            "created history App or target mismatch",
        )?;
        positive_id(&created["id"])
    }
}
pub struct Adapter<T, I, W> {
    pub transport: T,
    pub inventory: I,
    pub publisher: W,
    app: AppIdentity,
    max_pages: usize,
    max_issues: usize,
}
impl<T: Transport, I: ExecutionInventory, W: HistoryPublisher> Adapter<T, I, W> {
    pub fn new(
        transport: T,
        inventory: I,
        publisher: W,
        app: AppIdentity,
        max_pages: usize,
        max_issues: usize,
    ) -> Result<Self> {
        require(
            max_pages > 0 && max_issues > 0,
            "missing approved read bounds",
        )?;
        Ok(Self {
            transport,
            inventory,
            publisher,
            app,
            max_pages,
            max_issues,
        })
    }
    fn graphql(&mut self, query: String, variables: Value) -> Result<Value> {
        let value = self.transport.request(
            "graphql",
            Some(json!({"query":query,"variables":variables})),
            false,
        )?;
        require(
            value
                .get("errors")
                .is_none_or(|e| e.as_array().is_some_and(Vec::is_empty)),
            "partial GraphQL response",
        )?;
        value
            .get("data")
            .cloned()
            .filter(|v| v.is_object())
            .ok_or_else(|| crate::Error::Stop("missing GraphQL data".into()))
    }
    /// Follow cursors and verify stable total counts, unique nodes, and explicit page termination.
    fn connection(
        &mut self,
        node: &str,
        typename: &str,
        field: &str,
        selection: &str,
    ) -> Result<Vec<Value>> {
        let mut cursor = Value::Null;
        let mut cursors = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut all = Vec::new();
        let mut total = None;
        for _ in 0..self.max_pages {
            let query = format!(
                "query($id:ID!,$cursor:String) {{ node(id:$id) {{ ... on {typename} {{ {field}(first:100,after:$cursor) {{ totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ {selection} }} }} }} }} }}"
            );
            let data = self.graphql(query, json!({"id":node,"cursor":cursor}))?;
            let connection = &data["node"][field];
            let count = connection["totalCount"]
                .as_u64()
                .ok_or_else(|| crate::Error::Stop("missing connection count".into()))?;
            require(
                total.is_none_or(|n| n == count),
                "connection changed during pagination",
            )?;
            total = Some(count);
            let values = connection["nodes"]
                .as_array()
                .ok_or_else(|| crate::Error::Stop("missing connection nodes".into()))?;
            for value in values {
                require(!value.is_null(), "inaccessible connection node")?;
                let id = text(value, "id")?;
                require(ids.insert(id), "duplicate connection node")?;
                all.push(value.clone());
            }
            let next = connection["pageInfo"]["hasNextPage"]
                .as_bool()
                .ok_or_else(|| crate::Error::Stop("missing page termination".into()))?;
            if !next {
                require(all.len() as u64 == count, "incomplete connection")?;
                return Ok(all);
            }
            require(!values.is_empty(), "empty nonterminal page")?;
            let end = text(&connection["pageInfo"], "endCursor")?;
            require(cursors.insert(end.clone()), "repeated pagination cursor")?;
            cursor = Value::String(end);
        }
        Err(crate::Error::Stop("read page bound exhausted".into()))
    }
    fn issue_node(&mut self, node: &str) -> Result<Value> {
        let data=self.graphql("query($id:ID!) { node(id:$id) { __typename ... on Issue { id number createdAt state repository { nameWithOwner } parent { id number repository { nameWithOwner } } comments { totalCount } } } }".into(),json!({"id":node}))?;
        let issue = data["node"].clone();
        require(
            issue["__typename"] == "Issue" && text(&issue, "id")? == node,
            "missing or unrelated issue node",
        )?;
        Ok(issue)
    }
    fn lookup(&mut self, key: &IssueKey) -> Result<String> {
        let (owner, name) = key
            .repository()
            .split_once('/')
            .expect("validated repository");
        let data=self.graphql("query($owner:String!,$name:String!,$number:Int!) { repository(owner:$owner,name:$name) { issue(number:$number) { id number repository { nameWithOwner } } } }".into(),json!({"owner":owner,"name":name,"number":key.number()}))?;
        let issue = &data["repository"]["issue"];
        require(issue_key(issue)? == *key, "wrong exact issue lookup")?;
        text(issue, "id")
    }
    fn parse_comment(&self, key: &IssueKey, value: &Value) -> Result<Comment> {
        let id = value["id"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| crate::Error::Stop("missing comment identity".into()))?
            .to_string();
        require(
            text(value, "issue_url")?
                == format!(
                    "https://api.github.com/repos/{}/issues/{}",
                    key.repository(),
                    key.number()
                ),
            "comment target mismatch",
        )?;
        let app = if value["performed_via_github_app"].is_null() {
            None
        } else {
            require(
                value["user"]["type"] == "Bot",
                "App record not authored by Bot",
            )?;
            Some(AppIdentity {
                app_id: positive_id(&value["performed_via_github_app"]["id"])?,
                actor_id: positive_id(&value["user"]["id"])?,
                login: text(&value["user"], "login")?,
            })
        };
        Ok(Comment {
            id,
            issue: key.clone(),
            app,
            body: text_allow_empty(value, "body")?,
        })
    }
}
impl<T: Transport, I: ExecutionInventory, W: HistoryPublisher> GitHub for Adapter<T, I, W> {
    fn snapshot(&mut self) -> Result<Snapshot> {
        let items = self.connection(
            PROJECT,
            "ProjectV2",
            "items",
            "id content { __typename ... on Issue { id number repository { nameWithOwner } } }",
        )?;
        let mut members = Vec::new();
        let mut queue = VecDeque::new();
        for item in items {
            let content = &item["content"];
            match content["__typename"].as_str() {
                Some("Issue") => {
                    let key = issue_key(content)?;
                    if key.repository() == crate::model::REPOSITORY {
                        members.push(key);
                        queue.push_back(text(content, "id")?);
                    }
                }
                Some("PullRequest" | "DraftIssue") => {}
                _ => return Err(crate::Error::Stop("inaccessible Project content".into())),
            }
        }
        let mut issues = BTreeMap::new();
        let mut visited = BTreeSet::new();
        while let Some(node) = queue.pop_front() {
            if !visited.insert(node.clone()) {
                continue;
            }
            require(
                visited.len() <= self.max_issues,
                "native issue closure bound exhausted",
            )?;
            let raw = self.issue_node(&node)?;
            let key = issue_key(&raw)?;
            let parent = if raw["parent"].is_null() {
                None
            } else {
                queue.push_back(text(&raw["parent"], "id")?);
                Some(issue_key(&raw["parent"])?)
            };
            let labels = self
                .connection(&node, "Issue", "labels", "id name")?
                .iter()
                .map(|l| text(l, "name"))
                .collect::<Result<_>>()?;
            let assignees=self.connection(&node,"Issue","assignedActors","... on User { id login } ... on Bot { id login } ... on Mannequin { id login } ... on Organization { id login }")?.iter().map(|a|text(a,"login")).collect::<Result<_>>()?;
            let native = self.connection(
                &node,
                "Issue",
                "subIssues",
                "id number repository { nameWithOwner }",
            )?;
            let mut children = Vec::new();
            for child in native {
                queue.push_back(text(&child, "id")?);
                children.push(issue_key(&child)?);
            }
            let blockers = self.connection(&node, "Issue", "blockedBy", "id state")?;
            let blockers_closed = blockers
                .iter()
                .map(|b| {
                    let state = text(b, "state")?;
                    require(
                        state == "OPEN" || state == "CLOSED",
                        "unknown blocker state",
                    )?;
                    Ok(state == "CLOSED")
                })
                .collect::<Result<_>>()?;
            let state = text(&raw, "state")?;
            require(state == "OPEN" || state == "CLOSED", "unknown issue state")?;
            let issue = Issue {
                execution_clear: self.inventory.clear(&key)?,
                key: key.clone(),
                node,
                created_at: text(&raw, "createdAt")?,
                open: state == "OPEN",
                pull_request: false,
                labels,
                assignees,
                blockers_closed,
                parent,
                children,
            };
            require(
                issues.insert(key, issue).is_none(),
                "conflicting repository-qualified issue identity",
            )?;
        }
        let snapshot = Snapshot {
            project: PROJECT.into(),
            complete: true,
            members,
            issues,
        };
        policy::validate(&snapshot)?;
        Ok(snapshot)
    }
    fn history(&mut self, issue: &IssueKey) -> Result<Vec<Comment>> {
        let node = self.lookup(issue)?;
        let before = self.issue_node(&node)?["comments"]["totalCount"]
            .as_u64()
            .ok_or_else(|| crate::Error::Stop("missing history count".into()))?;
        // The stable GraphQL total proves the required REST page count, including
        // an exactly full last page. Refuse over-budget histories before fetching
        // any comment bodies. Never let the subprocess follow additional pages.
        let page_count = before.div_ceil(100).max(1);
        require(
            page_count <= self.max_pages as u64,
            "history page bound exhausted",
        )?;
        let mut comments = Vec::new();
        let mut ids = BTreeSet::new();
        for page in 1..=page_count {
            let values = self.transport.request(
                &format!(
                    "repos/{}/issues/{}/comments?per_page=100&page={page}",
                    issue.repository(),
                    issue.number()
                ),
                None,
                false,
            )?;
            let values = values
                .as_array()
                .ok_or_else(|| crate::Error::Stop("malformed history page".into()))?;
            require(
                values.len() as u64 == before.saturating_sub((page - 1) * 100).min(100),
                "incomplete or changing history page",
            )?;
            for raw in values {
                let comment = self.parse_comment(issue, raw)?;
                require(ids.insert(comment.id.clone()), "duplicate history comment")?;
                comments.push(comment);
            }
        }
        let after = self.issue_node(&node)?["comments"]["totalCount"]
            .as_u64()
            .ok_or_else(|| crate::Error::Stop("missing history count".into()))?;
        require(
            before == after && comments.len() as u64 == after,
            "incomplete or changing history",
        )?;
        Ok(comments)
    }
    fn publish(&mut self, issue: &IssueKey, body: &str) -> Result<String> {
        self.publisher.publish(issue, body, &self.app)
    }
    fn exact_comment(&mut self, issue: &IssueKey, id: &str) -> Result<Comment> {
        require(
            id.parse::<u64>()
                .is_ok_and(|n| n > 0 && n.to_string() == id),
            "invalid exact comment ID",
        )?;
        let raw = self.transport.request(
            &format!("repos/{}/issues/comments/{id}", issue.repository()),
            None,
            false,
        )?;
        let comment = self.parse_comment(issue, &raw)?;
        require(comment.id == id, "wrong exact comment identity")?;
        Ok(comment)
    }
}
fn text(value: &Value, key: &str) -> Result<String> {
    let text = text_allow_empty(value, key)?;
    require(!text.is_empty(), "empty GitHub field")?;
    Ok(text)
}
fn text_allow_empty(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| crate::Error::Stop(format!("missing GitHub field: {key}")))
}
fn issue_key(value: &Value) -> Result<IssueKey> {
    let repo = text(&value["repository"], "nameWithOwner")?;
    let number = value["number"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| crate::Error::Stop("missing issue number".into()))?;
    format!("{repo}#{number}").try_into()
}
fn positive_id(value: &Value) -> Result<String> {
    value
        .as_u64()
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
        .ok_or_else(|| crate::Error::Stop("missing authenticated App identity".into()))
}
