//! Serialized intake, native worker, and recovery workflows. No daemon or activation entry point.
use crate::{
    Result,
    history::{self, Anchor, PendingRecord, Record},
    model::{Action, Active, Comment, Event, Identity, IssueKey, Snapshot},
    policy, require,
    store::{PendingEffect, State, Store},
};

/// Implementations must finish all pages or return an error; no partial success.
pub trait GitHub {
    ///
    /// # Errors
    ///
    /// Returns an error if the complete Project and native issue reads cannot be established.
    fn snapshot(&mut self) -> Result<Snapshot>;
    ///
    /// # Errors
    ///
    /// Returns an error if the complete history for the exact issue cannot be read.
    fn history(&mut self, issue: &IssueKey) -> Result<Vec<Comment>>;
    ///
    /// # Errors
    ///
    /// Returns an error if publication fails or its acknowledgement is uncertain.
    /// An error does not authorize another send.
    fn publish(&mut self, issue: &IssueKey, body: &str) -> Result<String>;
    ///
    /// # Errors
    ///
    /// Returns an error if the exact comment cannot be read or its target is invalid.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the native effect fails or its acknowledgement is uncertain.
    /// An error does not authorize another send.
    fn send(&mut self, action: &Action) -> Result<()>;
    ///
    /// # Errors
    ///
    /// Returns an error if the original exact action target cannot be inspected.
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
    ///
    /// # Errors
    ///
    /// Returns an error if the assignee is empty or the private store cannot be loaded.
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
            state.anchors.get(issue).map_or(&[][..], Vec::as_slice),
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
        let mut worker_checkpoint = None;
        for key in keys {
            for record in self.records(state, &key)? {
                match record.event {
                    Event::Selected { job, work, .. } => selected.push((key.clone(), job, work)),
                    Event::Intent(action) => intents.push(action),
                    Event::Outcome { action, .. } => outcomes.push(action),
                    Event::WorkerCheckpoint { run, digest, phase } => {
                        worker_checkpoint = Some((key.clone(), run, digest, phase));
                    }
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
            let identity_matches = record.id == pending.record;
            let issue_matches = record.issue == pending.issue;
            if !identity_matches || !issue_matches {
                return self.stop_integrity(state, "pending history identity mismatch");
            }
            match record.event {
                Event::Selected { job, work, .. } if selected.is_empty() => {
                    selected.push((pending.issue.clone(), job, work));
                }
                Event::Intent(action) if !intents.contains(&action) => intents.push(action),
                Event::WorkerCheckpoint { run, digest, phase } => {
                    worker_checkpoint = Some((pending.issue.clone(), run, digest, phase));
                }
                _ => {}
            }
        }
        let selection_matches = state.active.as_ref().map_or(selected.is_empty(), |active| {
            selected.len() == 1
                && selected[0]
                    == (
                        active.issue.clone(),
                        active.job.clone(),
                        active.work.clone(),
                    )
        });
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
        let worker_matches_job = state.worker.as_ref().is_none_or(|worker| {
            state
                .active
                .as_ref()
                .is_some_and(|a| a.issue == worker.plan.issue && a.job == worker.plan.job)
        });
        let expected_worker = state
            .worker
            .as_ref()
            .map(|worker| -> Result<_> {
                Ok((
                    worker.plan.issue.clone(),
                    worker.plan.run.clone(),
                    worker.digest()?,
                    worker.phase.clone(),
                ))
            })
            .transpose()?;
        if !selection_matches
            || !effects_match
            || !worker_matches_job
            || worker_checkpoint != expected_worker
        {
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
        let anchors = state.anchors.get(issue).map_or(&[][..], Vec::as_slice);
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
    ///
    /// # Errors
    ///
    /// Returns an error on receipt failure, invalid history, unresolved publication,
    /// unconfirmed effects, or an invalid ownership handoff. Original identities are retained.
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
    ///
    /// # Errors
    ///
    /// Returns an error when the configured polling failure allowance is exhausted.
    /// The observation retains any error from the poll itself.
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
    ///
    /// # Errors
    ///
    /// Returns an error if recovery, complete reads, history integrity, persistence,
    /// or active-job authorization cannot be established.
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
    ///
    /// # Errors
    ///
    /// Returns an error if recovery fails, an action is unresolved, no job is selected,
    /// or fresh eligibility cannot be established.
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
    ///
    /// # Errors
    ///
    /// Returns an error if recovery, exact action identity, authenticated intent,
    /// fresh authorization, persistence, native send, or exact readback fails.
    ///
    /// # Panics
    ///
    /// Panics if the pending effect disappears after it is established in this
    /// serialized transition. This indicates an internal invariant failure.
    pub fn perform(&mut self, action: &Action) -> Result<()> {
        self.perform_checked(action, true)
    }
    fn perform_checked(&mut self, action: &Action, require_eligibility: bool) -> Result<()> {
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
            previous_intents.len() <= 1 && previous_intents.first().is_none_or(|a| *a == action),
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
            require(&pending.action == action, "another action retains the slot")?;
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
        if require_eligibility {
            let fresh = self.github.snapshot()?;
            self.check_eligible(&fresh, &active)?;
        } else {
            let worker = state
                .worker
                .as_ref()
                .ok_or_else(|| crate::Error::Stop("cancellation without worker".into()))?;
            let expected = crate::worker::WorkerCommand::Cancel {
                worker: worker.identity()?,
            }
            .action(&worker.plan.cancel, &worker.plan.issue)?;
            require(
                worker.phase == crate::worker::WorkerPhase::Cancelling && action == &expected,
                "only exact cancellation bypasses intake authorization",
            )?;
        }
        require(
            !state.pending_effect.as_ref().expect("pending").attempted,
            "uncertain action cannot be repeated",
        )?;
        state.pending_effect.as_mut().expect("pending").attempted = true;
        self.save(&state)?;
        self.effects.send(action)?;
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
    ///
    /// # Errors
    ///
    /// Returns an error if the reason is empty, recovery fails, an effect is unresolved,
    /// no job is active, or the wait record cannot be persisted and verified.
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

impl<G: GitHub, E: crate::worker::WorkerRuntime> Controller<G, E> {
    /// Publish only after implementation, then retain the slot for independent reviews and CI.
    ///
    /// # Errors
    /// Rejects a live/blocked worker, malformed publication, or uncertain/mismatched remote PR.
    pub fn publish_worker_pr(
        &mut self,
        request: crate::worker::PublicationRequest,
    ) -> Result<crate::worker::WorkerRecord> {
        use crate::worker::{WorkerCommand, WorkerPhase};
        self.prepare_admission()?;
        let mut worker = self
            .load()?
            .worker
            .ok_or_else(|| crate::Error::Stop("no implementation worker".into()))?;
        require(
            matches!(
                worker.phase,
                WorkerPhase::Implemented | WorkerPhase::ReviewCiWait
            ),
            "implementation is not successfully terminal; no PR publication",
        )?;
        require(
            !request.branch.is_empty()
                && request.branch != "main"
                && request
                    .branch
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
                && request.head.len() == 40
                && request
                    .head
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                && !request.title.trim().is_empty(),
            "invalid exact publication branch/head/title",
        )?;
        let expected_head = request.head.clone();
        let expected_branch = request.branch.clone();
        let action = WorkerCommand::Publish {
            request,
            intent: worker.plan.publication.clone(),
        }
        .action(&worker.plan.publication, &worker.plan.issue)?;
        self.perform(&action)?;
        let publication: crate::worker::PublicationResult = self.worker_outcome(&action)?;
        require(
            publication.intent == worker.plan.publication
                && publication.head == expected_head
                && publication.branch == expected_branch
                && publication.base == "main"
                && publication.number > 0
                && !publication.node_id.is_empty()
                && publication.url
                    == format!(
                        "https://github.com/LowkeyLab/bazel-repo/pull/{}",
                        publication.number
                    ),
            "exact remote PR differs from publication intent",
        )?;
        worker.publication = Some(publication);
        worker.phase = WorkerPhase::ReviewCiWait;
        self.checkpoint_worker(worker.clone())?;
        Ok(worker)
    }
    /// Request cancellation; an acknowledgement alone never permits replacement or release.
    ///
    /// # Errors
    /// Requires reconciled history, a known exact worker, and confirmed cessation/effects.
    pub fn cancel_worker(&mut self, reason: String) -> Result<crate::worker::WorkerRecord> {
        use crate::worker::{WorkerCommand, WorkerPhase};
        require(!reason.is_empty(), "missing cancellation reason")?;
        self.recover()?;
        let mut worker = self
            .load()?
            .worker
            .ok_or_else(|| crate::Error::Stop("no worker to cancel".into()))?;
        if worker.phase == WorkerPhase::Stopped {
            return Ok(worker);
        }
        let identity = worker.identity()?;
        if worker.phase != WorkerPhase::Cancelling {
            worker.phase = WorkerPhase::Cancelling;
            worker.stop_reason = Some(reason);
            self.checkpoint_worker(worker.clone())?;
        }
        let action = WorkerCommand::Cancel {
            worker: identity.clone(),
        }
        .action(&worker.plan.cancel, &worker.plan.issue)?;
        self.perform_checked(&action, false)?;
        if let Some(evidence) = self.effects.stopped_effects(&identity)? {
            require(
                !evidence.is_empty(),
                "empty exact cessation/effects evidence",
            )?;
            worker.cessation = Some(evidence);
            worker.phase = WorkerPhase::Stopped;
            self.checkpoint_worker(worker.clone())?;
        }
        Ok(worker)
    }
    fn checkpoint_worker(&mut self, worker: crate::worker::WorkerRecord) -> Result<()> {
        let mut state = self.load()?;
        let issue = worker.plan.issue.clone();
        let event = Event::WorkerCheckpoint {
            run: worker.plan.run.clone(),
            digest: worker.digest()?,
            phase: worker.phase.clone(),
        };
        state.worker = Some(worker);
        self.append(&mut state, &issue, event)
    }
    fn worker_outcome<T: serde::de::DeserializeOwned>(&mut self, action: &Action) -> Result<T> {
        let state = self.load()?;
        let record = self
            .records(&state, &action.issue)?
            .into_iter()
            .find_map(|r| match r.event {
                Event::Outcome {
                    action: id,
                    evidence,
                } if id == action.id => Some(evidence),
                _ => None,
            })
            .ok_or_else(|| crate::Error::Stop("missing exact worker action outcome".into()))?;
        Ok(serde_json::from_str(&record)?)
    }
    fn advance_worker_admission(
        &mut self,
        worker: &mut crate::worker::WorkerRecord,
        issue_url: &str,
    ) -> Result<()> {
        use crate::worker::{WorkerCommand, WorkerPhase};
        if worker.phase == WorkerPhase::Prepared {
            let command = WorkerCommand::Create {
                request_id: worker.plan.create.clone(),
                profile: worker.plan.request.profile.clone(),
                principal: worker.plan.request.principal.clone(),
                cwd: worker.plan.request.pool.gateway_cwd()?.into(),
                skills: worker.plan.request.mandatory_skills.clone(),
                launch_digest: crate::history::digest(&serde_json::to_string(&worker.plan.launch)?),
            };
            let action = command.action(&worker.plan.create, &worker.plan.issue)?;
            self.perform(&action)?;
            let session: crate::worker::SessionEvidence = self.worker_outcome(&action)?;
            require(
                !session.session_id.is_empty()
                    && session.session_id == session.stored_session_id
                    && session.profile == worker.plan.request.profile
                    && session.principal == worker.plan.request.principal
                    && session.creation_id == worker.plan.create
                    && !session.frozen_binding.is_empty()
                    && session.loaded_skills == worker.plan.launch.skills,
                "native session or actual frozen skill content does not match intent",
            )?;
            worker.session = Some(session);
            worker.phase = WorkerPhase::Created;
            self.checkpoint_worker(worker.clone())?;
        }
        if worker.phase == WorkerPhase::Created {
            let session = worker
                .session
                .as_ref()
                .ok_or_else(|| crate::Error::Stop("missing created session".into()))?;
            let text = format!(
                "Use the explicitly preloaded implement skill with issue {issue_url}. Preserve the mandatory review-before-repair gate. Do not correct a failed internal review without durable shared-budget authorization. Do not publish a PR; the controller owns a separate exact publication intent."
            );
            let action = WorkerCommand::Submit {
                session_id: session.session_id.clone(),
                submission_id: worker.plan.submit.clone(),
                text,
            }
            .action(&worker.plan.submit, &worker.plan.issue)?;
            self.perform(&action)?;
            let admission: crate::worker::Admission = self.worker_outcome(&action)?;
            worker.admission = Some(admission);
            if worker
                .admission
                .as_ref()
                .is_some_and(|a| a.status == "terminal")
            {
                worker.admission = Some(self.effects.observe(&worker.identity()?)?);
            }
            worker
                .admission
                .as_ref()
                .ok_or_else(|| crate::Error::Stop("missing admitted worker".into()))?
                .validate(&worker.identity()?)?;
            worker.phase = WorkerPhase::Running;
            self.checkpoint_worker(worker.clone())?;
        }
        Ok(())
    }
    /// Admit and observe one native implementation worker, retaining the originating slot.
    ///
    /// # Errors
    /// Any missing prerequisite, uncertain effect, history failure, or changed identity stops.
    /// This does not authorize repair, replacement, release, merge, or activation.
    pub fn execute_worker(
        &mut self,
        request: crate::worker::WorkerRequest,
    ) -> Result<crate::worker::WorkerRecord> {
        use crate::worker::{WorkerPhase, WorkerPlan, WorkerRecord};
        let active = self.prepare_admission()?;
        require(
            active.work == crate::model::Work::Implementation,
            "research is not implementation authority",
        )?;
        let issue_url = format!(
            "https://github.com/{}/issues/{}",
            active.issue.repository(),
            active.issue.number()
        );
        let mut worker = if let Some(worker) = self.load()?.worker {
            require(
                worker.plan.request == request
                    && worker.plan.issue == active.issue
                    && worker.plan.job == active.job,
                "retained worker identity or original pool receipt changed",
            )?;
            worker
        } else {
            let launch = self.effects.prepare(&request, &issue_url)?;
            launch.validate(&request)?;
            let worker = WorkerRecord {
                plan: WorkerPlan {
                    run: Identity::new(),
                    create: Identity::new(),
                    submit: Identity::new(),
                    cancel: Identity::new(),
                    publication: Identity::new(),
                    issue: active.issue.clone(),
                    job: active.job,
                    request,
                    launch,
                },
                phase: WorkerPhase::Prepared,
                session: None,
                admission: None,
                stop_reason: None,
                cessation: None,
                publication: None,
            };
            self.checkpoint_worker(worker.clone())?;
            worker
        };
        if matches!(
            worker.phase,
            WorkerPhase::RepairBlocked
                | WorkerPhase::Cancelling
                | WorkerPhase::Stopped
                | WorkerPhase::ReviewCiWait
        ) {
            return Ok(worker);
        }
        let current = self.effects.prepare(&worker.plan.request, &issue_url)?;
        current.validate(&worker.plan.request)?;
        require(
            current.skills == worker.plan.launch.skills
                && current.review_gate == worker.plan.launch.review_gate
                && current.gateway.profile_id == worker.plan.launch.gateway.profile_id,
            "frozen skill/gate/profile evidence changed; retain worker",
        )?;
        self.advance_worker_admission(&mut worker, &issue_url)?;
        if worker.phase == WorkerPhase::Running {
            let identity = worker.identity()?;
            match self.effects.boundary(&identity)? {
                crate::worker::Boundary::Clear => {}
                crate::worker::Boundary::ReviewFailed { gate, findings } => {
                    require(
                        gate == worker.plan.launch.review_gate && !findings.is_empty(),
                        "unbound review boundary evidence",
                    )?;
                    worker.phase = WorkerPhase::RepairBlocked;
                    worker.stop_reason = Some(format!(
                        "internal review failed at {gate}; durable shared-budget repair authorization required: {findings}"
                    ));
                    self.checkpoint_worker(worker.clone())?;
                    return Ok(worker);
                }
            }
            let admission = self.effects.observe(&identity)?;
            admission.validate(&identity)?;
            if admission.status == "terminal" {
                worker.phase = if admission.outcome.as_deref() == Some("completed") {
                    WorkerPhase::Implemented
                } else {
                    WorkerPhase::RepairBlocked
                };
                if worker.phase == WorkerPhase::RepairBlocked {
                    worker.stop_reason = Some(
                        "native turn did not complete; no automatic replacement or repair".into(),
                    );
                }
            }
            if worker.admission.as_ref() != Some(&admission) || worker.phase != WorkerPhase::Running
            {
                worker.admission = Some(admission);
                self.checkpoint_worker(worker.clone())?;
            }
        }
        Ok(worker)
    }
}
