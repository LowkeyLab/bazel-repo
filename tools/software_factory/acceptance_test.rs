use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use googletest::{expect_that, gtest, matchers::eq};
use software_factory::{
    Error,
    controller::{Controller, Decision, Effects, GitHub, Observation},
    model::{
        Action, AppIdentity, Comment, EffectKind, Identity, Issue, IssueKey, PROJECT, Snapshot,
    },
    policy,
    store::{StoppedOwner, Store},
};

mod adapter_test;
type Result<T> = software_factory::Result<T>;
fn key(n: u64) -> IssueKey {
    format!("LowkeyLab/bazel-repo#{n}").try_into().unwrap()
}
fn app() -> AppIdentity {
    AppIdentity {
        app_id: "100".into(),
        actor_id: "200".into(),
        login: "factory[bot]".into(),
    }
}
fn issue(n: u64, day: &str) -> Issue {
    Issue {
        key: key(n),
        node: format!("I{n}"),
        created_at: format!("2026-01-{day}T00:00:00Z"),
        open: true,
        pull_request: false,
        labels: vec!["ready-for-agent".into()],
        assignees: vec![],
        blockers_closed: vec![],
        parent: None,
        children: vec![],
        execution_clear: true,
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        project: PROJECT.into(),
        complete: true,
        members: vec![key(2), key(3)],
        issues: BTreeMap::from([(key(2), issue(2, "02")), (key(3), issue(3, "03"))]),
    }
}
#[derive(Clone)]
struct Remote(Rc<RefCell<RemoteState>>);
struct RemoteState {
    snapshot: Snapshot,
    comments: Vec<Comment>,
    sends: usize,
    lost_ack: bool,
    drop_send: bool,
    fail_exact: bool,
    wrong_exact: bool,
    revoke: bool,
}
impl Remote {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(RemoteState {
            snapshot: snapshot(),
            comments: vec![],
            sends: 0,
            lost_ack: false,
            drop_send: false,
            fail_exact: false,
            wrong_exact: false,
            revoke: false,
        })))
    }
}
impl GitHub for Remote {
    fn snapshot(&mut self) -> Result<Snapshot> {
        Ok(self.0.borrow().snapshot.clone())
    }
    fn history(&mut self, k: &IssueKey) -> Result<Vec<Comment>> {
        Ok(self
            .0
            .borrow()
            .comments
            .iter()
            .filter(|c| &c.issue == k)
            .cloned()
            .collect())
    }
    fn publish(&mut self, k: &IssueKey, body: &str) -> Result<String> {
        let mut s = self.0.borrow_mut();
        s.sends += 1;
        let id = format!("C{}", s.sends);
        if !s.drop_send {
            s.comments.push(Comment {
                id: id.clone(),
                issue: k.clone(),
                app: Some(app()),
                body: body.into(),
            });
        }
        if s.revoke {
            s.snapshot.issues.get_mut(k).unwrap().assignees = vec!["developer".into()];
        }
        if s.lost_ack {
            s.lost_ack = false;
            return Err(Error::Stop("lost publication ack".into()));
        }
        Ok(id)
    }
    fn exact_comment(&mut self, k: &IssueKey, id: &str) -> Result<Comment> {
        let s = self.0.borrow();
        if s.fail_exact {
            return Err(Error::Stop("readback unavailable".into()));
        }
        let mut comment = s
            .comments
            .iter()
            .find(|c| &c.issue == k && c.id == id)
            .cloned()
            .ok_or_else(|| Error::Stop("missing comment".into()))?;
        if s.wrong_exact {
            comment.body = "changed readback".into();
        }
        Ok(comment)
    }
}
#[derive(Clone, Default)]
struct Native(Rc<RefCell<NativeState>>);
#[derive(Default)]
struct NativeState {
    sent: Vec<Action>,
    lost_ack: bool,
    absent: bool,
    wrong: bool,
}
impl Effects for Native {
    fn send(&mut self, a: &Action) -> Result<()> {
        let mut s = self.0.borrow_mut();
        s.sent.push(a.clone());
        if s.lost_ack {
            s.lost_ack = false;
            Err(Error::Stop("lost native ack".into()))
        } else {
            Ok(())
        }
    }
    fn inspect_exact(&mut self, a: &Action) -> Result<Observation> {
        let s = self.0.borrow();
        if s.absent {
            return Ok(Observation::Absent);
        }
        let Some(found) = s.sent.iter().find(|f| f.id == a.id) else {
            return Ok(Observation::Unknown);
        };
        let mut found = found.clone();
        if s.wrong {
            found.target = "wrong".into();
        }
        Ok(Observation::Confirmed {
            action: found,
            evidence: "native-receipt:exact-1".into(),
        })
    }
}
impl software_factory::controller::Hermes for Native {}
impl software_factory::controller::Pool for Native {}
impl software_factory::controller::Notifications for Native {}
struct Stopped(bool);
impl StoppedOwner for Stopped {
    fn verify_stopped(&mut self, _: &Identity, _: &Identity) -> Result<String> {
        if self.0 {
            Ok("supervisor:fixture-stopped-owner".into())
        } else {
            Err(Error::Stop("old owner not stopped".into()))
        }
    }
}
fn setup() -> (
    tempfile::TempDir,
    Controller<Remote, Native>,
    Remote,
    Native,
) {
    let dir = tempfile::tempdir().unwrap();
    let remote = Remote::new();
    let native = Native::default();
    let store = Store::initialize(&dir.path().join("private"), app(), Identity::new()).unwrap();
    let c = Controller::new(store, remote.clone(), native.clone(), "controller".into()).unwrap();
    (dir, c, remote, native)
}
fn action() -> Action {
    Action {
        id: Identity::new(),
        issue: key(2),
        kind: EffectKind::Hermes,
        target: "native-session:exact-1".into(),
        payload: "frozen-admission".into(),
    }
}
#[gtest]
fn selection_wait_and_restart_retain_one_job() {
    let (dir, mut c, r, _) = setup();
    expect_that!(
        matches!(c.poll().unwrap(),Decision::Selected(a) if a.issue==key(2)),
        eq(true)
    );
    c.wait("CI wait".into()).unwrap();
    let (store, _, _) = c.into_parts();
    let job = store.load().unwrap().active.unwrap().job;
    drop(store);
    let store = Store::open(
        &dir.path().join("private"),
        Identity::new(),
        &mut Stopped(true),
    )
    .unwrap();
    let mut c = Controller::new(store, r.clone(), Native::default(), "controller".into()).unwrap();
    expect_that!(
        matches!(c.poll().unwrap(),Decision::Retained(a) if a.job==job),
        eq(true)
    );
    expect_that!(r.0.borrow().sends, eq(3));
}
#[gtest]
fn uncertain_history_recovers_exact_record_without_duplicate_send() {
    let (_dir, mut c, r, _) = setup();
    r.0.borrow_mut().lost_ack = true;
    expect_that!(c.poll().is_err(), eq(true));
    expect_that!(matches!(c.poll().unwrap(), Decision::Retained(_)), eq(true));
    expect_that!(r.0.borrow().sends, eq(1));
}
#[gtest]
fn lost_ack_discovered_comment_damage_stays_stopped_after_restoration() {
    for deleted in [false, true] {
        let (_dir, mut c, remote, native) = setup();
        remote.0.borrow_mut().lost_ack = true;
        expect_that!(c.poll().is_err(), eq(true));
        remote.0.borrow_mut().fail_exact = true;
        expect_that!(c.recover().is_err(), eq(true));
        let (store, github, effects) = c.into_parts();
        expect_that!(store.load().unwrap().integrity_stop.is_none(), eq(true));
        let mut c = Controller::new(store, github, effects, "controller".into()).unwrap();
        let original = remote.0.borrow().comments.clone();
        if deleted {
            remote.0.borrow_mut().comments.clear();
        } else {
            remote.0.borrow_mut().comments[0].body = "edited marker".into();
        }
        expect_that!(c.recover().is_err(), eq(true));
        {
            let mut state = remote.0.borrow_mut();
            state.comments = original;
            state.fail_exact = false;
        }
        expect_that!(c.recover().is_err(), eq(true));
        expect_that!(c.prepare_admission().is_err(), eq(true));
        expect_that!(remote.0.borrow().sends, eq(1));
        expect_that!(native.0.borrow().sent.len(), eq(0));
    }
}
#[gtest]
fn absent_after_uncertain_history_send_never_republishes() {
    let (_dir, mut c, r, _) = setup();
    {
        let mut s = r.0.borrow_mut();
        s.drop_send = true;
        s.lost_ack = true;
    }
    expect_that!(c.poll().is_err(), eq(true));
    expect_that!(c.poll().is_err(), eq(true));
    expect_that!(r.0.borrow().sends, eq(1));
}
#[gtest]
fn readback_failure_does_not_advance_anchor_or_send_effect() {
    let (_dir, mut c, r, n) = setup();
    c.poll().unwrap();
    r.0.borrow_mut().fail_exact = true;
    expect_that!(c.perform(action()).is_err(), eq(true));
    expect_that!(n.0.borrow().sent.len(), eq(0));
    let (store, _, _) = c.into_parts();
    expect_that!(store.load().unwrap().anchors[&key(2)].len(), eq(1));
    expect_that!(store.load().unwrap().pending_record.is_some(), eq(true));
    r.0.borrow_mut().fail_exact = false;
    let mut c = Controller::new(store, r.clone(), n, "controller".into()).unwrap();
    c.recover().unwrap();
    expect_that!(r.0.borrow().sends, eq(2));
}
#[gtest]
fn recheck_after_selection_retains_slot_on_assignment_conflict() {
    let (_dir, mut c, r, n) = setup();
    r.0.borrow_mut().revoke = true;
    expect_that!(c.poll().is_err(), eq(true));
    expect_that!(c.prepare_admission().is_err(), eq(true));
    expect_that!(n.0.borrow().sent.len(), eq(0));
    let (store, _, _) = c.into_parts();
    expect_that!(store.load().unwrap().active.unwrap().issue, eq(&key(2)));
}
#[gtest]
fn admission_rechecks_blockers_without_removing_readiness() {
    let (_dir, mut c, r, _) = setup();
    c.poll().unwrap();
    r.0.borrow_mut()
        .snapshot
        .issues
        .get_mut(&key(2))
        .unwrap()
        .blockers_closed = vec![false];
    expect_that!(c.prepare_admission().is_err(), eq(true));
    expect_that!(
        r.0.borrow().snapshot.issues[&key(2)].labels.clone(),
        eq(&vec!["ready-for-agent".to_string()])
    );
    r.0.borrow_mut()
        .snapshot
        .issues
        .get_mut(&key(2))
        .unwrap()
        .blockers_closed
        .clear();
    expect_that!(c.prepare_admission().unwrap().issue, eq(&key(2)));
}
#[gtest]
fn action_recovery_preserves_original_identity_and_exact_target() {
    let (dir, mut c, r, n) = setup();
    c.poll().unwrap();
    let a = action();
    n.0.borrow_mut().lost_ack = true;
    expect_that!(c.perform(a.clone()).is_err(), eq(true));
    expect_that!(r.0.borrow().comments.len(), eq(2));
    let (store, _, _) = c.into_parts();
    expect_that!(store.load().unwrap().pending_effect.unwrap().action, eq(&a));
    drop(store);
    let store = Store::open(
        &dir.path().join("private"),
        Identity::new(),
        &mut Stopped(true),
    )
    .unwrap();
    let mut c = Controller::new(store, r.clone(), n.clone(), "controller".into()).unwrap();
    c.recover().unwrap();
    c.perform(a).unwrap();
    expect_that!(n.0.borrow().sent.len(), eq(1));
    expect_that!(r.0.borrow().comments.len(), eq(4));
}
#[gtest]
fn absent_or_wrong_effect_result_never_allows_repeat_or_another_job() {
    for wrong in [false, true] {
        let (_dir, mut c, _, n) = setup();
        c.poll().unwrap();
        let a = action();
        {
            let mut s = n.0.borrow_mut();
            s.lost_ack = true;
            s.absent = !wrong;
            s.wrong = wrong;
        }
        expect_that!(c.perform(a.clone()).is_err(), eq(true));
        expect_that!(c.perform(a).is_err(), eq(true));
        expect_that!(c.poll().is_err(), eq(true));
        expect_that!(n.0.borrow().sent.len(), eq(1));
    }
}
#[gtest]
fn damaged_history_stops_advancement() {
    for mode in [
        "edited",
        "deleted",
        "forged",
        "duplicate",
        "marker",
        "author",
    ] {
        let (_dir, mut c, r, n) = setup();
        c.poll().unwrap();
        {
            let mut s = r.0.borrow_mut();
            match mode {
                "edited" => s.comments[0].body.push(' '),
                "deleted" => s.comments.clear(),
                "forged" => {
                    let mut a = s.comments[0].clone();
                    a.id = "forged".into();
                    a.app = None;
                    s.comments.push(a);
                }
                "duplicate" => {
                    let a = s.comments[0].clone();
                    s.comments.push(a);
                }
                "marker" => s.comments[0].body = "worker summary".into(),
                _ => s.comments[0].app.as_mut().unwrap().app_id = "other".into(),
            }
        }
        expect_that!(c.prepare_admission().is_err(), eq(true));
        expect_that!(n.0.borrow().sent.len(), eq(0));
    }
}
#[gtest]
fn native_pool_hermes_and_notification_routes_share_intent_and_recovery_contract() {
    for kind in [
        EffectKind::Pool,
        EffectKind::Hermes,
        EffectKind::Notification,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let r = Remote::new();
        let native = Native::default();
        let store = Store::initialize(&dir.path().join("private"), app(), Identity::new()).unwrap();
        let routes = software_factory::controller::NativeEffects {
            hermes: native.clone(),
            pool: native.clone(),
            notifications: native.clone(),
        };
        let mut c = Controller::new(store, r.clone(), routes, "controller".into()).unwrap();
        c.poll().unwrap();
        let mut a = action();
        a.kind = kind;
        c.perform(a).unwrap();
        expect_that!(native.0.borrow().sent.len(), eq(1));
        expect_that!(r.0.borrow().comments.len(), eq(3));
    }
}
#[gtest]
fn notification_failure_cannot_clear_stop_or_release_slot() {
    let (_dir, mut c, _, n) = setup();
    c.poll().unwrap();
    let mut a = action();
    a.kind = EffectKind::Notification;
    {
        let mut n = n.0.borrow_mut();
        n.lost_ack = true;
        n.absent = true;
    }
    expect_that!(c.perform(a).is_err(), eq(true));
    expect_that!(c.poll().is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    expect_that!(store.load().unwrap().active.unwrap().issue, eq(&key(2)));
}
#[gtest]
fn process_owner_probe() {
    if let Ok(path) = std::env::var("SOFTWARE_FACTORY_LOCK_PROBE") {
        let allowed = std::env::var("SOFTWARE_FACTORY_LOCK_EXPECT").unwrap() == "allow";
        expect_that!(
            Store::open(
                std::path::Path::new(&path),
                Identity::new(),
                &mut Stopped(true)
            )
            .is_ok(),
            eq(allowed)
        );
    }
}
#[gtest]
fn independent_process_observes_exclusive_lock_and_stopped_owner_handoff() {
    let (dir, c, _, _) = setup();
    let run = |expected: &str| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_owner_probe"])
            .env("SOFTWARE_FACTORY_LOCK_PROBE", dir.path().join("private"))
            .env("SOFTWARE_FACTORY_LOCK_EXPECT", expected)
            .output()
            .unwrap()
            .status
            .success()
    };
    expect_that!(run("deny"), eq(true));
    drop(c);
    expect_that!(run("allow"), eq(true));
}
#[gtest]
fn lock_and_stopped_owner_handoff_are_required() {
    let (dir, c, _, _) = setup();
    let path = dir.path().join("private");
    expect_that!(
        Store::open(&path, Identity::new(), &mut Stopped(true)).is_err(),
        eq(true)
    );
    drop(c);
    expect_that!(
        Store::open(&path, Identity::new(), &mut Stopped(false)).is_err(),
        eq(true)
    );
    expect_that!(
        Store::open(&path, Identity::new(), &mut Stopped(true)).is_ok(),
        eq(true)
    );
}
#[gtest]
fn missing_or_nonprivate_receipts_are_not_new_jobs() {
    use std::os::unix::fs::PermissionsExt;
    for name in ["identity.json", "state.json", "lock"] {
        let (dir, c, _, _) = setup();
        drop(c);
        let path = dir.path().join("private").join(name);
        if name == "lock" {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644)).unwrap();
        } else {
            std::fs::remove_file(path).unwrap();
        }
        expect_that!(
            Store::open(
                &dir.path().join("private"),
                Identity::new(),
                &mut Stopped(true)
            )
            .is_err(),
            eq(true)
        );
        expect_that!(
            Store::initialize(&dir.path().join("private"), app(), Identity::new()).is_err(),
            eq(true)
        );
    }
}
#[gtest]
fn receipts_have_private_modes_and_reject_symlinked_root() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (dir, c, _, _) = setup();
    expect_that!(
        std::fs::metadata(dir.path().join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        eq(0o700)
    );
    expect_that!(
        std::fs::metadata(dir.path().join("private/state.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        eq(0o600)
    );
    drop(c);
    symlink(dir.path().join("private"), dir.path().join("alias")).unwrap();
    expect_that!(
        Store::open(
            &dir.path().join("alias"),
            Identity::new(),
            &mut Stopped(true)
        )
        .is_err(),
        eq(true)
    );
}
#[gtest]
fn exclusions_are_applied_before_oldest_selection() {
    for mode in [
        "blocked",
        "waiting",
        "hitl",
        "closed",
        "pr",
        "owner",
        "execution",
        "unauthorized",
    ] {
        let mut s = snapshot();
        let i = s.issues.get_mut(&key(2)).unwrap();
        match mode {
            "blocked" => i.blockers_closed = vec![false],
            "waiting" => i.labels.push("needs-info".into()),
            "hitl" => i.labels.push("wayfinder:decision".into()),
            "closed" => i.open = false,
            "pr" => i.pull_request = true,
            "owner" => i.assignees.push("developer".into()),
            "execution" => i.execution_clear = false,
            _ => i.labels.clear(),
        }
        expect_that!(
            policy::select(&s, "controller").unwrap().unwrap().0,
            eq(&key(3))
        );
    }
}
#[gtest]
fn research_authorization_does_not_override_waiting_or_hitl() {
    let (_dir, mut c, r, _) = setup();
    r.0.borrow_mut()
        .snapshot
        .issues
        .get_mut(&key(2))
        .unwrap()
        .labels = vec!["wayfinder:research".into()];
    expect_that!(
        matches!(c.poll().unwrap(),Decision::Selected(a) if a.work==software_factory::model::Work::Research),
        eq(true)
    );
    r.0.borrow_mut()
        .snapshot
        .issues
        .get_mut(&key(2))
        .unwrap()
        .labels
        .push("ready-for-human".into());
    expect_that!(c.poll().is_err(), eq(true));
}
#[gtest]
fn native_order_constrains_only_eligible_siblings() {
    let mut s = snapshot();
    let mut parent = issue(1, "01");
    parent.labels.clear();
    parent.children = vec![key(2), key(3)];
    s.issues.insert(key(1), parent);
    s.issues.get_mut(&key(2)).unwrap().parent = Some(key(1));
    let second = s.issues.get_mut(&key(3)).unwrap();
    second.parent = Some(key(1));
    second.created_at = "2026-01-01T00:00:00Z".into();
    expect_that!(
        policy::select(&s, "controller").unwrap().unwrap().0,
        eq(&key(2))
    );
    s.issues
        .get_mut(&key(2))
        .unwrap()
        .blockers_closed
        .push(false);
    expect_that!(
        policy::select(&s, "controller").unwrap().unwrap().0,
        eq(&key(3))
    );
}
#[gtest]
fn partial_or_contradictory_native_reads_cannot_select() {
    for mode in ["partial", "missing-child", "parent", "cycle"] {
        let (_dir, mut c, r, _) = setup();
        {
            let mut r = r.0.borrow_mut();
            let s = &mut r.snapshot;
            match mode {
                "partial" => s.complete = false,
                "missing-child" => s.issues.get_mut(&key(2)).unwrap().children.push(key(99)),
                "parent" => s.issues.get_mut(&key(2)).unwrap().parent = Some(key(3)),
                _ => {
                    s.issues.get_mut(&key(2)).unwrap().parent = Some(key(3));
                    s.issues.get_mut(&key(3)).unwrap().parent = Some(key(2));
                    s.issues.get_mut(&key(2)).unwrap().children.push(key(3));
                    s.issues.get_mut(&key(3)).unwrap().children.push(key(2));
                }
            }
        }
        expect_that!(c.poll().is_err(), eq(true));
        expect_that!(r.0.borrow().sends, eq(0));
    }
}
#[gtest]
fn completed_action_identity_cannot_be_reused_with_changed_payload() {
    let (_dir, mut c, _, n) = setup();
    c.poll().unwrap();
    let a = action();
    c.perform(a.clone()).unwrap();
    let mut changed = a;
    changed.payload = "different admission".into();
    expect_that!(c.perform(changed).is_err(), eq(true));
    expect_that!(n.0.borrow().sent.len(), eq(1));
}
#[gtest]
fn observed_history_damage_is_a_durable_stop_not_automatic_repair_authority() {
    let (_dir, mut c, r, _) = setup();
    c.poll().unwrap();
    let original = r.0.borrow().comments[0].body.clone();
    r.0.borrow_mut().comments[0].body.push(' ');
    expect_that!(c.prepare_admission().is_err(), eq(true));
    r.0.borrow_mut().comments[0].body = original;
    expect_that!(c.prepare_admission().is_err(), eq(true));
}
#[gtest]
fn controller_polling_reports_sixty_seconds_and_bounds_failed_observations() {
    let (_dir, mut c, r, _) = setup();
    let mut retry = policy::PollPolicy::new(2, 180).unwrap();
    expect_that!(
        c.poll_with_backoff(&mut retry).unwrap().next_poll_seconds,
        eq(60)
    );
    r.0.borrow_mut().snapshot.complete = false;
    let failed = c.poll_with_backoff(&mut retry).unwrap();
    expect_that!(failed.observation.is_err(), eq(true));
    expect_that!(failed.next_poll_seconds, eq(120));
    expect_that!(
        c.poll_with_backoff(&mut retry).unwrap().next_poll_seconds,
        eq(180)
    );
    expect_that!(c.poll_with_backoff(&mut retry).is_err(), eq(true));
}
#[gtest]
fn no_eligible_work_is_idle_and_numeric_ties_do_not_use_project_order() {
    let (_dir, mut c, r, _) = setup();
    for issue in r.0.borrow_mut().snapshot.issues.values_mut() {
        issue.labels.clear();
    }
    expect_that!(matches!(c.poll().unwrap(), Decision::Idle), eq(true));
    expect_that!(r.0.borrow().sends, eq(0));
    let mut s = snapshot();
    let mut ten = issue(10, "01");
    ten.created_at = "2025-12-31T19:00:00-05:00".into();
    s.issues.insert(key(10), ten);
    s.issues.get_mut(&key(2)).unwrap().created_at = "2026-01-01T00:00:00Z".into();
    s.members.insert(0, key(10));
    expect_that!(
        policy::select(&s, "controller").unwrap().unwrap().0,
        eq(&key(2))
    );
}

#[gtest]
fn known_pending_selection_retries_only_transient_reads() {
    let (_dir, mut c, r, _) = setup();
    r.0.borrow_mut().fail_exact = true;
    expect_that!(c.poll().is_err(), eq(true));
    r.0.borrow_mut().fail_exact = false;
    expect_that!(matches!(c.poll().unwrap(), Decision::Retained(_)), eq(true));
    expect_that!(r.0.borrow().sends, eq(1));
}
#[gtest]
fn known_pending_history_damage_stays_stopped_after_restoration() {
    for mode in ["marker", "deleted", "author", "body", "exact"] {
        let (_dir, mut c, r, n) = setup();
        r.0.borrow_mut().fail_exact = true;
        expect_that!(c.poll().is_err(), eq(true));
        let original = r.0.borrow().comments.clone();
        {
            let mut remote = r.0.borrow_mut();
            remote.fail_exact = false;
            match mode {
                "marker" => remote.comments[0].body = "worker summary".into(),
                "deleted" => remote.comments.clear(),
                "author" => remote.comments[0].app = None,
                "body" => remote.comments[0].body.push(' '),
                _ => remote.wrong_exact = true,
            }
        }
        expect_that!(c.prepare_admission().is_err(), eq(true));
        {
            let mut remote = r.0.borrow_mut();
            remote.comments = original;
            remote.wrong_exact = false;
        }
        let (store, _, _) = c.into_parts();
        expect_that!(store.load().unwrap().integrity_stop.is_some(), eq(true));
        let mut c = Controller::new(store, r.clone(), n.clone(), "controller".into()).unwrap();
        expect_that!(c.prepare_admission().is_err(), eq(true));
        expect_that!(r.0.borrow().sends, eq(1));
        expect_that!(n.0.borrow().sent.len(), eq(0));
    }
}

#[gtest]
fn lost_pending_effect_with_unpublished_intent_stops_before_admission() {
    let (dir, mut c, r, _) = setup();
    c.poll().unwrap();
    {
        let mut remote = r.0.borrow_mut();
        remote.drop_send = true;
        remote.lost_ack = true;
    }
    expect_that!(c.perform(action()).is_err(), eq(true));
    let path = dir.path().join("private/state.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["pending_effect"] = serde_json::Value::Null;
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    expect_that!(c.prepare_admission().is_err(), eq(true));
    let (store, _, _) = c.into_parts();
    expect_that!(store.load().unwrap().integrity_stop.is_some(), eq(true));
}

#[gtest]
fn missing_optional_receipt_fields_stop_loading() {
    for field in [
        "active",
        "pending_record",
        "pending_effect",
        "integrity_stop",
    ] {
        let (dir, mut c, _, _) = setup();
        c.poll().unwrap();
        let path = dir.path().join("private/state.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove(field);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let (store, _, _) = c.into_parts();
        expect_that!(store.load().is_err(), eq(true));
    }
}
#[gtest]
fn null_private_authority_cannot_override_authenticated_history() {
    for field in ["active", "pending_effect", "pending_record", "anchors"] {
        let (dir, mut c, r, n) = setup();
        c.poll().unwrap();
        if field == "pending_effect" {
            n.0.borrow_mut().lost_ack = true;
            expect_that!(c.perform(action()).is_err(), eq(true));
        }
        if field == "pending_record" {
            r.0.borrow_mut().fail_exact = true;
            expect_that!(c.wait("CI".into()).is_err(), eq(true));
            r.0.borrow_mut().fail_exact = false;
        }
        let path = dir.path().join("private/state.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value[field] = if field == "anchors" {
            serde_json::json!({})
        } else {
            serde_json::Value::Null
        };
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        expect_that!(c.prepare_admission().is_err(), eq(true));
        expect_that!(c.poll().is_err(), eq(true));
        let (store, _, _) = c.into_parts();
        expect_that!(store.load().unwrap().integrity_stop.is_some(), eq(true));
    }
}

#[gtest]
fn poll_interval_and_failure_backoff_are_bounded_configuration() {
    let mut p = policy::PollPolicy::new(3, 180).unwrap();
    expect_that!(p.observed(true).unwrap(), eq(60));
    expect_that!(p.observed(false).unwrap(), eq(120));
    expect_that!(p.observed(false).unwrap(), eq(180));
    expect_that!(p.observed(false).unwrap(), eq(180));
    expect_that!(p.observed(false).is_err(), eq(true));
    expect_that!(p.observed(true).unwrap(), eq(60));
    expect_that!(policy::PollPolicy::new(0, 180).is_err(), eq(true));
}
