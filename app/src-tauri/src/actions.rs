//! Actions ring glue: Share, Float shelf, Move to…, clipboard image as a source.

use anyhow::{Context, Result};
use morph_platform::actions::Hooks;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const FLOAT: &str = "float";

/// Items on the Float shelf (references, not copies).
#[derive(Default)]
pub struct Shelf(pub Mutex<Vec<PathBuf>>);

/// Hooks for actions that need the app's windows.
pub fn hooks(app: &AppHandle) -> Hooks {
    let (a, b, c) = (app.clone(), app.clone(), app.clone());
    Hooks {
        share: Some(Arc::new(move |paths| share(&a, paths))),
        float: Some(Arc::new(move |paths| add_to_shelf(&b, paths))),
        move_to: Some(Arc::new(move |paths| move_to(&c, paths))),
    }
}

/// Create (hidden) or reveal the Float shelf, pinned to the right edge.
fn shelf_window(app: &AppHandle) -> Result<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(FLOAT) {
        return Ok(w);
    }
    let (w, h) = (260.0, 420.0);
    let win = WebviewWindowBuilder::new(app, FLOAT, WebviewUrl::App("float.html".into()))
        .title("Float")
        .inner_size(w, h)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()?;
    if let (Some((cx, cy)), Ok(scale)) = (morph_platform::cursor(), win.scale_factor()) {
        if let Some(area) = morph_platform::work_area_at(cx, cy) {
            let x = area.x + area.w - (w * scale) as i32 - 8;
            let y = area.y + (area.h - (h * scale) as i32) / 2;
            let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
        }
    }
    Ok(win)
}

pub fn add_to_shelf(app: &AppHandle, paths: &[PathBuf]) -> Result<()> {
    {
        let shelf = app.state::<Shelf>();
        let mut items = shelf.0.lock().unwrap();
        for p in paths {
            if !items.contains(p) {
                items.push(p.clone());
            }
        }
    }
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        if let Ok(w) = shelf_window(&app2) {
            let _ = w.show();
            let _ = w.emit("float://changed", ());
        }
    })?;
    Ok(())
}

/// Share through the system share sheet (Windows) — anchored to the Float shelf so
/// the sheet has a visible owner — or KDE Connect / LocalSend on Linux.
fn share(app: &AppHandle, paths: &[PathBuf]) -> Result<()> {
    #[cfg(windows)]
    {
        add_to_shelf(app, paths)?;
        let (tx, rx) = std::sync::mpsc::channel();
        let app2 = app.clone();
        let paths = paths.to_vec();
        app.run_on_main_thread(move || {
            let r = shelf_window(&app2)
                .and_then(|w| Ok(w.hwnd()?.0 as isize))
                .and_then(|h| morph_platform::win_share::share(h, &paths));
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        })?;
        rx.recv().context("share")?.map_err(|e| anyhow::anyhow!(e))
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        morph_platform::actions::share_linux(paths)
    }
}

/// Ask for a folder, then move each item there (never overwriting).
fn move_to(app: &AppHandle, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let _ = app;
    let start = paths
        .first()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let Some(dest) = rfd::FileDialog::new()
        .set_title("Move to…")
        .set_directory(&start)
        .pick_folder()
    else {
        anyhow::bail!(morph_core::Error::Cancelled);
    };
    let naming = morph_core::Settings::load().naming;
    let mut out = Vec::new();
    for p in paths {
        let name = p
            .file_name()
            .context("no file name")?
            .to_string_lossy()
            .into_owned();
        let target = (1..10_000)
            .map(|n| dest.join(naming.numbered(&name, n)))
            .find(|c| c.symlink_metadata().is_err())
            .context("no free name")?;
        if std::fs::rename(p, &target).is_err() {
            // Different drive: copy, then remove the original only after the copy succeeded.
            copy_all(p, &target)?;
            if p.is_dir() {
                std::fs::remove_dir_all(p)?;
            } else {
                std::fs::remove_file(p)?;
            }
        }
        out.push(target);
    }
    Ok(out)
}

fn copy_all(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)?.flatten() {
            copy_all(&e.path(), &to.join(e.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem {
    path: PathBuf,
    name: String,
    is_dir: bool,
    image: bool,
}

#[tauri::command]
pub fn float_items(shelf: tauri::State<Shelf>) -> Vec<ShelfItem> {
    let mut items = shelf.0.lock().unwrap();
    items.retain(|p| p.exists());
    items
        .iter()
        .map(|p| ShelfItem {
            path: p.clone(),
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            is_dir: p.is_dir(),
            image: morph_core::detect(p)
                .map(|k| k.family == morph_core::Family::Image)
                .unwrap_or(false),
        })
        .collect()
}

#[tauri::command]
pub fn float_add(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    add_to_shelf(&app, &paths).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn float_remove(shelf: tauri::State<Shelf>, paths: Vec<PathBuf>) {
    shelf.0.lock().unwrap().retain(|p| !paths.contains(p));
}

/// Start a native drag of shelf items out to Explorer / a file manager / any app.
#[tauri::command]
pub fn float_drag_out(app: AppHandle, paths: Vec<PathBuf>) -> Result<(), String> {
    let icon = include_bytes!("../icons/32x32.png").to_vec();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let Some(w) = app2.get_webview_window(FLOAT) else {
            return;
        };
        let item = drag::DragItem::Files(paths);
        let image = drag::Image::Raw(icon);
        let done =
            |r: drag::DragResult, _p: drag::CursorPosition| log::debug!("float drag-out: {r:?}");
        #[cfg(target_os = "linux")]
        let r = w.gtk_window().map_err(|e| e.to_string()).and_then(|g| {
            drag::start_drag(&g, item, image, done, Default::default()).map_err(|e| e.to_string())
        });
        #[cfg(not(target_os = "linux"))]
        let r =
            drag::start_drag(&w, item, image, done, Default::default()).map_err(|e| e.to_string());
        if let Err(e) = r {
            log::warn!("drag out failed: {e}");
        }
    })
    .map_err(|e| e.to_string())
}

/// Save the clipboard image as a PNG and open it in the expanded ring.
pub fn clipboard_image(app: &AppHandle) -> Result<PathBuf> {
    let mut cb = arboard::Clipboard::new().context("opening the clipboard")?;
    let img = cb.get_image().context("the clipboard has no image")?;
    let rgba =
        image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
            .context("clipboard image")?;
    let dir = dirs::picture_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Metamorphosis");
    std::fs::create_dir_all(&dir)?;
    let stamp = morph_core::naming::timestamp();
    let sink = morph_core::OutputSink::new(Default::default(), None, Some(dir.clone()));
    let out = sink.file_named(&dir, &format!("Clipboard {stamp}.png"))?;
    rgba.save_with_format(out.path(), image::ImageFormat::Png)?;
    let path = out.commit()?;
    crate::expanded::show(
        app,
        crate::expanded::Request {
            mode: "ring".into(),
            paths: vec![path.clone()],
            ring: Some("convert".into()),
            op: None,
        },
    )?;
    Ok(path)
}
