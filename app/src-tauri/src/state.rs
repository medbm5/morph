//! Shared app state.

use morph_core::{Registry, Settings};
use morph_input_watch::{RingKind, Watch};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

pub struct AppState {
    pub registry: RwLock<Arc<Registry>>,
    pub watch: Mutex<Option<Watch>>,
    pub ring: Mutex<RingSession>,
    /// Logical size of the ring overlay (the 600 px reference scaled to the screen).
    pub ring_size: Mutex<f64>,
    /// Cancel flags of running jobs.
    pub jobs: Mutex<std::collections::HashMap<u64, Arc<std::sync::atomic::AtomicBool>>>,
    pub settings: RwLock<Settings>,
}

/// The ring currently on screen.
#[derive(Default)]
pub struct RingSession {
    pub open: Option<RingKind>,
    pub opened_at: Option<Instant>,
    /// Explorer selection captured at drag start (Windows), used until the drop
    /// target tells us what's really being dragged.
    pub prefetched: Vec<PathBuf>,
    pub last_latency_ms: Option<f64>,
    /// Where the overlay sits on screen (physical pixels).
    pub rect: Option<morph_platform::Rect>,
}

impl AppState {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry: RwLock::new(Arc::new(registry)),
            watch: Mutex::new(None),
            ring: Mutex::new(RingSession::default()),
            ring_size: Mutex::new(560.0),
            jobs: Mutex::new(Default::default()),
            settings: RwLock::new(Settings::load()),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.registry.read().unwrap().clone()
    }
}
