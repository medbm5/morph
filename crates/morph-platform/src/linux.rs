//! Linux implementation (X11; Wayland pieces arrive in S17).

use crate::overlay::Handle;
use crate::Rect;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt as _, PropMode, StackMode,
};
use x11rb::wrapper::ConnectionExt as _;

fn x11() -> Option<(x11rb::rust_connection::RustConnection, usize)> {
    std::env::var_os("DISPLAY")?;
    x11rb::connect(None).ok()
}

pub fn cursor() -> Option<(i32, i32)> {
    let (conn, n) = x11()?;
    let root = conn.setup().roots[n].root;
    let r = conn.query_pointer(root).ok()?.reply().ok()?;
    Some((r.root_x as i32, r.root_y as i32))
}

pub fn work_area_at(_x: i32, _y: i32) -> Option<Rect> {
    // _NET_WORKAREA (first desktop) is a good-enough clamp across window managers.
    let (conn, n) = x11()?;
    let root = conn.setup().roots[n].root;
    let atom = conn
        .intern_atom(false, b"_NET_WORKAREA")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let prop = conn
        .get_property(false, root, atom, AtomEnum::CARDINAL, 0, 4)
        .ok()?
        .reply()
        .ok()?;
    let v: Vec<u32> = prop.value32()?.collect();
    if v.len() < 4 {
        let s = &conn.setup().roots[n];
        return Some(Rect {
            x: 0,
            y: 0,
            w: s.width_in_pixels as i32,
            h: s.height_in_pixels as i32,
        });
    }
    Some(Rect {
        x: v[0] as i32,
        y: v[1] as i32,
        w: v[2] as i32,
        h: v[3] as i32,
    })
}

pub fn reduced_motion() -> bool {
    // GNOME and derivatives.
    if let Ok(o) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
    {
        if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "false" {
            return true;
        }
    }
    // KDE Plasma: AnimationDurationFactor = 0 disables animations.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let kdeglobals = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(home.join(".config"))
        .join("kdeglobals");
    if let Ok(s) = std::fs::read_to_string(kdeglobals) {
        for line in s.lines() {
            if let Some(v) = line.trim().strip_prefix("AnimationDurationFactor=") {
                if v.trim().parse::<f64>().is_ok_and(|f| f == 0.0) {
                    return true;
                }
            }
        }
    }
    false
}

/// File managers on Linux don't expose their selection to other processes.
pub fn selection_at(_x: i32, _y: i32) -> Result<Vec<PathBuf>> {
    Ok(vec![])
}

pub fn reveal(path: &Path) -> Result<()> {
    let uri = file_uri(path);
    // The FileManager1 D-Bus interface selects the file in Nautilus, Dolphin, Nemo, Thunar…
    let ok = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.FileManager1",
            "--object-path",
            "/org/freedesktop/FileManager1",
            "--method",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("['{uri}']"),
            "",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        return Ok(());
    }
    let dir = path.parent().context("no parent folder")?;
    let status = Command::new("xdg-open")
        .arg(dir)
        .status()
        .context("running xdg-open")?;
    if !status.success() {
        bail!("xdg-open failed");
    }
    Ok(())
}

/// `file://` URI with percent-encoding for anything outside the unreserved set.
pub fn file_uri(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut s = String::from("file://");
    for b in abs.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                s.push(b as char)
            }
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    s
}

/// The window manager handles focus/taskbar hints for GTK windows; we only add
/// `_NET_WM_STATE_ABOVE` + skip-taskbar/pager in case the WM ignored GTK's hints.
pub fn overlay_prepare(h: Handle) -> Result<()> {
    let Some((conn, _)) = x11() else {
        return Ok(());
    };
    let win = h.0 as u32;
    let state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let atoms: Vec<u32> = [
        &b"_NET_WM_STATE_ABOVE"[..],
        b"_NET_WM_STATE_SKIP_TASKBAR",
        b"_NET_WM_STATE_SKIP_PAGER",
    ]
    .iter()
    .filter_map(|n| {
        conn.intern_atom(false, n)
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    })
    .collect();
    conn.change_property32(PropMode::REPLACE, win, state, AtomEnum::ATOM, &atoms)?;
    conn.flush()?;
    Ok(())
}

pub fn overlay_show(h: Handle, r: Rect) -> Result<()> {
    let Some((conn, _)) = x11() else {
        bail!("no X display")
    };
    let win = h.0 as u32;
    conn.map_window(win)?;
    conn.configure_window(
        win,
        &ConfigureWindowAux::new()
            .x(r.x)
            .y(r.y)
            .width(r.w as u32)
            .height(r.h as u32)
            .stack_mode(StackMode::ABOVE),
    )?;
    conn.flush()?;
    Ok(())
}

pub fn overlay_hide(h: Handle) -> Result<()> {
    let Some((conn, _)) = x11() else {
        return Ok(());
    };
    conn.unmap_window(h.0 as u32)?;
    conn.flush()?;
    Ok(())
}

/// Circular shaping is done through GTK in the app (it owns the GdkWindow);
/// this is a no-op here.
pub fn overlay_circle(_h: Handle, _size: i32, _diameter: i32) -> Result<()> {
    Ok(())
}

/// Is a compositing manager running? Without one, ARGB windows aren't translucent.
pub fn has_compositor() -> bool {
    let Some((conn, n)) = x11() else { return false };
    let name = format!("_NET_WM_CM_S{n}");
    let Ok(Ok(atom)) = conn.intern_atom(false, name.as_bytes()).map(|c| c.reply()) else {
        return false;
    };
    conn.get_selection_owner(atom.atom)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some_and(|r| r.owner != x11rb::NONE)
}
