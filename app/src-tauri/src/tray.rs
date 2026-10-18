//! System tray icon and its menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// The tray menu in the current language.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let fr = crate::settings::language(&app.state::<crate::state::AppState>().settings()) == "fr";
    let tr = |en: &'static str, fr_: &'static str| if fr { fr_ } else { en };
    let item = |id: &str, en: &'static str, fr_: &'static str| {
        MenuItem::with_id(app, id, tr(en, fr_), true, None::<&str>)
    };
    let undo = item("undo", "Undo last job", "Annuler la dernière tâche")?;
    let history = item("history", "History…", "Historique…")?;
    let components = item("components", "Components…", "Composants…")?;
    let settings = item("settings", "Settings…", "Réglages…")?;
    let clip = item(
        "clipboard",
        "Image from clipboard…",
        "Image du presse-papiers…",
    )?;
    let float = item("float", "Float shelf", "Étagère flottante")?;
    let dock = item("dock", "Drop dock", "Zone de dépôt")?;
    let about = item("about", "About Metamorphosis", "À propos de Metamorphosis")?;
    let quit = item("quit", "Quit", "Quitter")?;
    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &clip,
            &float,
            &dock,
            &sep()?,
            &undo,
            &history,
            &components,
            &settings,
            &sep()?,
            &about,
            &sep()?,
            &quit,
        ],
    )
}

/// Rebuild the menu after a language change.
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(m)) = (app.tray_by_id("main"), menu(app)) {
        let _ = tray.set_menu(Some(m));
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = menu(app)?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundled icon"))
        .tooltip("Metamorphosis")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "about" => crate::show_window(app, "about"),
            "undo" => match crate::commands::undo_last() {
                Ok(Some(what)) => log::info!("undone: {what}"),
                Ok(None) => log::info!("nothing to undo"),
                Err(e) => log::warn!("undo failed: {e}"),
            },
            "history" => {
                let _ = crate::commands::open_history(app.clone());
            }
            "clipboard" => {
                if let Err(e) = crate::actions::clipboard_image(app) {
                    log::warn!("clipboard image: {e:#}");
                }
            }
            "float" => {
                let _ = crate::actions::add_to_shelf(app, &[]);
            }
            "dock" => {
                if let Err(e) = crate::dock::show(app) {
                    log::warn!("dock: {e:#}");
                }
            }
            "settings" => {
                let _ = crate::settings::open_settings(app.clone());
            }
            "components" => {
                let _ = crate::commands::open_components(app.clone(), None);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_window(tray.app_handle(), "about");
            }
        })
        .build(app)?;
    Ok(())
}
