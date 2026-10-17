//! Privacy allowlist at the application boundary; delivery never blocks business work.
use std::{io::Write, sync::mpsc, time::Instant};

use serde::Serialize;

use crate::actions::ActionId;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Authentication,
    Catalog,
    Persistence,
    Checking,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Confirmed,
    Failed,
    Unconfirmed,
    Limited,
    Unavailable,
    Timeout,
    Malformed,
    Oversized,
}

#[derive(Serialize)]
struct Fact {
    category: Category,
    outcome: Outcome,
    duration_ms: f64,
    release: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

pub struct Diagnostics {
    sender: mpsc::SyncSender<Fact>,
    release: String,
}

impl Diagnostics {
    pub fn start(release: String) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Fact>(256);
        // A blocked or broken diagnostic destination cannot hold a request or
        // shutdown open. The bounded queue may drop records; history is authoritative.
        let _ = std::thread::Builder::new()
            .name("book-smartz-diagnostics".into())
            .spawn(move || {
                for fact in receiver {
                    let Ok(mut record) = serde_json::to_vec(&fact) else {
                        continue;
                    };
                    record.push(b'\n');
                    if std::io::stderr().write_all(&record).is_err() {
                        break;
                    }
                }
            });
        Self { sender, release }
    }

    pub fn record(
        &self,
        category: Category,
        outcome: Outcome,
        started: Instant,
        action: Option<ActionId>,
    ) {
        let _ = self.sender.try_send(Fact {
            category,
            outcome,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            release: self.release.clone(),
            action: action.map(ActionId::text),
        });
    }

    pub fn unconfirmed_save(&self, started: Instant, action: ActionId) {
        for category in [Category::Persistence, Category::Checking] {
            self.record(category, Outcome::Unconfirmed, started, Some(action));
        }
    }

    pub fn scope(&self, category: Category) -> Scope<'_> {
        Scope {
            diagnostics: self,
            category,
            outcome: Outcome::Failed,
            started: Instant::now(),
        }
    }
}

pub struct Scope<'a> {
    diagnostics: &'a Diagnostics,
    category: Category,
    outcome: Outcome,
    started: Instant,
}
impl Scope<'_> {
    pub const fn outcome(&mut self, outcome: Outcome) {
        self.outcome = outcome;
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.diagnostics
            .record(self.category, self.outcome, self.started, None);
    }
}
