//! Diagnostic facts are fallible local delivery; they never own catalog outcomes.
use std::io::{self, Write};

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandObserved {
    pub command: String,
    pub outcome: String,
    pub reason_code: String,
    pub catalog_id: Option<String>,
    pub revision: Option<u64>,
}
pub trait Listener {
    /// # Errors
    /// Returns a delivery failure without changing the observed command.
    fn observe(&mut self, fact: &CommandObserved) -> io::Result<()>;
}
pub struct Diagnostics {
    listeners: Vec<(&'static str, Box<dyn Listener>)>,
}
impl Diagnostics {
    #[must_use]
    pub fn new(listeners: Vec<(&'static str, Box<dyn Listener>)>) -> Self {
        Self { listeners }
    }
    #[must_use]
    pub fn local(json: bool) -> Self {
        Self::new(vec![
            ("stderr", Box::new(StderrListener { json })),
            ("tracing", Box::new(TracingListener)),
        ])
    }
    pub fn observe(&mut self, fact: &CommandObserved) -> Vec<&'static str> {
        let mut warnings = Vec::new();
        for (id, listener) in &mut self.listeners {
            if listener.observe(fact).is_err() {
                // Direct fallback cannot recurse into a failed listener or stop independent delivery.
                let _ = writeln!(
                    io::stderr(),
                    "worktree-pool: diagnostic_listener_failed listener={id}"
                );
                if warnings.is_empty() {
                    warnings.push("diagnostic_listener_failed");
                }
            }
        }
        warnings
    }
}
struct StderrListener {
    json: bool,
}
impl Listener for StderrListener {
    fn observe(&mut self, fact: &CommandObserved) -> io::Result<()> {
        if fact.outcome == "completed" {
            return Ok(());
        }
        let mut stderr = io::stderr().lock();
        if self.json {
            serde_json::to_writer(&mut stderr, fact).map_err(io::Error::other)?;
            writeln!(stderr)
        } else {
            writeln!(
                stderr,
                "worktree-pool: {} {} ({})",
                fact.command, fact.outcome, fact.reason_code
            )
        }
    }
}
struct TracingListener;
impl Listener for TracingListener {
    fn observe(&mut self, fact: &CommandObserved) -> io::Result<()> {
        tracing::info!(command = %fact.command, outcome = %fact.outcome, reason_code = %fact.reason_code,
            catalog_id = ?fact.catalog_id, revision = ?fact.revision, "command observed");
        Ok(())
    }
}
