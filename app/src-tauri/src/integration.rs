//! Keyboard and file-manager entry points: the global hotkey on the Explorer selection,
//! and the file-manager menu entries (installed once per version, per user).

use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Ctrl+Alt+M: open the ring on whatever is selected in the active Explorer window.
pub fn hotkey() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyM)
}

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed || *shortcut != hotkey() {
                return;
            }
            let paths = morph_platform::selection_foreground();
            if paths.is_empty() {
                // Nothing selected (or not Explorer): the Float shelf is a drop target.
                let _ = crate::actions::add_to_shelf(app, &[]);
                return;
            }
            let req = crate::expanded::Request {
                mode: "ring".into(),
                ring: Some("convert".into()),
                paths,
                op: None,
                text: None,
            };
            if let Err(e) = crate::expanded::show(app, req) {
                log::warn!("hotkey: {e:#}");
            }
        })
        .build()
}

/// Register the hotkey and install file-manager menus once per app version.
pub fn install_once(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().register(hotkey()) {
        log::warn!("couldn't register Ctrl+Alt+M: {e}");
    }
    let marker = morph_core::components::config_dir().join("integration.version");
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(morph_core::VERSION) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    match morph_platform::shell_menu::install(&exe) {
        Ok(done) => {
            log::info!("file-manager menus installed: {}", done.join(", "));
            let _ = std::fs::create_dir_all(marker.parent().unwrap());
            let _ = std::fs::write(&marker, morph_core::VERSION);
        }
        Err(e) => log::warn!("file-manager menus: {e:#}"),
    }
}
