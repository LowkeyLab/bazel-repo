//! Request and poll hosted reviews without duplicate uncertain sends.
mod github;
mod store;

use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use serde_json::{Value, json};

use github::{Github, review, timestamp};
use policy::{Decision, Review, decide};
use store::{Commit, Request, RequestId, Store};

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    Request,
    Poll,
    Track,
}
#[derive(Parser)]
#[command(about = "Request or poll one hosted review for an exact PR head")]
struct Cli {
    #[arg(value_enum)]
    action: Action,
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr: u64,
    #[arg(long)]
    head: String,
    /// Supply UTC Unix seconds for this observation.
    #[arg(long)]
    now: u64,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(long, required_if_eq("action", "track"))]
    request_comment: Option<u64>,
}
fn state_root(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME").filter(|p| !p.is_empty()) {
        ensure!(
            PathBuf::from(&path).is_absolute(),
            "XDG_STATE_HOME must be absolute"
        );
        return Ok(PathBuf::from(path).join("hosted-review"));
    }
    let home =
        PathBuf::from(std::env::var_os("HOME").context("HOME is missing; specify --state-dir")?);
    ensure!(home.is_absolute(), "HOME must be absolute");
    Ok(home.join(".local/state/hosted-review"))
}
fn execute(mut cli: Cli) -> Result<Value> {
    cli.repo = cli.repo.to_ascii_lowercase();
    let pieces: Vec<_> = cli.repo.split('/').collect();
    ensure!(
        pieces.len() == 2
            && pieces.iter().all(|v| !v.is_empty()
                && *v != "."
                && *v != ".."
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))),
        "repository must be owner/name"
    );
    ensure!(cli.pr > 0, "PR number must be positive");
    let head = Commit::try_from(cli.head)?;
    let store = Store::open(&state_root(cli.state_dir)?, &cli.repo, cli.pr)?;
    let mut saved = store.read(&cli.repo, cli.pr)?;
    let gh = Github::new(&cli.repo, cli.pr);
    let pull = gh.pull()?;
    if pull.state != "open" {
        return Ok(json!({"status":"closed"}));
    }
    if pull.head.sha != head.as_str() {
        return Ok(json!({"status":"head_changed", "head":pull.head.sha}));
    }
    let comments = gh.comments()?;
    let status = review(&comments, head.as_str())?;
    if status == Review::Complete {
        return Ok(json!({"status":"complete", "head":head.as_str()}));
    }
    let actor = gh.actor()?;
    ensure!(actor.id > 0, "invalid authenticated caller identity");
    let index = saved
        .requests
        .iter()
        .rposition(|r| r.head.as_str() == head.as_str());
    if matches!(cli.action, Action::Track) {
        if index.is_some() {
            return Ok(json!({"status":"already_tracked"}));
        }
        ensure!(
            status == Review::Incomplete,
            "tracking requires an authenticated incomplete review for this head"
        );
        let id = cli.request_comment.context("request comment is required")?;
        let comment = comments
            .iter()
            .find(|c| c.id == id)
            .context("request comment is missing")?;
        ensure!(
            comment.user.id == actor.id
                && comment.body.contains("@codex review")
                && comment
                    .body
                    .contains(&format!("Review commit `{}`.", head.as_str())),
            "request comment must belong to this caller and explicitly identify this commit"
        );
        let at = timestamp(&comment.created_at)?;
        ensure!(
            cli.now >= at,
            "request comment is later than the observation time"
        );
        saved.requests.push(Request {
            id: RequestId::new(),
            head,
            at,
            actor: actor.id,
            comment: Some(id),
        });
        store.save(&saved)?;
        return Ok(json!({"status":"tracked", "comment_id":id, "requested_at":at}));
    }
    ensure!(
        cli.request_comment.is_none(),
        "request-comment is only valid with track"
    );
    let known_comments: Vec<_> = saved.requests.iter().filter_map(|r| r.comment).collect();
    if let Some(index) = index {
        let request = &mut saved.requests[index];
        ensure!(
            request.actor == actor.id,
            "authenticated caller differs from saved request owner"
        );
        if request.comment.is_none() {
            let matches: Vec<_> = comments
                .iter()
                .filter(|c| c.user.id == request.actor && c.body == request.body())
                .collect();
            ensure!(
                matches.len() <= 1,
                "multiple comments match one request; inspect remote state"
            );
            if let Some(comment) = matches.first() {
                request.comment = Some(comment.id);
                request.at = request.at.max(timestamp(&comment.created_at)?);
                store.save(&saved)?;
                return Ok(
                    json!({"status":"reconciled", "request_id":saved.requests[index].id, "comment_id":comment.id}),
                );
            }
        }
        // An external request has no verified head binding. Do not guess its timeout.
        if comments.iter().any(|c| {
            c.user.kind != "Bot"
                && !known_comments.contains(&c.id)
                && c.body.contains("@codex review")
                && timestamp(&c.created_at).is_ok_and(|at| at >= request.at)
                && Some(c.id) != request.comment
                && c.body != request.body()
        }) {
            return Ok(json!({"status":"external_request", "head":head.as_str()}));
        }
    }
    let last = index.map(|i| (saved.requests[i].at, saved.requests[i].comment.is_some()));
    let decision = decide(true, status, last, cli.now);
    let initial = matches!(cli.action, Action::Request) && index.is_none();
    if initial && status != Review::Missing {
        return Ok(json!({"status":"untracked_head"}));
    }
    if initial
        && comments.iter().any(|c| {
            c.user.kind != "Bot"
                && !known_comments.contains(&c.id)
                && c.body.contains("@codex review")
        })
    {
        return Ok(json!({"status":"external_request"}));
    }

    if !initial && decision != Decision::Request {
        return Ok(
            json!({"status":match decision { Decision::Complete=>"complete", Decision::Closed=>"closed", Decision::Untracked=>"untracked_head", Decision::Uncertain=>"uncertain", _=>"waiting" }, "head":head.as_str()}),
        );
    }
    let request = Request {
        id: RequestId::new(),
        head,
        at: cli.now,
        actor: actor.id,
        comment: None,
    };
    let body = request.body();
    saved.requests.push(request);
    store.save(&saved)?; // Commit intent before the outgoing effect.
    let current = gh.pull()?;
    if current.state != "open"
        || current.head.sha
            != saved
                .requests
                .last()
                .context("saved request is missing")?
                .head
                .as_str()
    {
        return Ok(json!({"status":"changed_before_send"}));
    }
    let fresh_comments = gh.comments()?;
    let fresh_review = review(&fresh_comments, &current.head.sha)?;
    if fresh_review == Review::Complete {
        return Ok(json!({"status":"complete", "head":current.head.sha}));
    }
    if !initial && fresh_review != Review::Incomplete {
        return Ok(json!({"status":"provider_unavailable_before_send"}));
    }
    if fresh_comments.iter().any(|c| {
        c.user.kind != "Bot"
            && c.body.contains("@codex review")
            && !comments.iter().any(|old| old.id == c.id)
    }) {
        return Ok(json!({"status":"external_request"}));
    }
    let comment = gh.send(&body)?;
    ensure!(
        comment.id > 0 && comment.user.id == actor.id && comment.body == body,
        "review request response is uncertain"
    );
    let request = saved
        .requests
        .last_mut()
        .context("saved request is missing")?;
    request.comment = Some(comment.id);
    request.at = request.at.max(timestamp(&comment.created_at)?);
    let result = json!({"status":"requested", "head":current.head.sha, "request_id":request.id, "comment_id":comment.id, "requested_at":request.at});
    store.save(&saved)?;
    Ok(result)
}
fn main() -> Result<()> {
    let result = execute(Cli::parse())?;
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, &result)?;
    output.write_all(b"\n")?;
    Ok(())
}
