use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::dispatcher::EventListener;
use super::event::EventEnvelope;

enum WorkerMessage {
    Event(Arc<EventEnvelope>),
    Flush(mpsc::SyncSender<()>),
    Shutdown,
}

pub struct StructuredTracingListener {
    sender: SyncSender<WorkerMessage>,
    dropped: Arc<AtomicU64>,
    failures: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
    worker_done: Mutex<mpsc::Receiver<()>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl StructuredTracingListener {
    pub fn new(capacity: usize) -> Self {
        Self::with_writer(capacity, Box::new(std::io::stdout()))
    }

    fn with_writer(capacity: usize, mut writer: Box<dyn Write + Send>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WorkerMessage>(capacity);
        let (worker_done_tx, worker_done) = mpsc::sync_channel(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let failures = Arc::new(AtomicU64::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let worker_failures = Arc::clone(&failures);
        let worker_healthy = Arc::clone(&healthy);
        let worker = std::thread::Builder::new()
            .name("flipped-structured-events".to_owned())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    match message {
                        WorkerMessage::Event(event) => {
                            let failed = serde_json::to_writer(&mut writer, event.as_ref())
                                .is_err()
                                || writer.write_all(b"\n").is_err();
                            if failed {
                                worker_failures.fetch_add(1, Ordering::Relaxed);
                                worker_healthy.store(false, Ordering::Relaxed);
                            }
                        }
                        WorkerMessage::Flush(done) => {
                            if writer.flush().is_err() {
                                worker_failures.fetch_add(1, Ordering::Relaxed);
                                worker_healthy.store(false, Ordering::Relaxed);
                            }
                            let _ = done.try_send(());
                        }
                        WorkerMessage::Shutdown => {
                            let _ = writer.flush();
                            break;
                        }
                    }
                }
                let _ = worker_done_tx.try_send(());
            })
            .expect("structured listener worker starts");
        Self {
            sender,
            dropped,
            failures,
            healthy,
            worker_done: Mutex::new(worker_done),
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn flush(&self, timeout: Duration) -> bool {
        self.flush_until(deadline_after(timeout))
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        let deadline = deadline_after(timeout);
        if !self.flush_until(deadline)
            || !send_until(&self.sender, WorkerMessage::Shutdown, deadline)
        {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero()
            || self
                .worker_done
                .lock()
                .expect("listener completion lock")
                .recv_timeout(remaining)
                .is_err()
        {
            return false;
        }
        let worker = self.worker.lock().expect("listener worker lock").take();
        // Completion is sent only after the worker loop exits, so this join cannot wait on I/O.
        worker.is_none_or(|worker| worker.join().is_ok())
    }

    fn flush_until(&self, deadline: Instant) -> bool {
        let (done, completed) = mpsc::sync_channel(1);
        if !send_until(&self.sender, WorkerMessage::Flush(done), deadline) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        !remaining.is_zero() && completed.recv_timeout(remaining).is_ok()
    }
}

fn deadline_after(timeout: Duration) -> Instant {
    Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now)
}

fn send_until(
    sender: &SyncSender<WorkerMessage>,
    mut message: WorkerMessage,
    deadline: Instant,
) -> bool {
    loop {
        match sender.try_send(message) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(returned)) => {
                message = returned;
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return false;
                }
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
        }
    }
}

impl EventListener for StructuredTracingListener {
    fn on_event(&self, event: Arc<EventEnvelope>) {
        match self.sender.try_send(WorkerMessage::Event(event)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.failures.fetch_add(1, Ordering::Relaxed);
                self.healthy.store(false, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Condvar;

    use crate::observability::{
        EventContext, EventDispatcher, Outcome, ServiceEvent, ServiceEventName, ServiceIdentity,
        Severity,
    };

    use super::*;

    struct GatedWriter {
        gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Write for GatedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let (lock, ready) = &*self.gate;
            let mut open = lock.lock().expect("gate lock");
            while !*open {
                open = ready.wait(open).expect("gate wait");
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn saturated_stalled_worker_cannot_overrun_flush_deadline() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let listener = Arc::new(StructuredTracingListener::with_writer(
            1,
            Box::new(GatedWriter {
                gate: Arc::clone(&gate),
            }),
        ));
        let dispatcher = EventDispatcher::new(
            ServiceIdentity {
                name: "test".to_owned(),
                version: "1".to_owned(),
                environment: "test".to_owned(),
                instance_id: "instance".to_owned(),
            },
            vec![listener.clone()],
        );
        for _ in 0..3 {
            dispatcher.emit(
                Severity::Info,
                EventContext::default(),
                ServiceEvent {
                    name: ServiceEventName::SessionCreated,
                    outcome: Outcome::Success,
                    error_code: None,
                    duration_ms: None,
                },
            );
        }

        let started = Instant::now();
        assert!(!listener.flush(Duration::from_millis(20)));
        assert!(started.elapsed() < Duration::from_millis(200));

        let (lock, ready) = &*gate;
        *lock.lock().expect("gate lock") = true;
        ready.notify_all();
        assert!(listener.shutdown(Duration::from_secs(1)));
    }
}
