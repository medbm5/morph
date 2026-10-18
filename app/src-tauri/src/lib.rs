//! Metamorphosis desktop shell: tray icon, windows and the HUD ring overlay.

mod actions;
mod commands;
mod dock;
mod expanded;
mod integration;
#[cfg(target_os = "linux")]
mod linux_shape;
mod logger;
mod overlay;
mod settings;
mod state;
mod tray;

use state::AppState;
use tauri::{Manager, RunEvent, WindowEvent};

pub fn run() {
    logger::init();
    log::info!("Metamorphosis {} starting", morph_core::VERSION);
    let app = tauri::Builder::default()
        // A second launch (file-manager menu, `--ring`, `--open`) hands its arguments
        // to the running app instead of starting another one.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_args(app, &argv, false);
        }))
        .plugin(integration::shortcut_plugin())
        .manage(AppState::new(morph_core::Registry::new()))
        .manage(actions::Shelf::default())
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
            commands::image_metadata,
            commands::image_size,
            commands::trace,
            commands::compose_preview,
            commands::detect_regions,
            commands::components_status,
            commands::install_component,
            commands::open_components,
            commands::undo_last,
            commands::history,
            commands::save_recipe,
            commands::open_history,
            commands::show_error,
            commands::video_info,
            commands::video_frame,
            commands::video_strip,
            commands::audio_peaks,
            commands::audio_loudness,
            commands::preview_op,
            commands::pdf_thumbs,
            actions::float_items,
            actions::float_add,
            actions::float_remove,
            actions::float_drag_out,
            expanded::expanded_request,
            dock::dock_expand,
            settings::get_settings,
            settings::set_settings,
            settings::get_look,
            settings::layout_ops,
            settings::layout_key_for,
            settings::portable_status,
            settings::set_portable,
            settings::open_settings,
        ])
        .setup(|app| {
            // The registry needs the app handle (actions use its windows).
            commands::reload_registry_for(app.handle());
            tray::create(app.handle())?;
            overlay::create(app.handle())?;
            overlay::start_watcher(app.handle());
            integration::install_once(app.handle());
            settings::apply(app.handle(), &app.state::<AppState>().settings(), false);
            let args: Vec<String> = std::env::args().collect();
            handle_args(app.handle(), &args, true);
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

/// Act on command-line arguments, at startup or forwarded from a second launch:
/// `--open <op> <files…>`, `--ring <ring> <files…>`, `--background`.
fn handle_args(app: &tauri::AppHandle, args: &[String], startup: bool) {
    if args.iter().any(|a| a == "--uninstall-menus") {
        // Used by the uninstaller and by users who prefer no context-menu entries.
        if let Err(e) = morph_platform::shell_menu::uninstall() {
            log::warn!("removing menus: {e:#}");
        }
        let _ =
            std::fs::remove_file(morph_core::components::config_dir().join("integration.version"));
        app.exit(0);
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--open") {
        if let Some(op) = args.get(i + 1) {
            let files: Vec<std::path::PathBuf> = args[i + 2..].iter().map(Into::into).collect();
            if let Err(e) = expanded::open(app, op, &files) {
                log::error!("--open {op}: {e:#}");
            }
        }
    } else if let Some(i) = args.iter().position(|a| a == "--ring") {
        let ring = args.get(i + 1).cloned().unwrap_or_else(|| "convert".into());
        let files: Vec<std::path::PathBuf> = args[i + 2..].iter().map(Into::into).collect();
        let req = expanded::Request {
            mode: "ring".into(),
            ring: Some(ring),
            paths: files,
            op: None,
            text: None,
        };
        if let Err(e) = expanded::show(app, req) {
            log::error!("--ring: {e:#}");
        }
    } else if !(startup && args.iter().any(|a| a == "--background")) {
        show_window(app, "about");
    }
}

/// Show a window by label and bring it to the front.
pub(crate) fn show_window(app: &tauri::AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
