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
}
#[derive(Clone, Default)]
struct Fixture(Rc<RefCell<Wire>>);
fn raw_issue(n: u64) -> Value {
    json!({"id":format!("I{n}"),"number":n,"repository":{"nameWithOwner":"LowkeyLab/bazel-repo"}})
}
impl Transport for Fixture {
    fn request(&mut self, endpoint: &str, body: Option<Value>, _paginate: bool) -> Result<Value> {
        let s = self.0.borrow();
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
            value["comments"] = json!({"totalCount":s.comments.iter().filter(|c|c["issue_url"]==issue_url(n)).count()});
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
        Ok(json!([comments]))
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
