//! Transient publication facts. This module never stores request or batch contents.
use crate::plan::{Batch, IssueNumber, TicketKey, TicketStatus};
use std::{
    cell::{Cell, RefCell},
    io::Write,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    RemoteChecks,
    Creation,
    ParentLinks,
    BlockerLinks,
    Labels,
    FinalVerification,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    ReadIssue,
    EnumerateIssues,
    CreateIssue,
    ReadRelationships,
    ReadParent,
    AddParent,
    AddBlocker,
    AddLabels,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    RemoteChecks,
    Issue,
    ParentLink,
    BlockerLink,
    Graph,
    Labels,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    Partial,
    Uncertain,
    Conflict,
}
impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Uncertain => "uncertain",
            Self::Conflict => "conflict",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    Ticket(TicketKey),
    Issue(IssueNumber),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub phase: Phase,
    pub subjects: Vec<Subject>,
}
impl Context {
    pub const fn new(phase: Phase) -> Self {
        Self {
            phase,
            subjects: Vec::new(),
        }
    }
    pub fn issue(phase: Phase, number: &IssueNumber) -> Self {
        Self {
            phase,
            subjects: vec![Subject::Issue(number.clone())],
        }
    }
    pub fn ticket(phase: Phase, batch: &Batch, index: usize) -> Self {
        let mut subjects = vec![Subject::Ticket(batch.tickets[index].key.clone())];
        if let TicketStatus::Created(mapping) = &batch.tickets[index].status {
            subjects.push(Subject::Issue(mapping.number.clone()));
        }
        Self { phase, subjects }
    }
    pub fn related(mut self, subject: Subject) -> Self {
        if !self.subjects.contains(&subject) {
            self.subjects.push(subject);
        }
        self
    }
    pub fn related_issue(mut self, batch: &Batch, number: &IssueNumber) -> Self {
        self = self.related(Subject::Issue(number.clone()));
        for ticket in &batch.tickets {
            if matches!(&ticket.status, TicketStatus::Created(mapping) if mapping.number == *number)
            {
                self = self.related(Subject::Ticket(ticket.key.clone()));
            }
        }
        self
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteOperation {
    pub context: Context,
    pub operation: Operation,
    pub sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    RemoteOperationStarted(RemoteOperation),
    StepVerified {
        context: Context,
        step: Step,
    },
    PublicationCompleted {
        outcome: Outcome,
    },
    PublicationStopped {
        last_operation: Option<RemoteOperation>,
        outcome: Outcome,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
}
/// The listener maps facts to records before it renders readable stderr lines.
#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub level: Level,
    pub event: Event,
}
impl From<Event> for Record {
    fn from(event: Event) -> Self {
        Self {
            level: if matches!(event, Event::PublicationStopped { .. }) {
                Level::Warning
            } else {
                Level::Info
            },
            event,
        }
    }
}
pub fn stderr_listener(mut writer: impl Write) -> impl FnMut(Event) {
    move |event| {
        let record = Record::from(event);
        // Each call writes immediately. A broken diagnostic stream changes no outcome.
        let guidance = if record.level == Level::Warning {
            " Read the final JSON result and follow the documented recovery instructions."
        } else {
            ""
        };
        let _ = writeln!(writer, "{:?}: {:?}{guidance}", record.level, record.event);
        let _ = writer.flush();
    }
}
type Listener = Box<dyn FnMut(Event)>;
pub struct Progress {
    listener: RefCell<Listener>,
    sequence: Cell<u64>,
    last: RefCell<Option<RemoteOperation>>,
}
impl Progress {
    pub fn new(listener: impl FnMut(Event) + 'static) -> Self {
        Self {
            listener: RefCell::new(Box::new(listener)),
            sequence: Cell::new(0),
            last: RefCell::new(None),
        }
    }
    pub fn silent() -> Self {
        Self::new(|_| {})
    }
    pub fn started(&self, context: &Context, operation: Operation) {
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        let remote = RemoteOperation {
            context: context.clone(),
            operation,
            sequence,
        };
        *self.last.borrow_mut() = Some(remote.clone());
        self.emit(Event::RemoteOperationStarted(remote));
    }
    pub fn verified(&self, context: Context, step: Step) {
        self.emit(Event::StepVerified { context, step });
    }
    pub fn finish(&self, outcome: Outcome) {
        self.emit(if outcome == Outcome::Complete {
            Event::PublicationCompleted { outcome }
        } else {
            Event::PublicationStopped {
                last_operation: self.last.borrow().clone(),
                outcome,
            }
        });
    }
    fn emit(&self, event: Event) {
        (self.listener.borrow_mut())(event);
    }
}
