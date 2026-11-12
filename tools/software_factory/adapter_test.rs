//! Complete GitHub wire fixtures exercised through the controller boundary.
use std::{cell::RefCell, rc::Rc};

use googletest::{expect_that, gtest, matchers::eq};
use serde_json::{Value, json};

use software_factory::{
    Result,
    controller::{Controller, Decision, GitHub},
    github::{
        Adapter, AppPublisher, ExecutionInventory, HistoryPublisher, InstallationTransportFactory,
        Transport, Unprovisioned,
    },
    model::{AppIdentity, IssueKey},
    policy,
    store::Store,
};

use super::{Native, app, key};

#[derive(Default)]
struct Wire {
    fault: String,
    comments: Vec<Value>,
    history_requests: Vec<(String, bool)>,
}
#[derive(Clone, Default)]
struct Fixture(Rc<RefCell<Wire>>);
fn raw_issue(n: u64) -> Value {
    json!({"id":format!("I{n}"),"number":n,"repository":{"nameWithOwner":"LowkeyLab/bazel-repo"}})
}
impl Transport for Fixture {
    fn request(&mut self, endpoint: &str, body: Option<Value>, paginate: bool) -> Result<Value> {
        let mut s = self.0.borrow_mut();
        if endpoint.contains("/comments?") {
            s.history_requests.push((endpoint.into(), paginate));
        }
        if endpoint == "graphql" {
            let body = body.unwrap();
            let q = body["query"].as_str().unwrap();
            let id = body["variables"]["id"].as_str().unwrap_or("I2");
            let n = id.trim_start_matches('I').parse::<u64>().unwrap_or(2);
            if s.fault == "graphql-error" {
                return Ok(json!({"errors":[{"message":"denied"}],"data":{"node":null}}));
            }
            if q.contains("repository(owner:") {
                return Ok(
                    json!({"data":{"repository":{"issue":raw_issue(body["variables"]["number"].as_u64().unwrap())}}}),
                );
            }
            if q.contains("$cursor") {
                let field = if q.contains("items(") {
                    "items"
                } else if q.contains("labels(") {
                    "labels"
                } else if q.contains("assignedActors(") {
                    "assignedActors"
                } else if q.contains("subIssues(") {
                    "subIssues"
                } else {
                    "blockedBy"
                };
                let values = match field {
                    "items" => vec![
                        json!({"id":"P2","content":raw_with_type(2)}),
                        json!({"id":"P3","content":raw_with_type(3)}),
                        json!({"id":"PR","content":{"__typename":"PullRequest"}}),
                        json!({"id":"OTHER","content":{"__typename":"Issue","id":"FOREIGN","number":99,"repository":{"nameWithOwner":"Other/repo"}}}),
                    ],
                    "labels" if n != 1 => vec![
                        json!({"id":"L1","name":"ready-for-agent"}),
                        json!({"id":"L2","name":"bug"}),
                    ],
                    "assignedActors" if n == 2 => vec![
                        json!({"id":"U1","login":"controller"}),
                        json!({"id":"U2","login":if s.fault=="conflict-on-later-page"{"developer"}else{"controller"}}),
                    ],
                    "subIssues" if n == 1 => vec![raw_issue(2), raw_issue(3)],
                    "blockedBy" if n == 2 => vec![
                        json!({"id":"B1","state":"CLOSED"}),
                        json!({"id":"B2","state":if s.fault=="blocker-on-later-page"{"OPEN"}else{"CLOSED"}}),
                    ],
                    _ => vec![],
                };
                let index = body["variables"]["cursor"]
                    .as_str()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let count = values.len();
                let mut nodes = values.get(index).cloned().into_iter().collect::<Vec<_>>();
                let mut total = count;
                let next = index + 1 < count;
                if s.fault == "incomplete-labels" && field == "labels" {
                    total += 1;
                }
                if s.fault == "inaccessible-child" && field == "subIssues" {
                    nodes = vec![Value::Null];
                }
                let cursor = if s.fault == "repeated-cursor" {
                    "1".to_string()
                } else {
                    (index + 1).to_string()
                };
                return Ok(
                    json!({"data":{"node":{field:{"totalCount":total,"nodes":nodes,"pageInfo":{"hasNextPage":next,"endCursor":cursor}}}}}),
                );
            }
            let mut value = raw_with_type(n);
            value["createdAt"] = json!(if n == 3 {
                "2026-01-01T00:00:00Z"
            } else {
                "2026-01-02T00:00:00Z"
            });
            value["state"] = json!("OPEN");
            value["parent"] = if n == 1 { Value::Null } else { raw_issue(1) };
            let count = s
                .comments
                .iter()
                .filter(|c| c["issue_url"] == issue_url(n))
                .count();
            let count = count
                + usize::from(s.fault == "changing-history" && !s.history_requests.is_empty());
            value["comments"] = json!({"totalCount":count});
            return Ok(json!({"data":{"node":value}}));
        }
        if endpoint.contains("/issues/comments/") {
            let id = endpoint.rsplit('/').next().unwrap().parse::<u64>().unwrap();
            return Ok(s.comments.iter().find(|c| c["id"] == id).unwrap().clone());
        }
        let n = endpoint
            .split("/issues/")
            .nth(1)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut comments = s
            .comments
            .iter()
            .filter(|c| c["issue_url"] == issue_url(n))
            .cloned()
            .collect::<Vec<_>>();
        if s.fault == "partial-history" {
            comments.clear();
        }
        if paginate {
            return Ok(json!([comments]));
        }
        let page = endpoint
            .split("&page=")
            .nth(1)
            .unwrap()
            .parse::<usize>()
            .unwrap();
        if s.fault == "failed-history-page" && page == 2 {
            return Err(software_factory::Error::Stop("fixture page failure".into()));
        }
        if s.fault == "malformed-history-page" {
            return Ok(json!({"unexpected":"object"}));
        }
        if s.fault == "duplicate-history" && page == 2 {
            return Ok(json!([comments[0].clone()]));
        }
        let size = if s.fault == "oversized-history-page" {
            101
        } else {
            100
        };
        Ok(json!(
            comments
                .into_iter()
                .skip((page - 1) * 100)
                .take(size)
                .collect::<Vec<_>>()
        ))
    }
}
fn raw_with_type(n: u64) -> Value {
    let mut issue = raw_issue(n);
    issue["__typename"] = json!("Issue");
    issue
}
fn issue_url(n: u64) -> String {
    format!("https://api.github.com/repos/LowkeyLab/bazel-repo/issues/{n}")
}
impl HistoryPublisher for Fixture {
    fn publish(&mut self, issue: &IssueKey, body: &str, _: &AppIdentity) -> Result<String> {
        let mut s = self.0.borrow_mut();
        let id = s.comments.len() as u64 + 1;
        s.comments.push(json!({"id":id,"issue_url":issue_url(issue.number()),"body":body,"performed_via_github_app":{"id":100},"user":{"id":200,"login":"factory[bot]","type":"Bot"}}));
        Ok(id.to_string())
    }
}
struct Clear;
impl ExecutionInventory for Clear {
    fn clear(&mut self, _: &IssueKey) -> Result<bool> {
        Ok(true)
    }
}
fn adapter(f: Fixture) -> Adapter<Fixture, Clear, Fixture> {
    Adapter::new(f.clone(), Clear, f, app(), 100, 100).unwrap()
}
#[gtest]
fn paginated_wire_adapter_selects_native_first_eligible_child_through_controller() {
    let dir = tempfile::tempdir().unwrap();
    let wire = Fixture::default();
    let store = Store::initialize(
        &dir.path().join("private"),
        app(),
        software_factory::model::Identity::new(),
    )
    .unwrap();
    let mut c = Controller::new(
        store,
        adapter(wire.clone()),
        Native::default(),
        "controller".into(),
    )
    .unwrap();
    expect_that!(
        matches!(c.poll().unwrap(),Decision::Selected(a) if a.issue==key(2)),
        eq(true)
    );
    expect_that!(wire.0.borrow().comments.len(), eq(1));
}
#[gtest]
fn later_blocker_and_assignment_pages_exclude_the_first_sibling() {
    for fault in ["blocker-on-later-page", "conflict-on-later-page"] {
        let f = Fixture::default();
        f.0.borrow_mut().fault = fault.into();
        let snapshot = adapter(f).snapshot().unwrap();
        expect_that!(
            policy::select(&snapshot, "controller").unwrap().unwrap().0,
            eq(&key(3))
        );
    }
}
#[gtest]
fn partial_connections_and_graphql_errors_never_establish_eligibility() {
    for fault in [
        "incomplete-labels",
        "inaccessible-child",
        "repeated-cursor",
        "graphql-error",
    ] {
        let f = Fixture::default();
        f.0.borrow_mut().fault = fault.into();
        expect_that!(adapter(f).snapshot().is_err(), eq(true));
    }
}
#[gtest]
fn history_rejects_an_over_bound_count_before_fetching_any_pages() {
    let f = Fixture::default();
    for _ in 0..201 {
        f.clone().publish(&key(2), "fixture", &app()).unwrap();
    }
    let mut a = Adapter::new(f.clone(), Clear, f.clone(), app(), 2, 100).unwrap();
    expect_that!(a.history(&key(2)).is_err(), eq(true));
    expect_that!(f.0.borrow().history_requests.len(), eq(0));
}

#[gtest]
fn history_fetches_only_explicit_bounded_pages_without_transport_autopagination() {
    for count in [0, 1, 100, 101, 200] {
        let f = Fixture::default();
        for _ in 0..count {
            f.clone().publish(&key(2), "fixture", &app()).unwrap();
        }
        let mut a = Adapter::new(f.clone(), Clear, f.clone(), app(), 2, 100).unwrap();
        expect_that!(a.history(&key(2)).unwrap().len(), eq(count));
        let expected = (1..=count.div_ceil(100).max(1))
            .map(|page| {
                (
                    format!(
                        "repos/LowkeyLab/bazel-repo/issues/2/comments?per_page=100&page={page}"
                    ),
                    false,
                )
            })
            .collect::<Vec<_>>();
        expect_that!(&f.0.borrow().history_requests, eq(&expected));
    }
}

#[gtest]
fn history_page_failures_never_return_partial_or_unproven_history() {
    for fault in [
        "failed-history-page",
        "malformed-history-page",
        "duplicate-history",
        "oversized-history-page",
        "changing-history",
        "partial-history",
    ] {
        let f = Fixture::default();
        for _ in 0..101 {
            f.clone().publish(&key(2), "fixture", &app()).unwrap();
        }
        f.0.borrow_mut().fault = fault.into();
        let mut a = Adapter::new(f.clone(), Clear, f.clone(), app(), 2, 100).unwrap();
        expect_that!(a.history(&key(2)).is_err(), eq(true));
        let requests = &f.0.borrow().history_requests;
        expect_that!(requests.len() <= 2, eq(true));
        expect_that!(requests.iter().all(|(_, paginate)| !paginate), eq(true));
    }
}

#[gtest]
fn partial_history_cannot_advance_or_admit() {
    let dir = tempfile::tempdir().unwrap();
    let f = Fixture::default();
    let store = Store::initialize(
        &dir.path().join("private"),
        app(),
        software_factory::model::Identity::new(),
    )
    .unwrap();
    let mut c = Controller::new(
        store,
        adapter(f.clone()),
        Native::default(),
        "controller".into(),
    )
    .unwrap();
    c.poll().unwrap();
    f.0.borrow_mut().fault = "partial-history".into();
    expect_that!(c.prepare_admission().is_err(), eq(true));
}
#[gtest]
fn unavailable_native_inventory_and_publication_prerequisites_fail_closed() {
    let f = Fixture::default();
    let mut a = Adapter::new(f.clone(), Unprovisioned, Unprovisioned, app(), 100, 100).unwrap();
    expect_that!(a.snapshot().is_err(), eq(true));
    expect_that!(a.publish(&key(2), "history").is_err(), eq(true));
}
#[derive(Clone)]
struct AppWire(Rc<RefCell<Vec<(String, Option<Value>)>>>, bool);
impl Transport for AppWire {
    fn request(&mut self, e: &str, b: Option<Value>, _: bool) -> Result<Value> {
        self.0.borrow_mut().push((e.into(), b.clone()));
        if e == "app" {
            return Ok(json!({"id":if self.1{999}else{100}}));
        }
        if e.ends_with("access_tokens") {
            return Ok(
                json!({"token":"fixture-secret","permissions":{"issues":"write","metadata":"read"},"repositories":[{"full_name":"LowkeyLab/bazel-repo"}]}),
            );
        }
        if e.starts_with("app/installations/") {
            return Ok(
                json!({"id":77,"app_id":100,"account":{"login":"LowkeyLab"},"suspended_at":null}),
            );
        }
        Ok(
            json!({"id":1,"body":b.unwrap()["body"],"issue_url":issue_url(2),"performed_via_github_app":{"id":100},"user":{"id":200,"login":"factory[bot]","type":"Bot"}}),
        )
    }
}
#[gtest]
fn app_jwt_transport_sends_bearer_only_through_private_stdin() {
    use software_factory::github::AppJwtTransport;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("curl");
    // Record only fixture credentials at the public command boundary.
    std::fs::write(&command, format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/args'\ncat > '{}/input'\nprintf '{{\"id\":100}}'\n",
        dir.path().display(), dir.path().display()
    )).unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o700)).unwrap();
    // The test target sets this variable. No global environment mutation is needed.
    let mut transport = AppJwtTransport::new(command.clone(), "RUST_TEST_THREADS".into()).unwrap();
    let body = json!({"repositories":["bazel-repo"],"permissions":{"issues":"write"}});
    expect_that!(
        transport
            .request(
                "app/installations/77/access_tokens",
                Some(body.clone()),
                false
            )
            .unwrap(),
        eq(&json!({"id":100}))
    );
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    let input = std::fs::read_to_string(dir.path().join("input")).unwrap();
    expect_that!(args.starts_with("--disable\n"), eq(true));
    expect_that!(args.contains("--config\n-\n"), eq(true));
    expect_that!(
        args.contains("https://api.github.com/app/installations/77/access_tokens"),
        eq(true)
    );
    expect_that!(args.contains("Authorization"), eq(false));
    expect_that!(
        input.contains("header = \"Authorization: Bearer 1\"\n"),
        eq(true)
    );
    expect_that!(
        input.contains(&format!(
            "data-binary = {}\n",
            serde_json::to_string(&body.to_string()).unwrap()
        )),
        eq(true)
    );
    transport.request("app", None, false).unwrap();
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    let input = std::fs::read_to_string(dir.path().join("input")).unwrap();
    expect_that!(args.contains("--request\nGET\n"), eq(true));
    expect_that!(input.contains("Authorization: Bearer 1"), eq(true));
    expect_that!(input.contains("data-binary"), eq(false));
    std::fs::write(
        &command,
        "#!/bin/sh\ncat >/dev/null\nprintf 'fixture.jwt.secret' >&2\nexit 22\n",
    )
    .unwrap();
    expect_that!(
        transport
            .request("app", None, false)
            .unwrap_err()
            .to_string(),
        eq("safety stop: App HTTP request failed; response withheld")
    );
}

#[gtest]
fn gh_read_and_installation_transports_keep_token_auth_without_secret_arguments() {
    use software_factory::github::{GhInstallationFactory, GhTransport};
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let command = dir.path().join("gh");
    std::fs::write(&command, format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}/args'\nprintf '%s' \"$GH_TOKEN\" > '{}/token'\ncat > '{}/input'\nprintf '{{}}'\n",
        dir.path().display(), dir.path().display(), dir.path().display()
    )).unwrap();
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut read = GhTransport::new(command.clone(), "RUST_TEST_THREADS".into()).unwrap();
    read.request(
        "repos/LowkeyLab/bazel-repo/issues/2/comments?per_page=100&page=1",
        None,
        false,
    )
    .unwrap();
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    expect_that!(args.contains("--paginate"), eq(false));
    expect_that!(args.contains("Authorization"), eq(false));
    expect_that!(
        std::fs::read_to_string(dir.path().join("token")).unwrap(),
        eq("1")
    );
    let mut installed = GhInstallationFactory { command }
        .create("fixture-installation".into())
        .unwrap();
    let body = json!({"body":"fixture publication"});
    installed
        .request(
            "repos/LowkeyLab/bazel-repo/issues/2/comments",
            Some(body.clone()),
            false,
        )
        .unwrap();
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    expect_that!(args.contains("fixture-installation"), eq(false));
    expect_that!(args.contains("--input\n-\n"), eq(true));
    expect_that!(
        std::fs::read_to_string(dir.path().join("token")).unwrap(),
        eq("fixture-installation")
    );
    expect_that!(
        std::fs::read_to_string(dir.path().join("input")).unwrap(),
        eq(&body.to_string())
    );
}

#[gtest]
fn app_jwt_transport_rejects_missing_credentials_and_unbounded_requests_before_spawn() {
    use software_factory::github::AppJwtTransport;
    let command = std::path::PathBuf::from("/nonexistent-approved-command");
    expect_that!(
        AppJwtTransport::new("curl".into(), "RUST_TEST_THREADS".into()).is_err(),
        eq(true)
    );
    let mut transport = AppJwtTransport::new(command.clone(), "RUST_TEST_THREADS".into()).unwrap();
    for (endpoint, paginate) in [
        ("app", true),
        ("https://other.invalid/app", false),
        ("app/../user", false),
    ] {
        expect_that!(
            transport.request(endpoint, None, paginate).is_err(),
            eq(true)
        );
    }
    let mut missing =
        AppJwtTransport::new(command, "SOFTWARE_FACTORY_ABSENT_JWT_FIXTURE_03167".into()).unwrap();
    expect_that!(
        missing
            .request("app", None, false)
            .unwrap_err()
            .to_string()
            .contains("JWT unavailable"),
        eq(true)
    );
}

struct AppFactory(AppWire);
impl InstallationTransportFactory for AppFactory {
    type Client = AppWire;
    fn create(&mut self, _: String) -> Result<AppWire> {
        Ok(self.0.clone())
    }
}
#[gtest]
fn publisher_authenticates_configured_app_and_scopes_installation_token() {
    for wrong in [false, true] {
        let requests = Rc::new(RefCell::new(vec![]));
        let wire = AppWire(requests.clone(), wrong);
        let mut p = AppPublisher::new(wire.clone(), AppFactory(wire), 77).unwrap();
        let body = format!("{}{{}}", software_factory::history::PREFIX);
        expect_that!(p.publish(&key(2), &body, &app()).is_ok(), eq(!wrong));
        let sends = requests
            .borrow()
            .iter()
            .filter(|(e, _)| e.contains("/issues/2/comments"))
            .count();
        expect_that!(sends, eq(if wrong { 0 } else { 1 }));
        if !wrong {
            let token = requests
                .borrow()
                .iter()
                .find(|(e, _)| e.ends_with("access_tokens"))
                .unwrap()
                .1
                .clone()
                .unwrap();
            expect_that!(
                token
                    == json!({"repositories":["bazel-repo"],"permissions":{"issues":"write","metadata":"read"}}),
                eq(true)
            );
        }
    }
}
