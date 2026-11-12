//! Serialized workflows. No daemon, worker implementation, or activation entry point.
use crate::{
    Result,
    history::{self, Anchor, PendingRecord, Record},
    model::{Action, Active, Comment, Event, Identity, IssueKey, Snapshot},
    policy, require,
    store::{PendingEffect, State, Store},
};

/// Implementations must finish all pages or return an error; no partial success.
pub trait GitHub {
    fn snapshot(&mut self) -> Result<Snapshot>;
    fn history(&mut self, issue: &IssueKey) -> Result<Vec<Comment>>;
    fn publish(&mut self, issue: &IssueKey, body: &str) -> Result<String>;
    fn exact_comment(&mut self, issue: &IssueKey, id: &str) -> Result<Comment>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation {
    Confirmed { action: Action, evidence: String },
    Absent,
    Unknown,
}
/// Native adapters retain their original tool receipts and identities. No blind retries.
pub trait Effects {
    fn send(&mut self, action: &Action) -> Result<()>;
    fn inspect_exact(&mut self, action: &Action) -> Result<Observation>;
}
/// Separate replaceable boundaries for native integrations. Not worker implementations.
pub trait Hermes: Effects {}
pub trait Pool: Effects {}
pub trait Notifications: Effects {}
pub struct NativeEffects<H, P, N> {
    pub hermes: H,
    pub pool: P,
    pub notifications: N,
}
impl<H: Hermes, P: Pool, N: Notifications> Effects for NativeEffects<H, P, N> {
    fn send(&mut self, action: &Action) -> Result<()> {
        match action.kind {
            crate::model::EffectKind::Hermes => self.hermes.send(action),
            crate::model::EffectKind::Pool => self.pool.send(action),
            crate::model::EffectKind::Notification => self.notifications.send(action),
            _ => Err(crate::Error::Stop(
                "effect requires separately implemented supported adapter".into(),
            )),
        }
    }
    fn inspect_exact(&mut self, action: &Action) -> Result<Observation> {
        match action.kind {
            crate::model::EffectKind::Hermes => self.hermes.inspect_exact(action),
            crate::model::EffectKind::Pool => self.pool.inspect_exact(action),
            crate::model::EffectKind::Notification => self.notifications.inspect_exact(action),
            _ => Err(crate::Error::Stop(
                "effect requires separately implemented supported adapter".into(),
            )),
        }
    }
}
#[derive(Clone, Debug)]
pub enum Decision {
    Idle,
    Retained(Active),
    Selected(Active),
}
pub struct PollObservation {
    pub observation: Result<Decision>,
    pub next_poll_seconds: u64,
}
pub struct Controller<G, E> {
    store: Store,
    github: G,
    effects: E,
    assignee: String,
    poisoned: bool,
}
impl<G: GitHub, E: Effects> Controller<G, E> {
    pub fn new(store: Store, github: G, effects: E, assignee: String) -> Result<Self> {
        require(!assignee.is_empty(), "missing configured assignee")?;
        store.load()?;
        Ok(Self {
            store,
            github,
            effects,
            assignee,
            poisoned: false,
        })
    }
    pub fn into_parts(self) -> (Store, G, E) {
        (self.store, self.github, self.effects)
    }
    fn load(&self) -> Result<State> {
        require(
            !self.poisoned,
            "receipt write failed; restart with stopped-owner handoff",
        )?;
        let state = self.store.load()?;
        require(
            state.integrity_stop.is_none(),
            "durable history-integrity stop; maintainer reconciliation required",
        )?;
        Ok(state)
    }
    fn save(&mut self, state: &State) -> Result<()> {
        let result = self.store.save(state);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn records(&mut self, state: &State, issue: &IssueKey) -> Result<Vec<Record>> {
        let comments = self.github.history(issue)?;
        self.validate_comments(state, issue, &comments)
    }
    fn validate_comments(
        &mut self,
        state: &State,
        issue: &IssueKey,
        comments: &[Comment],
    ) -> Result<Vec<Record>> {
        let result = history::validate(
            comments,
            issue,
            &state.app,
            state.anchors.get(issue).map(Vec::as_slice).unwrap_or(&[]),
            state.pending_record.as_ref().filter(|p| &p.issue == issue),
        );
        if result.is_err() {
            let mut stopped = self.store.load()?;
            stopped.integrity_stop = Some(format!(
                "history integrity unresolved for {}",
                issue.as_str()
            ));
            self.save(&stopped)?;
        }
        result
    }
    fn stop_integrity<T>(&mut self, state: &State, message: &str) -> Result<T> {
        let mut stopped = state.clone();
        stopped.integrity_stop = Some(message.into());
        self.save(&stopped)?;
        Err(crate::Error::Stop(message.into()))
    }
    fn validate_all(&mut self, state: &State) -> Result<()> {
        let mut keys: std::collections::BTreeSet<_> = state.anchors.keys().cloned().collect();
        keys.extend(state.active.iter().map(|a| a.issue.clone()));
        keys.extend(state.pending_record.iter().map(|p| p.issue.clone()));
        keys.extend(state.pending_effect.iter().map(|p| p.action.issue.clone()));
        let mut selected = Vec::new();
        let mut intents = Vec::new();
        let mut outcomes = Vec::new();
        for key in keys {
            for record in self.records(state, &key)? {
                match record.event {
                    Event::Selected { job, work, .. } => selected.push((key.clone(), job, work)),
                    Event::Intent(action) => intents.push(action),
                    Event::Outcome { action, .. } => outcomes.push(action),
                    _ => {}
                }
            }
        }
        // Pending transitions must agree with private authority even before publication.
        // They are not confirmed outcomes or a replacement for authenticated history.
        if let Some(pending) = &state.pending_record {
            let record = pending
                .body
                .strip_prefix(history::PREFIX)
                .and_then(|body| serde_json::from_str::<Record>(body).ok());
            let Some(record) = record else {
                return self.stop_integrity(state, "invalid pending history record");
            };
            if record.id != pending.record || record.issue != pending.issue {
                return self.stop_integrity(state, "pending history identity mismatch");
            }
            match record.event {
                Event::Selected { job, work, .. } if selected.is_empty() => {
                    selected.push((pending.issue.clone(), job, work));
                }
                Event::Intent(action) if !intents.contains(&action) => intents.push(action),
                _ => {}
            }
        }
        let selection_matches = match &state.active {
            Some(active) => {
                selected.len() == 1
                    && selected[0]
                        == (
                            active.issue.clone(),
                            active.job.clone(),
                            active.work.clone(),
                        )
            }
            None => selected.is_empty(),
        };
        let unresolved: Vec<_> = intents
            .iter()
            .filter(|a| !outcomes.contains(&a.id))
            .collect();
        let effects_match = unresolved.iter().all(|a| {
            state
                .pending_effect
                .as_ref()
                .is_some_and(|p| &p.action == *a)
        }) && unresolved.len() <= 1
            && state.pending_effect.as_ref().is_none_or(|p| {
                state
                    .active
                    .as_ref()
                    .is_some_and(|a| a.issue == p.action.issue)
                    && (!p.attempted || intents.iter().any(|a| a == &p.action))
            });
        if !selection_matches || !effects_match {
            return self
                .stop_integrity(state, "private authority contradicts authenticated history");
        }
        Ok(())
    }
    fn append(&mut self, state: &mut State, issue: &IssueKey, event: Event) -> Result<()> {
        require(
            state.pending_record.is_none(),
            "pending history must be reconciled first",
        )?;
        self.records(state, issue)?;
        let anchors = state.anchors.get(issue).map(Vec::as_slice).unwrap_or(&[]);
        let record = Record {
            id: Identity::new(),
            issue: issue.clone(),
            sequence: anchors.len(),
            previous_digest: anchors.last().map(|a| a.digest.clone()),
            event,
        };
        state.pending_record = Some(PendingRecord {
            issue: issue.clone(),
            record: record.id.clone(),
            body: record.body()?,
            attempted: false,
            comment: None,
        });
        self.save(state)?; // exact payload exists before publication can begin
        self.finish_record(state)
    }
    fn finish_record(&mut self, state: &mut State) -> Result<()> {
        let pending = state
            .pending_record
            .clone()
            .ok_or_else(|| crate::Error::Stop("missing pending record".into()))?;
        let comments = self.github.history(&pending.issue)?;
        self.validate_comments(state, &pending.issue, &comments)?;
        let matching: Vec<_> = comments.iter().filter(|c| c.body == pending.body).collect();
        require(matching.len() <= 1, "duplicate pending record")?;
        let id = if let Some(found) = matching.first() {
            found.id.clone()
        } else {
            require(
                !pending.attempted,
                "uncertain history publication absent; do not repeat",
            )?;
            state.pending_record.as_mut().expect("pending").attempted = true;
            self.save(state)?;
            let id = self.github.publish(&pending.issue, &pending.body)?;
            require(!id.is_empty(), "missing publication identity")?;
            state.pending_record.as_mut().expect("pending").comment = Some(id.clone());
            self.save(state)?;
            id
        };
        if pending
            .comment
            .as_ref()
            .is_some_and(|expected| expected != &id)
        {
            return self.stop_integrity(state, "pending comment identity changed");
        }
        if state
            .pending_record
            .as_ref()
            .is_some_and(|record| record.comment.is_none())
        {
            state.pending_record.as_mut().expect("pending").comment = Some(id.clone());
            self.save(state)?;
        }
        let exact = self.github.exact_comment(&pending.issue, &id)?;
        if exact.id != id
            || exact.issue != pending.issue
            || exact.app.as_ref() != Some(&state.app)
            || exact.body != pending.body
        {
            return self.stop_integrity(state, "exact history readback mismatch");
        }
        // Re-read full continuity, including the exact readback identity, before advancing.
        let comments = self.github.history(&pending.issue)?;
        self.validate_comments(state, &pending.issue, &comments)?;
        require(
            comments
                .iter()
                .any(|c| c.id == id && c.body == pending.body),
            "published record absent from complete history",
        )?;
        state
            .anchors
            .entry(pending.issue.clone())
            .or_default()
            .push(Anchor {
                comment: id,
                record: pending.record,
                digest: history::digest(&pending.body),
            });
        state.pending_record = None;
        self.save(state)
    }
    /// Reconcile pending exact targets. Absence after uncertain send never grants retry authority.
    pub fn recover(&mut self) -> Result<()> {
        let mut state = self.load()?;
        self.validate_all(&state)?;
        if state.pending_record.is_some() {
            self.finish_record(&mut state)?;
        }
        if state.pending_effect.as_ref().is_some_and(|p| p.attempted) {
            self.finish_effect(&mut state)?;
        }
        if let Some(active) = state.active.clone() {
            let records = self.records(&state, &active.issue)?;
            require(records.iter().any(|r| matches!(&r.event,Event::Selected {job,work,..} if job == &active.job && work == &active.work)),"active job has no authenticated selection")?;
            while let Some(handoff) = state.handoffs.first().cloned() {
                let event = Event::Ownership {
                    previous: handoff.previous,
                    owner: handoff.owner,
                    evidence: handoff.evidence,
                };
                if !self
                    .records(&state, &active.issue)?
                    .iter()
                    .any(|r| r.event == event)
                {
                    self.append(&mut state, &active.issue, event)?;
                }
                state.handoffs.remove(0);
                self.save(&state)?;
            }
        } else {
            require(state.handoffs.is_empty(), "orphan ownership handoff")?;
        }
        Ok(())
    }
    /// The host scheduler consumes this delay. This library starts no service.
    pub fn poll_with_backoff(
        &mut self,
        policy: &mut policy::PollPolicy,
    ) -> Result<PollObservation> {
        let observation = self.poll();
        let next_poll_seconds = policy.observed(observation.is_ok())?;
        Ok(PollObservation {
            observation,
            next_poll_seconds,
        })
    }
    pub fn poll(&mut self) -> Result<Decision> {
        self.recover()?;
        let mut state = self.load()?;
        let snapshot = self.github.snapshot()?;
        policy::validate(&snapshot)?;
        if let Some(active) = state.active.clone() {
            self.check_eligible(&snapshot, &active)?;
            return Ok(Decision::Retained(active));
        }
        require(state.pending_effect.is_none(), "orphan pending action")?;
        // History uncertainty excludes admission, even for otherwise eligible candidates.
        for key in &snapshot.members {
            self.records(&state, key)?;
        }
        let Some((issue, work)) = policy::select(&snapshot, &self.assignee)? else {
            return Ok(Decision::Idle);
        };
        let active = Active {
            issue: issue.clone(),
            job: Identity::new(),
            work: work.clone(),
        };
        state.active = Some(active.clone());
        // Selection ownership and exact history intent are persisted in one local transition.
        let owner = state.owner.clone();
        self.append(
            &mut state,
            &issue,
            Event::Selected {
                job: active.job.clone(),
                work,
                owner,
            },
        )?;
        let fresh = self.github.snapshot()?;
        self.check_eligible(&fresh, &active)?;
        Ok(Decision::Selected(active))
    }
    fn check_eligible(&self, snapshot: &Snapshot, active: &Active) -> Result<()> {
        policy::validate(snapshot)?;
        require(
            snapshot.members.contains(&active.issue),
            "active issue left Project",
        )?;
        let issue = snapshot
            .issues
            .get(&active.issue)
            .ok_or_else(|| crate::Error::Stop("missing active issue".into()))?;
        require(
            policy::authorization(issue, &self.assignee).as_ref() == Some(&active.work),
            "active issue authorization or ownership changed; retain slot",
        )
    }
    /// An intent seam, not a worker launcher. Call immediately before native admission.
    pub fn prepare_admission(&mut self) -> Result<Active> {
        self.recover()?;
        let state = self.load()?;
        require(
            state.pending_record.is_none() && state.pending_effect.is_none(),
            "unresolved action blocks admission",
        )?;
        let active = state
            .active
            .ok_or_else(|| crate::Error::Stop("no selected job".into()))?;
        let snapshot = self.github.snapshot()?;
        self.check_eligible(&snapshot, &active)?;
        Ok(active)
    }
    /// Controller-level effect contract. Production integrations remain separately gated.
    pub fn perform(&mut self, action: Action) -> Result<()> {
        self.recover()?;
        let mut state = self.load()?;
        let active = state
            .active
            .clone()
            .ok_or_else(|| crate::Error::Stop("no active issue".into()))?;
        require(
            action.issue == active.issue && !action.target.is_empty() && !action.payload.is_empty(),
            "invalid exact action target",
        )?;
        let records = self.records(&state, &active.issue)?;
        let previous_intents: Vec<_> = records
            .iter()
            .filter_map(|r| {
                if let Event::Intent(a) = &r.event {
                    Some(a)
                } else {
                    None
                }
            })
            .filter(|a| a.id == action.id)
            .collect();
        require(
            previous_intents.len() <= 1 && previous_intents.first().is_none_or(|a| **a == action),
            "changed payload reused an original action identity",
        )?;
        if records
            .iter()
            .any(|r| matches!(&r.event,Event::Outcome {action:id,..} if id == &action.id))
        {
            require(
                previous_intents.len() == 1,
                "outcome without original intent",
            )?;
            return Ok(());
        }
        if let Some(pending) = &state.pending_effect {
            require(pending.action == action, "another action retains the slot")?;
        } else {
            state.pending_effect = Some(PendingEffect {
                action: action.clone(),
                attempted: false,
            });
            self.save(&state)?;
        }
        if previous_intents.is_empty() {
            self.append(&mut state, &active.issue, Event::Intent(action.clone()))?;
        }
        self.validate_all(&state)?;
        let fresh = self.github.snapshot()?;
        self.check_eligible(&fresh, &active)?;
        require(
            !state.pending_effect.as_ref().expect("pending").attempted,
            "uncertain action cannot be repeated",
        )?;
        state.pending_effect.as_mut().expect("pending").attempted = true;
        self.save(&state)?;
        self.effects.send(&action)?;
        self.finish_effect(&mut state)
    }
    fn finish_effect(&mut self, state: &mut State) -> Result<()> {
        let action = state
            .pending_effect
            .as_ref()
            .ok_or_else(|| crate::Error::Stop("missing pending effect".into()))?
            .action
            .clone();
        let records = self.records(state, &action.issue)?;
        let intents: Vec<_> = records
            .iter()
            .filter_map(|r| {
                if let Event::Intent(a) = &r.event {
                    Some(a)
                } else {
                    None
                }
            })
            .filter(|a| a.id == action.id)
            .collect();
        require(
            intents.len() == 1 && intents[0] == &action,
            "missing authenticated original action intent",
        )?;
        if records
            .iter()
            .any(|r| matches!(&r.event,Event::Outcome {action:id,..} if id == &action.id))
        {
            state.pending_effect = None;
            return self.save(state);
        }
        match self.effects.inspect_exact(&action)? {
            Observation::Confirmed {
                action: found,
                evidence,
            } => {
                require(
                    found == action && !evidence.is_empty(),
                    "effect readback target or identity mismatch",
                )?;
                self.append(
                    state,
                    &action.issue,
                    Event::Outcome {
                        action: action.id,
                        evidence,
                    },
                )?;
                state.pending_effect = None;
                self.save(state)
            }
            Observation::Absent | Observation::Unknown => Err(crate::Error::Stop(
                "effect uncertain; preserve original identity and inspect exact target".into(),
            )),
        }
    }
    pub fn wait(&mut self, reason: String) -> Result<()> {
        require(!reason.is_empty(), "missing wait reason")?;
        self.recover()?;
        let mut state = self.load()?;
        require(
            state.pending_effect.is_none(),
            "uncertain effect blocks wait advancement",
        )?;
        let issue = state
            .active
            .as_ref()
            .ok_or_else(|| crate::Error::Stop("no active issue".into()))?
            .issue
            .clone();
        self.append(&mut state, &issue, Event::Wait { reason })
    }
}
