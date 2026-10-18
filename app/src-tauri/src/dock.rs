//! The drop dock: a small HUD target pinned to a screen edge that expands into the
//! ring when files are dragged onto it. The main entry point on Wayland, where the
//! cursor-following ring can't work; available everywhere from the tray.

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "dock";
const SMALL: f64 = 96.0;
const LARGE: f64 = 560.0;

pub fn show(app: &AppHandle) -> anyhow::Result<()> {
    if let Some(w) = app.get_webview_window(LABEL) {
        w.show()?;
        return Ok(());
    }
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("dock.html".into()))
        .title("Metamorphosis dock")
        .inner_size(SMALL, SMALL)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(target_os = "linux")]
    let anchored = {
        use gtk::glib::translate::ToGlibPtr;
        use gtk::prelude::*;
        w.gtk_window().ok().is_some_and(|gw| {
            let ptr: *mut gtk::ffi::GtkWindow = gw.upcast_ref::<gtk::Window>().to_glib_none().0;
            // SAFETY: the window is built but not shown yet, and this runs on the GTK thread.
            unsafe {
                morph_platform::wayland::anchor(
                    ptr.cast(),
                    morph_platform::wayland::Edge::Right,
                    12,
                )
            }
            .unwrap_or(false)
        })
    };
    #[cfg(not(target_os = "linux"))]
    let anchored = false;
    if !anchored {
        // Right edge, vertically centred (X11, Windows; Wayland without layer-shell places it itself).
        if let (Some((cx, cy)), Ok(scale)) = (morph_platform::cursor(), w.scale_factor()) {
            if let Some(a) = morph_platform::work_area_at(cx, cy) {
                let s = (SMALL * scale) as i32;
                let _ = w.set_position(tauri::PhysicalPosition::new(
                    a.x + a.w - s - 12,
                    a.y + (a.h - s) / 2,
                ));
            }
        }
    }
    w.show()?;
    Ok(())
}

/// Grow into the ring (keeping the right edge in place) or shrink back to the dock.
#[tauri::command]
pub fn dock_expand(app: AppHandle, expand: bool) -> Result<(), String> {
    let Some(w) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    let target = if expand { LARGE } else { SMALL };
    let scale = w.scale_factor().unwrap_or(1.0);
    if let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) {
        let delta = ((target * scale) as i32) - size.width as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(
            pos.x - delta,
            pos.y - delta / 2,
        ));
    }
    w.set_size(LogicalSize::new(target, target))
        .map_err(|e| e.to_string())
}

/// Wayland: bind the portal shortcut that summons the dock.
#[cfg(target_os = "linux")]
pub fn summon_shortcut(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let a = app.clone();
        let r = morph_platform::wayland::summon_shortcut("CTRL+ALT+M", move || {
            let a2 = a.clone();
            let _ = a.run_on_main_thread(move || {
                let _ = show(&a2);
            });
        })
        .await;
        if let Err(e) = r {
            log::info!("no global shortcut on this desktop: {e:#}");
        }
    });
}
