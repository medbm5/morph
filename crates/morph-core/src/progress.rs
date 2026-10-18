//! Progress reporting and cancellation for a running operation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

type Callback = Arc<dyn Fn(f32, Option<&str>) + Send + Sync>;

/// Handed to an operation; reports the fraction done (0–1) of the current item.
#[derive(Clone)]
pub struct Progress {
    callback: Option<Callback>,
    cancel: Arc<AtomicBool>,
}

impl Default for Progress {
    fn default() -> Self {
        Self::silent()
    }
}

impl Progress {
    pub fn silent() -> Self {
        Self { callback: None, cancel: Arc::new(AtomicBool::new(false)) }
    }

    pub fn new(cancel: Arc<AtomicBool>, f: impl Fn(f32, Option<&str>) + Send + Sync + 'static) -> Self {
        Self { callback: Some(Arc::new(f)), cancel }
    }

    pub fn set(&self, fraction: f32) {
        if let Some(cb) = &self.callback {
            cb(fraction.clamp(0.0, 1.0), None);
        }
    }

    pub fn message(&self, fraction: f32, msg: &str) {
        if let Some(cb) = &self.callback {
            cb(fraction.clamp(0.0, 1.0), Some(msg));
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)` once the user has cancelled; call between expensive steps.
    pub fn check(&self) -> Result<(), crate::Error> {
        if self.is_cancelled() {
            Err(crate::Error::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
}
