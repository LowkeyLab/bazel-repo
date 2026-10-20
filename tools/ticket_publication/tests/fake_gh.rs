//! Persistent fake GitHub process for command tests.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
fn main() -> Result<()> {
    if let Some(path) = std::env::var_os("REMOTE_REQUESTS") {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(b"request\n")?;
        let count = fs::read_to_string(path)?.lines().count();
        let limit: usize = std::env::var("REMOTE_REQUEST_LIMIT")?.parse()?;
        ensure!(count <= limit, "request budget exhausted");
    }
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
        .find(|a| a.starts_with("repos/") || a.as_str() == "graphql")
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
    let result = if endpoint == "graphql" {
        parent_response(&state)?
    } else if endpoint.contains("/sub_issues") || endpoint.contains("/dependencies/") {
        relationship_response(endpoint, &args, &mut state, &path)?
    } else if endpoint.ends_with("/labels") {
        label_response(endpoint, &mut state, &path)?
    } else if args.windows(2).any(|a| a == ["--method", "POST"]) {
        create_response(endpoint, &args, &mut state, &path)?
    } else if endpoint.starts_with("repos/LowkeyLab/bazel-repo/issues?") {
        enumeration_response(endpoint, &args, &state)?
    } else if endpoint == "repos/LowkeyLab/bazel-repo/issues/2401" {
        issue(&state, 2401)?
    } else if endpoint.starts_with("repos/LowkeyLab/bazel-repo/issues/") {
        let number: u64 = endpoint.rsplit('/').next().unwrap().parse()?;
        issue(&state, number)?
    } else {
        anyhow::bail!("unsupported endpoint")
    };
    let mut result = result;
    if endpoint.starts_with("repos/LowkeyLab/bazel-repo/issues/")
        && !endpoint.contains("/labels")
        && !endpoint.contains("/sub_issues")
        && !endpoint.contains("/dependencies/")
        && state["label_mutations"].as_u64().unwrap_or(0) > 0
    {
        if state["mode"] == "label-incomplete" {
            result.as_object_mut().unwrap().remove("labels");
        }
        if state["mode"] == "label-final-read" {
            let reads = state["label_reads"].as_u64().unwrap_or(0) + 1;
            state["label_reads"] = json!(reads);
            fs::write(&path, serde_json::to_vec(&state)?)?;
            ensure!(reads < 3, "final issue read failed");
        }
    }
    println!("{result}");
    Ok(())
}

fn issue(state: &Value, number: u64) -> Result<Value> {
    if let Some(value) = state["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["number"] == number)
    {
        return Ok(value.clone());
    }
    ensure!(number == 2401 || number == 2392, "issue not found");
    Ok(
        json!({"number":number,"id":number+10000,"node_id":if number == 2401 {"I_parent"} else {"I_external"},"title":"Original parent","body":"Original body","repository_url":"https://api.github.com/repos/LowkeyLab/bazel-repo","state":"open","labels":[{"name":"keep"}]}),
    )
}

fn parent_response(state: &Value) -> Result<Value> {
    Ok({
        let request: Value = serde_json::from_reader(io::stdin())?;
        let node = request["variables"]["id"].as_str().unwrap();
        let number = state["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["node_id"] == node)
            .unwrap()["number"]
            .as_u64()
            .unwrap();
        let actual = state["parents"][number.to_string()].as_u64();
        if state["mode"] == "parent-incomplete" {
            return Ok(json!({"data":{"node":{"id":node}}}));
        }
        if state["mode"] == "parent-errors" {
            return Ok(json!({"errors":[{"message":"permission"}],"data":{"node":null}}));
        }
        let parent = actual.map(|n| json!({"id":issue(state,n).unwrap()["node_id"]}));
        json!({"data":{"node":{"id":node,"parent":parent}}})
    })
}

fn relationship_response(
    endpoint: &str,
    args: &[String],
    state: &mut Value,
    path: &std::path::Path,
) -> Result<Value> {
    ensure!(state["mode"] != "graph-permission", "permission denied");
    ensure!(
        state["mode"] != "native-unavailable",
        "native relationships unavailable"
    );
    ensure!(
        state["mode"] != "label-final-graph" || state["label_mutations"].as_u64().unwrap_or(0) == 0,
        "final graph unavailable"
    );
    let number: u64 = endpoint.split('/').nth(4).unwrap().parse()?;
    let child_link = endpoint.contains("/sub_issues");
    if args.windows(2).any(|a| a == ["--method", "POST"]) {
        write_relationship(state, path, number, child_link)
    } else {
        read_relationship(
            state,
            args,
            number,
            child_link,
            endpoint.contains("/dependencies/blocking"),
        )
    }
}

fn write_relationship(
    state: &mut Value,
    path: &std::path::Path,
    number: u64,
    child_link: bool,
) -> Result<Value> {
    Ok({
        let before = state.clone();
        let request: Value = serde_json::from_reader(io::stdin())?;
        let id = request[if child_link {
            "sub_issue_id"
        } else {
            "issue_id"
        }]
        .as_u64()
        .unwrap();
        let target = state["issues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .cloned()
            .or_else(|| (id == 12392).then(|| issue(state, 2392).unwrap()))
            .unwrap();
        if child_link {
            ensure!(
                request["replace_parent"] == false,
                "parent replacement forbidden"
            );
            ensure!(
                state["parents"][target["number"].to_string()].is_null(),
                "existing parent"
            );
            if state["children"].is_null() {
                state["children"] = json!([]);
            }
            state["children"]
                .as_array_mut()
                .unwrap()
                .push(target["number"].clone());
            if state["parents"].is_null() {
                state["parents"] = json!({});
            }
            state["parents"][target["number"].to_string()] = json!(number);
        } else {
            if state["dependencies"].is_null() {
                state["dependencies"] = json!({});
            }
            if state["dependencies"][number.to_string()].is_null() {
                state["dependencies"][number.to_string()] = json!([]);
            }
            state["dependencies"][number.to_string()]
                .as_array_mut()
                .unwrap()
                .push(target["number"].clone());
        }
        if state["mode"] == "no-link-effect" {
            *state = before;
            state["mode"] = Value::Null;
        }
        if state["mode"] == "late-content" {
            state["issues"][0]["title"] = json!("External edit");
        }
        state["link_mutations"] = json!(state["link_mutations"].as_u64().unwrap_or(0) + 1);
        fs::write(path, serde_json::to_vec(state)?)?;
        if state["mode"] == "lose-link-response" {
            anyhow::bail!("lost link response");
        }
        target
    })
}

fn read_relationship(
    state: &Value,
    args: &[String],
    number: u64,
    child_link: bool,
    blocking: bool,
) -> Result<Value> {
    Ok({
        if state["mode"] == "incomplete-relationships" {
            return Ok(json!([[{}]]));
        }
        if state["mode"] == "empty-pages" {
            return Ok(json!([]));
        }
        ensure!(
            args.contains(&"--paginate".to_owned()) && args.contains(&"--slurp".to_owned()),
            "complete paginated relationships required"
        );
        let numbers = if child_link {
            &state["children"]
        } else {
            &state["dependencies"][number.to_string()]
        };
        let mut values: Vec<Value> = if blocking {
            state["dependencies"]
                .as_object()
                .map(|dependencies| {
                    dependencies
                        .iter()
                        .filter(|(_, blockers)| {
                            blockers.as_array().unwrap().iter().any(|n| n == number)
                        })
                        .map(|(subject, _)| issue(state, subject.parse().unwrap()).unwrap())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            numbers
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|n| issue(state, n.as_u64().unwrap()).unwrap())
                        .collect()
                })
                .unwrap_or_default()
        };
        if let Some(extra) = state[if child_link {
            "extra_children"
        } else if blocking {
            "extra_blocking"
        } else {
            "extra_dependencies"
        }]
        .as_array()
        {
            values.extend(extra.iter().cloned());
        }
        let pages: Vec<Value> = values.chunks(25).map(|a| json!(a)).collect();
        if pages.is_empty() {
            json!([[]])
        } else {
            json!(pages)
        }
    })
}

fn create_response(
    endpoint: &str,
    args: &[String],
    state: &mut Value,
    path: &std::path::Path,
) -> Result<Value> {
    Ok({
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
        fs::write(path, serde_json::to_vec(state)?)?;
        if state["mode"] == "lose-response" {
            anyhow::bail!("response lost after acceptance");
        }
        issue
    })
}

fn enumeration_response(endpoint: &str, args: &[String], state: &Value) -> Result<Value> {
    Ok({
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
            return Ok(json!({}));
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
    })
}

fn label_response(endpoint: &str, state: &mut Value, path: &std::path::Path) -> Result<Value> {
    let number: u64 = endpoint.split('/').nth(4).unwrap().parse()?;
    ensure!(
        state["mode"] != "stop-labels" || state["label_mutations"].as_u64().unwrap_or(0) == 0,
        "interrupted label publication"
    );
    if let Some(expected) = state.get("expected_children") {
        ensure!(
            state["children"] == *expected,
            "labels preceded complete graph"
        );
    }
    if let Some(expected) = state.get("expected_dependencies") {
        ensure!(
            state["dependencies"] == *expected,
            "labels preceded complete dependencies"
        );
    }
    let request: Value = serde_json::from_reader(io::stdin())?;
    let before = state.clone();
    let issue = state["issues"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|issue| issue["number"] == number)
        .unwrap();
    for label in request["labels"].as_array().unwrap() {
        ensure!(
            !issue["labels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|actual| actual["name"] == *label),
            "repeated completed label mutation"
        );
        issue["labels"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":label}));
    }
    let labels = issue["labels"].clone();
    if state["mode"] == "no-label-effect" {
        *state = before;
    }
    if state["mode"] == "label-final-content" {
        state["issues"][0]["title"] = json!("External edit");
    }
    state["label_mutations"] = json!(state["label_mutations"].as_u64().unwrap_or(0) + 1);
    fs::write(path, serde_json::to_vec(state)?)?;
    ensure!(
        state["mode"] != "lose-label-response",
        "lost label response"
    );
    Ok(labels)
}
