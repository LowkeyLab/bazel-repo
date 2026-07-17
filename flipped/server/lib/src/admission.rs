use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{Result, ServerError};

#[derive(Clone)]
pub struct AdmissionController {
    imports: Arc<Semaphore>,
    watches: Arc<Semaphore>,
    max_watches_per_session: usize,
}

pub struct ImportPermit {
    _permit: OwnedSemaphorePermit,
}

pub(crate) struct WatchPermit {
    _permit: OwnedSemaphorePermit,
}

impl AdmissionController {
    pub fn new(
        max_concurrent_imports: usize,
        max_global_watches: usize,
        max_watches_per_session: usize,
    ) -> Self {
        Self {
            imports: Arc::new(Semaphore::new(max_concurrent_imports)),
            watches: Arc::new(Semaphore::new(max_global_watches)),
            max_watches_per_session,
        }
    }

    pub fn try_reserve_import(&self) -> Result<ImportPermit> {
        Arc::clone(&self.imports)
            .try_acquire_owned()
            .map(|permit| ImportPermit { _permit: permit })
            .map_err(|_| ServerError::ResourceExhausted)
    }

    pub(crate) fn try_reserve_watch(&self, current_session_watches: usize) -> Option<WatchPermit> {
        if current_session_watches >= self.max_watches_per_session {
            return None;
        }
        Arc::clone(&self.watches)
            .try_acquire_owned()
            .ok()
            .map(|permit| WatchPermit { _permit: permit })
    }

    #[cfg(test)]
    fn available_imports(&self) -> usize {
        self.imports.available_permits()
    }

    #[cfg(test)]
    fn available_watches(&self) -> usize {
        self.watches.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_permits_saturate_and_release_on_drop() {
        let admission = AdmissionController::new(1, 1, 1);
        let permit = admission
            .try_reserve_import()
            .expect("first import admitted");
        assert_eq!(admission.available_imports(), 0);
        assert!(matches!(
            admission.try_reserve_import(),
            Err(ServerError::ResourceExhausted)
        ));
        drop(permit);
        assert_eq!(admission.available_imports(), 1);
        assert!(admission.try_reserve_import().is_ok());
    }

    #[test]
    fn watch_permits_apply_global_and_session_bounds_and_release() {
        let admission = AdmissionController::new(1, 1, 1);
        assert!(admission.try_reserve_watch(1).is_none());
        let permit = admission
            .try_reserve_watch(0)
            .expect("first global watch admitted");
        assert_eq!(admission.available_watches(), 0);
        assert!(admission.try_reserve_watch(0).is_none());
        drop(permit);
        assert_eq!(admission.available_watches(), 1);
        assert!(admission.try_reserve_watch(0).is_some());
    }
}
