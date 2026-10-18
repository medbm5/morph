//! Metamorphosis desktop shell: tray icon, windows and the HUD ring overlay.

mod commands;
mod expanded;
#[cfg(target_os = "linux")]
mod linux_shape;
mod logger;
mod overlay;
mod state;
mod tray;

use state::AppState;
use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    logger::init();
    log::info!("Metamorphosis {} starting", morph_core::VERSION);
    let registry = morph_engines::registry();
    let app = tauri::Builder::default()
        .manage(AppState::new(registry))
        .manage(expanded::Pending::default())
        .invoke_handler(tauri::generate_handler![
            commands::ring_items,
            commands::run_op,
            commands::cancel_job,
            commands::hide_ring,
            commands::ring_shown,
            commands::reveal,
            commands::open_editor,
            commands::op_spec,
            commands::describe,
            commands::trace,
            expanded::expanded_request,
        ])
        .setup(|app| {
            tray::create(app.handle())?;
            overlay::create(app.handle())?;
            overlay::start_watcher(app.handle());
            // Autostart launches with --background: stay in the tray.
            if !std::env::args().any(|a| a == "--background") {
                show_window(app.handle(), "about");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing a window only hides it; the app lives in the tray until Quit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Metamorphosis");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // Last window closed: keep running in the background.
            api.prevent_exit();
            let _ = handle.get_webview_window("about").map(|w| w.hide());
        }
    });
}

/// Show a window by label and bring it to the front.
pub(crate) fn show_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
