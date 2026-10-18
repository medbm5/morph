//! The HUD ring overlay: a transparent, topmost, never-focused window that is
//! created hidden at startup and reused, so the first ring frame appears within
//! the 50 ms budget.

use crate::state::AppState;
use morph_input_watch::{Action, RingKind};
use serde::Serialize;
use std::sync::mpsc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const LABEL: &str = "ring";

pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let size = *app.state::<AppState>().ring_size.lock().unwrap();
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("ring.html".into()))
        .title("Metamorphosis ring")
        .inner_size(size, size)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        let h = morph_platform::overlay::Handle(hwnd.0 as isize);
        let _ = morph_platform::overlay::prepare(h);
    }
    Ok(w)
}

/// Payload of `ring://open`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OpenPayload {
    ring: RingKind,
    /// Paths known before the drop target saw the drag (Explorer selection), may be empty.
    paths: Vec<String>,
    reduced: bool,
    compositor: bool,
}

/// Show the overlay centred on (x, y) physical pixels.
fn show(app: &AppHandle, x: i32, y: i32) {
    let Some(w) = app.get_webview_window(LABEL) else {
        return;
    };
    let scale = w.scale_factor().unwrap_or(1.0);
    let size_logical = *app.state::<AppState>().ring_size.lock().unwrap();
    // Keep the ring within ~75 % of the work-area height on small screens.
    let mut size = (size_logical * scale).round() as i32;
    if let Some(area) = morph_platform::work_area_at(x, y) {
        size = size.min((area.h as f64 * 0.75) as i32);
    }
    let rect = morph_platform::centred_square(x, y, size);

    #[cfg(windows)]
    {
        if let Ok(hwnd) = w.hwnd() {
            let h = morph_platform::overlay::Handle(hwnd.0 as isize);
            let _ = morph_platform::overlay::show_at(h, rect);
            // Clip input and drops to the ring's disc (r 292 of the 600 reference).
            let _ = morph_platform::overlay::set_circle(
                h,
                rect.w,
                (rect.w as f64 * 588.0 / 600.0) as i32,
            );
            // (Re)install our drop target: WebView2 creates its child windows lazily,
            // after the webview's own handler was set up on a hidden window.
            let emitter = app.clone();
            let _ = app.run_on_main_thread(move || {
                let cb = std::sync::Arc::new(move |e: morph_platform::DropEvent| {
                    let _ = emitter.emit_to(LABEL, "ring://dnd", e);
                });
                let n = morph_platform::overlay::register_drop_target(h, cb);
                log::debug!("drop target registered on {n} window(s)");
            });
            return;
        }
    }
    let _ = w.set_size(tauri::PhysicalSize::new(rect.w as u32, rect.h as u32));
    let _ = w.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
    let _ = w.show();
    #[cfg(target_os = "linux")]
    crate::linux_shape::apply(&w, rect.w);
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        #[cfg(windows)]
        if let Ok(hwnd) = w.hwnd() {
            let _ = morph_platform::overlay::hide(morph_platform::overlay::Handle(hwnd.0 as isize));
            return;
        }
        let _ = w.hide();
    }
}

/// Start the input watcher and the thread that turns its actions into overlay events.
pub fn start_watcher(app: &AppHandle) {
    let (tx, rx) = mpsc::channel::<Action>();
    let sink: morph_input_watch::Sink = std::sync::Arc::new(move |a| {
        let _ = tx.send(a);
    });
    match morph_input_watch::start(Default::default(), sink) {
        Ok(watch) => {
            log::info!("input watcher started");
            *app.state::<AppState>().watch.lock().unwrap() = Some(watch);
        }
        Err(e) => {
            log::warn!("{e}");
            if morph_platform::is_wayland() {
                wayland_mode(app);
            }
            return;
        }
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("morph-ring".into())
        .spawn(move || {
            for action in rx {
                handle(&app, action);
            }
        })
        .expect("spawning ring thread");
}

/// No global input on Wayland: show the drop dock, bind the portal shortcut, and
/// explain the difference once.
fn wayland_mode(app: &AppHandle) {
    log::info!("Wayland session: using the drop dock");
    if let Err(e) = crate::dock::show(app) {
        log::warn!("dock: {e:#}");
    }
    #[cfg(target_os = "linux")]
    crate::dock::summon_shortcut(app);
    let marker = morph_core::components::config_dir().join("wayland.seen");
    if !marker.exists() {
        let _ = std::fs::create_dir_all(marker.parent().unwrap());
        let _ = std::fs::write(&marker, morph_core::VERSION);
        let req = crate::expanded::Request {
            mode: "wayland".into(),
            ring: None,
            paths: Vec::new(),
            op: None,
        };
        if let Err(e) = crate::expanded::show(app, req) {
            log::warn!("wayland page: {e:#}");
        }
    }
}

fn handle(app: &AppHandle, action: Action) {
    if !matches!(action, Action::Moved { .. }) {
        log::debug!("ring action {action:?}");
    }
    let state = app.state::<AppState>();
    match action {
        Action::DragStarted { x, y } => {
            // Prefetch the Explorer selection so segments are ready on the first frame.
            state.ring.lock().unwrap().prefetched.clear();
            let app = app.clone();
            std::thread::spawn(move || {
                let paths = morph_platform::selection_at(x, y);
                app.state::<AppState>().ring.lock().unwrap().prefetched = paths;
            });
        }
        Action::Open { ring, x, y } => {
            let paths = {
                let mut s = state.ring.lock().unwrap();
                s.open = Some(ring);
                s.opened_at = Some(Instant::now());
                s.prefetched
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect()
            };
            let payload = OpenPayload {
                ring,
                paths,
                reduced: morph_platform::reduced_motion(),
                compositor: morph_platform::has_compositor(),
            };
            let _ = app.emit_to(LABEL, "ring://open", payload);
            show(app, x, y);
        }
        Action::Switch { ring } => {
            state.ring.lock().unwrap().open = Some(ring);
            let _ = app.emit_to(LABEL, "ring://switch", ring);
        }
        Action::Pick { .. } | Action::Next | Action::Prev | Action::Apply | Action::Cancel => {
            let _ = app.emit_to(LABEL, "ring://key", action);
        }
        Action::Released { .. } => {
            let _ = app.emit_to(LABEL, "ring://released", ());
        }
        Action::Moved { .. } | Action::DragEnded => {}
    }
}
