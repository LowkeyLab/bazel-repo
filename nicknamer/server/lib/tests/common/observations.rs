use nicknamer_server::observations::{Observation, ObservationSink, SharedObserver};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct RecordingObserver(Mutex<Vec<Observation>>);

impl RecordingObserver {
    pub fn shared() -> (Arc<Self>, SharedObserver) {
        let recorder = Arc::new(Self::default());
        (recorder.clone(), recorder)
    }

    pub fn events(&self) -> Vec<Observation> {
        self.0.lock().unwrap().clone()
    }
}

impl ObservationSink for RecordingObserver {
    fn record(&self, event: &Observation) {
        self.0.lock().unwrap().push(event.clone());
    }
}
