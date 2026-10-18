//! Drag + trigger-key detection for the Metamorphosis ring.
//!
//! - **Windows:** `WH_MOUSE_LL` / `WH_KEYBOARD_LL` hooks on a dedicated thread.
//! - **X11:** XInput2 raw events on the root window (no grabs), with an
//!   `XdndSelection` owner check so only real drag-and-drop opens the ring.
//! - **Wayland:** global input can't be observed; [`start`] returns
//!   [`Error::Unsupported`] and the app uses the drop dock instead.
//!
//! The platform-independent logic lives in [`machine`], which is unit-tested.

pub mod machine;

#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

pub use machine::{Action, Key, Machine, Raw, RingKind, Triggers};

use std::sync::{Arc, Mutex};

/// Receives actions from the watcher thread. Must return quickly (hooks have a
/// time budget): forward to a channel.
pub type Sink = Arc<dyn Fn(Action) + Send + Sync>;

#[derive(Debug)]
pub enum Error {
    /// This session can't observe global input (Wayland).
    Unsupported(&'static str),
    Other(anyhow::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported(why) => write!(f, "drag detection unavailable: {why}"),
            Error::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for Error {}

/// A running watcher. Dropping it stops the watcher.
pub struct Watch {
    machine: Arc<Mutex<Machine>>,
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl Watch {
    /// Close the ring but keep tracking the drag (e.g. the drag carried no files).
    pub fn close_ring(&self) {
        self.machine.lock().unwrap().close();
    }

    /// Forget the current drag (the overlay handled the drop).
    pub fn reset(&self) {
        self.machine.lock().unwrap().reset();
    }

    /// Change trigger keys at runtime.
    pub fn set_triggers(&self, t: Triggers) {
        self.machine.lock().unwrap().triggers = t;
    }

    pub fn is_dragging(&self) -> bool {
        self.machine.lock().unwrap().is_dragging()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

/// What kind of session we're in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    Windows,
    X11,
    Wayland,
    Other,
}

pub fn session() -> Session {
    if cfg!(windows) {
        return Session::Windows;
    }
    if cfg!(target_os = "linux") {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland");
        // Running under XWayland still can't see other clients' input reliably.
        if wayland {
            return Session::Wayland;
        }
        if std::env::var_os("DISPLAY").is_some() {
            return Session::X11;
        }
    }
    Session::Other
}

/// Start watching for drags and trigger taps.
pub fn start(triggers: Triggers, sink: Sink) -> Result<Watch, Error> {
    let machine = Arc::new(Mutex::new(Machine::new(triggers)));
    match session() {
        #[cfg(windows)]
        Session::Windows => {
            let stop = windows::start(machine.clone(), sink).map_err(Error::Other)?;
            Ok(Watch {
                machine,
                stop: Some(stop),
            })
        }
        #[cfg(target_os = "linux")]
        Session::X11 => {
            let stop = x11::start(machine.clone(), sink).map_err(Error::Other)?;
            Ok(Watch {
                machine,
                stop: Some(stop),
            })
        }
        Session::Wayland => Err(Error::Unsupported(
            "Wayland doesn't let apps observe global input; use the drop dock or an X11 session",
        )),
        _ => {
            let _ = sink;
            Err(Error::Unsupported("unsupported platform"))
        }
    }
}
