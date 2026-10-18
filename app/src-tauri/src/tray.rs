//! System tray icon and its menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let undo = MenuItem::with_id(app, "undo", "Undo last job", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History…", true, None::<&str>)?;
    let components = MenuItem::with_id(app, "components", "Components…", true, None::<&str>)?;
    let clip = MenuItem::with_id(
        app,
        "clipboard",
        "Image from clipboard…",
        true,
        None::<&str>,
    )?;
    let float = MenuItem::with_id(app, "float", "Float shelf", true, None::<&str>)?;
    let dock = MenuItem::with_id(app, "dock", "Drop dock", true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", "About Metamorphosis", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(
        app,
        &[
            &clip,
            &float,
            &dock,
            &sep()?,
            &undo,
            &history,
            &components,
            &sep()?,
            &about,
            &sep()?,
            &quit,
        ],
    )?;

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
