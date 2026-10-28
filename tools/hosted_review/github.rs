//! Read GitHub evidence and submit one marked review request through gh.
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;

use policy::Review;

#[derive(Deserialize)]
pub struct User {
    pub id: u64,
    pub login: String,
    #[serde(rename = "type")]
    pub kind: String,
}
#[derive(Deserialize)]
pub struct Comment {
    pub id: u64,
    pub user: User,
    pub body: String,
    pub created_at: String,
}
#[derive(Deserialize)]
pub struct Pull {
    pub state: String,
    pub head: Head,
}
#[derive(Deserialize)]
pub struct Head {
    pub sha: String,
}

pub struct Github {
    base: String,
}
impl Github {
    pub fn new(repository: &str, pr: u64) -> Self {
        Self {
            base: format!("repos/{repository}/issues/{pr}"),
        }
    }
    fn read<T: serde::de::DeserializeOwned>(&self, endpoint: &str, pages: bool) -> Result<T> {
        let mut command = Command::new("gh");
        command.args(["api", "--hostname", "github.com", endpoint]);
        if pages {
            command.args(["--paginate", "--slurp"]);
        }
        let output = command.output().context("cannot start gh")?;
        ensure!(
            output.status.success(),
            "GitHub read failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).context("invalid GitHub read response")
    }
    pub fn actor(&self) -> Result<User> {
        self.read("user", false)
    }
    pub fn pull(&self) -> Result<Pull> {
        self.read(&self.base.replace("/issues/", "/pulls/"), false)
    }
    pub fn comments(&self) -> Result<Vec<Comment>> {
        let pages: Vec<Vec<Comment>> =
            self.read(&format!("{}/comments?per_page=100", self.base), true)?;
        let comments: Vec<Comment> = pages.into_iter().flatten().collect();
        for comment in &comments {
            ensure!(
                comment.id > 0 && comment.user.id > 0,
                "invalid GitHub comment identity"
            );
            timestamp(&comment.created_at)?;
        }
        Ok(comments)
    }
    pub fn send(&self, body: &str) -> Result<Comment> {
        let output = Command::new("gh")
            .args([
                "api",
                "--hostname",
                "github.com",
                &format!("{}/comments", self.base),
                "--method",
                "POST",
                "--raw-field",
                &format!("body={body}"),
            ])
            .output()
            .context("review request outcome is uncertain")?;
        ensure!(
            output.status.success(),
            "review request outcome is uncertain: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).context("review request response is uncertain")
    }
}

/// The bot identity comes from authenticated GitHub response fields, not body text.
pub fn review(comments: &[Comment], head: &str) -> Result<Review> {
    let mut result = Review::Missing;
    let mut latest = None;
    for c in comments {
        if c.user.id != 199175422
            || c.user.login != "chatgpt-codex-connector[bot]"
            || c.user.kind != "Bot"
        {
            continue;
        }
        let Some((_, rest)) = c.body.split_once("<!-- codex-review-state:v1 ") else {
            continue;
        };
        let (json, _) = rest
            .split_once(" -->")
            .context("invalid hosted review marker")?;
        let value: Value = serde_json::from_str(json).context("invalid hosted review state")?;
        let state = value
            .get("code_review")
            .context("missing code review state")?;
        if state.get("commit_sha").and_then(Value::as_str) != Some(head) {
            continue;
        }
        let at = timestamp(&c.created_at)?;
        if latest.is_some_and(|previous| previous > (at, c.id)) {
            continue;
        }
        result = match state.get("status").and_then(Value::as_str) {
            Some("completed") => Review::Complete,
            Some("running" | "queued" | "pending" | "in_progress") => Review::Incomplete,
            _ => anyhow::bail!("unknown hosted review status"),
        };
        latest = Some((at, c.id));
    }
    Ok(result)
}
pub fn timestamp(value: &str) -> Result<u64> {
    let at = chrono::DateTime::parse_from_rfc3339(value)
        .context("invalid GitHub timestamp")?
        .timestamp();
    at.try_into()
        .context("GitHub timestamp is before the Unix epoch")
}
