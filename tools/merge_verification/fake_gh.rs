//! Controlled read-only GitHub API responses for command integration tests.
use std::{fs, io::Write};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
fn main() -> Result<()> {
    let path = std::env::var_os("VERIFICATION_REMOTE").context("remote fixture")?;
    let mut state: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.first().map(String::as_str) == Some("api")
            && args.get(1).map(String::as_str) == Some("--hostname")
            && args.get(2).map(String::as_str) == Some("github.com"),
        "unexpected command"
    );
    let endpoint = args.get(3).context("endpoint")?;
    ensure!(
        !args
            .iter()
            .any(|v| matches!(v.as_str(), "--method" | "POST" | "PATCH" | "DELETE")),
        "mutation request"
    );
    if endpoint == "graphql" {
        let query = args
            .iter()
            .find_map(|v| v.strip_prefix("query="))
            .context("query")?;
        ensure!(
            query.starts_with("query ") && !query.contains("mutation"),
            "unexpected GraphQL operation"
        );
    } else {
        ensure!(
            !args.iter().any(|v| v == "-f" || v == "-F"),
            "unexpected REST fields"
        );
    }
    let index = usize::try_from(state["counts"][endpoint].as_u64().unwrap_or(0))?;
    let route = state["routes"][endpoint].clone();
    ensure!(!route.is_null(), "unknown endpoint: {endpoint}");
    let response = if let Some(sequence) = route.get("sequence") {
        sequence
            .as_array()
            .context("sequence")?
            .get(index)
            .context("exhausted responses")?
            .clone()
    } else {
        route
    };
    if response["pages"].as_bool() == Some(true) {
        ensure!(
            args.iter().any(|v| v == "--paginate") && args.iter().any(|v| v == "--slurp"),
            "pagination omitted"
        );
    }
    state["counts"][endpoint] = json!(index + 1);
    state["calls"]
        .as_array_mut()
        .context("calls")?
        .push(json!(args));
    fs::write(path, serde_json::to_vec(&state)?)?;
    let mut out = std::io::stdout().lock();
    if let Some(raw) = response["raw"].as_str() {
        out.write_all(raw.as_bytes())?;
    } else {
        serde_json::to_writer(&mut out, &response["body"])?;
    }
    out.write_all(b"\n")?;
    std::process::exit(i32::try_from(response["exit"].as_i64().unwrap_or(0))?);
}
