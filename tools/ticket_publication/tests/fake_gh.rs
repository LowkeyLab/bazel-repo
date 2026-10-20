//! Persistent fake GitHub process for command tests.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.first().map(String::as_str) == Some("api"),
        "expected gh api"
    );
    ensure!(
        args.windows(2).any(|a| a == ["--hostname", "github.com"]),
        "explicit GitHub host required"
    );
    let endpoint = args
        .iter()
        .find(|a| a.starts_with("repos/"))
        .ok_or_else(|| anyhow::anyhow!("missing endpoint"))?;
    let path = PathBuf::from(std::env::var_os("REMOTE_STATE").unwrap());
    let mut state: Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        json!({"creations":0,"issues":[]})
    };
    if state["mode"] == "permission" {
        anyhow::bail!("permission denied");
    }
    let result = if args.windows(2).any(|a| a == ["--method", "POST"]) {
        ensure!(
            args.windows(2).any(|a| a == ["--input", "-"]),
            "JSON stdin required"
        );
        ensure!(
            endpoint == "repos/LowkeyLab/bazel-repo/issues",
            "only issue creation is allowed"
        );
        let request: Value = serde_json::from_reader(io::stdin())?;
        ensure!(
            request.as_object().unwrap().len() == 2,
            "creation must not apply labels or links"
        );
        let number = 3000 + state["creations"].as_u64().unwrap();
        let issue = json!({"number":number,"id":number+10000,"node_id":format!("I_{number}"),"title":request["title"],"body":request["body"],"repository_url":"https://api.github.com/repos/LowkeyLab/bazel-repo","state":"open","labels":[]});
        state["creations"] = json!(state["creations"].as_u64().unwrap() + 1);
        state["issues"].as_array_mut().unwrap().push(issue.clone());
        fs::write(&path, serde_json::to_vec(&state)?)?;
        if state["mode"] == "lose-response" {
            anyhow::bail!("response lost after acceptance");
        }
        issue
    } else if endpoint.starts_with("repos/LowkeyLab/bazel-repo/issues?") {
        ensure!(
            endpoint == "repos/LowkeyLab/bazel-repo/issues?state=all&per_page=100",
            "all states required"
        );
        ensure!(
            args.contains(&"--paginate".to_owned()) && args.contains(&"--slurp".to_owned()),
            "complete paginated reads required"
        );
        if let Some(path) = std::env::var_os("REMOTE_ENUMERATIONS") {
            let mut log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            log.write_all(b"enumeration\n")?;
        }
        if state["mode"] == "incomplete" {
            println!("{{}} ");
            return Ok(());
        }
        let pages: Vec<Value> = state["issues"]
            .as_array()
            .unwrap()
            .chunks(100)
            .map(|page| json!(page))
            .collect();
        if pages.is_empty() {
            json!([[]])
        } else {
            json!(pages)
        }
    } else if endpoint == "repos/LowkeyLab/bazel-repo/issues/2401" {
        json!({"number":2401,"id":12401,"node_id":"I_parent","title":"Original parent","body":"Original body","repository_url":"https://api.github.com/repos/LowkeyLab/bazel-repo","state":"open"})
    } else if endpoint.starts_with("repos/LowkeyLab/bazel-repo/issues/") {
        let number: u64 = endpoint.rsplit('/').next().unwrap().parse()?;
        state["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["number"] == number)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("issue not found"))?
    } else {
        anyhow::bail!("unsupported endpoint")
    };
    println!("{result}");
    Ok(())
}
