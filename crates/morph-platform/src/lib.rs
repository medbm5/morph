//! Platform integration for Metamorphosis: the overlay window, the file manager
//! (selection, reveal), system settings (reduced motion) and, in later slices,
//! clipboard, share and terminal.
//!
//! Every function has a Windows and a Linux implementation; anything a platform
//! can't do returns an error rather than panicking.

pub mod actions;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_clip;
pub mod shell_menu;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub mod win_clip;
#[cfg(windows)]
mod win_drop;
#[cfg(windows)]
pub mod win_share;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
use win as imp;

use std::path::{Path, PathBuf};

/// A screen rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// A drag over the overlay, in client pixels of the overlay window.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DropEvent {
    /// `paths` is empty when the drag carries no files.
    Enter {
        paths: Vec<PathBuf>,
        x: i32,
        y: i32,
    },
    Over {
        x: i32,
        y: i32,
    },
    Leave,
    Drop {
        paths: Vec<PathBuf>,
        x: i32,
        y: i32,
    },
}

/// Current cursor position (physical pixels).
pub fn cursor() -> Option<(i32, i32)> {
    imp::cursor()
}

/// Work area (screen minus taskbar/panels) of the monitor containing a point.
pub fn work_area_at(x: i32, y: i32) -> Option<Rect> {
    imp::work_area_at(x, y)
}

/// Place a `size`×`size` square centred on (x, y), kept inside the work area.
pub fn centred_square(x: i32, y: i32, size: i32) -> Rect {
    let mut r = Rect {
        x: x - size / 2,
        y: y - size / 2,
        w: size,
        h: size,
    };
    if let Some(a) = work_area_at(x, y) {
        r.x = r.x.clamp(a.x, (a.x + a.w - size).max(a.x));
        r.y = r.y.clamp(a.y, (a.y + a.h - size).max(a.y));
    }
    r
}

/// Does the OS ask for reduced motion? (Windows client-area animations off,
/// GNOME `enable-animations=false`, KDE `AnimationDurationFactor=0`.)
pub fn reduced_motion() -> bool {
    imp::reduced_motion()
}

/// Paths currently selected in the file-manager window at (x, y), if we can tell.
/// Used to build the ring before the drop target sees the drag.
pub fn selection_at(x: i32, y: i32) -> Vec<PathBuf> {
    imp::selection_at(x, y).unwrap_or_else(|e| {
        log::debug!("selection lookup failed: {e:#}");
        Vec::new()
    })
}

/// Move files or folders to the Recycle Bin / freedesktop Trash (Undo).
pub fn trash(paths: &[PathBuf]) -> anyhow::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    trash::delete_all(paths).map_err(|e| anyhow::anyhow!("moving to the trash: {e}"))
}

/// Selected items in the foreground file-manager window (Windows Explorer).
pub fn selection_foreground() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        win::selection_foreground().unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Open the containing folder with `path` selected.
pub fn reveal(path: &Path) -> anyhow::Result<()> {
    imp::reveal(path)
}

/// Overlay window helpers operating on a native window handle.
pub mod overlay {
    use super::Rect;

    /// Native handle of the overlay window (HWND on Windows, X11 window id on Linux).
    #[derive(Debug, Clone, Copy)]
    pub struct Handle(pub isize);

    /// One-time setup: never take focus, stay out of the taskbar and Alt-Tab, stay on top.
    pub fn prepare(h: Handle) -> anyhow::Result<()> {
        super::imp::overlay_prepare(h)
    }

    /// Move to `r` and show without activating (the drag source keeps focus).
    pub fn show_at(h: Handle, r: Rect) -> anyhow::Result<()> {
        super::imp::overlay_show(h, r)
    }

    pub fn hide(h: Handle) -> anyhow::Result<()> {
        super::imp::overlay_hide(h)
    }

    /// Make the overlay (and its child windows) an OLE drop target that only accepts
    /// files and always answers COPY. Windows only; call on the UI thread after the
    /// window has been shown once. Returns how many windows were registered.
    pub fn register_drop_target(
        h: Handle,
        cb: std::sync::Arc<dyn Fn(super::DropEvent) + Send + Sync>,
    ) -> usize {
        #[cfg(windows)]
        {
            super::win_drop::register(windows::Win32::Foundation::HWND(h.0 as *mut _), cb)
        }
        #[cfg(not(windows))]
        {
            let _ = (h, cb);
            0
        }
    }

    /// Clip input and drops to a circle of `diameter` px centred in the window.
    pub fn set_circle(h: Handle, size: i32, diameter: i32) -> anyhow::Result<()> {
        super::imp::overlay_circle(h, size, diameter)
    }
}

/// Can windows be translucent? Always on Windows; on X11 only with a compositing manager.
pub fn has_compositor() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::has_compositor()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}
