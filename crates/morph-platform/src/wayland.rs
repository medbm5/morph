//! Wayland mode helpers: layer-shell anchoring (KDE, wlroots) loaded at runtime, and
//! the GlobalShortcuts portal to summon the drop dock.
//!
//! Wayland doesn't let apps read global input, see the cursor outside their windows or
//! place windows freely, so Metamorphosis offers a drop dock instead of the
//! cursor-following ring.

use anyhow::{bail, Result};

/// Edges for [`anchor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Pin a GTK window to a screen edge on the overlay layer with gtk-layer-shell,
/// loaded with `dlopen` so the app runs (unpinned) where it isn't installed.
/// `gtk_window` is a `GtkWindow*`. Returns false when layer-shell isn't available.
///
/// # Safety
/// `gtk_window` must be a valid, not-yet-realized `GtkWindow*`, and this must run
/// on the GTK main thread.
pub unsafe fn anchor(gtk_window: *mut std::ffi::c_void, edge: Edge, margin: i32) -> Result<bool> {
    type IsSupported = unsafe extern "C" fn() -> i32;
    type Init = unsafe extern "C" fn(*mut std::ffi::c_void);
    type SetLayer = unsafe extern "C" fn(*mut std::ffi::c_void, i32);
    type SetAnchor = unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32);
    type SetMargin = unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32);
    type SetNamespace = unsafe extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_char);

    let lib = match libloading::Library::new("libgtk-layer-shell.so.0") {
        Ok(l) => l,
        Err(_) => return Ok(false),
    };
    let supported: libloading::Symbol<IsSupported> = lib.get(b"gtk_layer_is_supported\0")?;
    if supported() == 0 {
        return Ok(false);
    }
    let init: libloading::Symbol<Init> = lib.get(b"gtk_layer_init_for_window\0")?;
    let set_layer: libloading::Symbol<SetLayer> = lib.get(b"gtk_layer_set_layer\0")?;
    let set_anchor: libloading::Symbol<SetAnchor> = lib.get(b"gtk_layer_set_anchor\0")?;
    let set_margin: libloading::Symbol<SetMargin> = lib.get(b"gtk_layer_set_margin\0")?;
    let set_ns: libloading::Symbol<SetNamespace> = lib.get(b"gtk_layer_set_namespace\0")?;
    // GtkLayerShellEdge: LEFT=0, RIGHT=1, TOP=2, BOTTOM=3. Layer OVERLAY=3.
    let e = match edge {
        Edge::Left => 0,
        Edge::Right => 1,
        Edge::Top => 2,
        Edge::Bottom => 3,
    };
    init(gtk_window);
    set_layer(gtk_window, 3);
    set_anchor(gtk_window, e, 1);
    set_margin(gtk_window, e, margin);
    set_ns(gtk_window, c"metamorphosis-dock".as_ptr());
    // The library must stay loaded for the window's lifetime.
    std::mem::forget(lib);
    Ok(true)
}

/// Bind a global shortcut through the xdg-desktop-portal GlobalShortcuts interface
/// (KDE Plasma, GNOME 48+, …) and call `on_activate` each time it fires. Runs until
/// the session closes; spawn it on an async runtime.
pub async fn summon_shortcut(
    preferred: &str,
    on_activate: impl Fn() + Send + 'static,
) -> Result<()> {
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    use futures_util::StreamExt;
    let proxy = match GlobalShortcuts::new().await {
        Ok(p) => p,
        Err(e) => bail!("GlobalShortcuts portal unavailable: {e}"),
    };
    let session = proxy.create_session().await?;
    let shortcut = NewShortcut::new("summon-dock", "Show the Metamorphosis drop dock")
        .preferred_trigger(Some(preferred));
    proxy
        .bind_shortcuts(&session, &[shortcut], None)
        .await?
        .response()?;
    let mut activated = proxy.receive_activated().await?;
    while let Some(ev) = activated.next().await {
        if ev.shortcut_id() == "summon-dock" {
            on_activate();
        }
    }
    Ok(())
}
