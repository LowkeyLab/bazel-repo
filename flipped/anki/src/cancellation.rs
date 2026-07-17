use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{ImportError, ImportErrorCode, Result};

/// Cooperative cancellation shared by upload, archive, and SQLite import work.
#[derive(Debug, Clone, Default)]
pub struct ImportCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ImportCancellation {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ImportError::new(ImportErrorCode::Cancelled))
        } else {
            Ok(())
        }
    }
}
