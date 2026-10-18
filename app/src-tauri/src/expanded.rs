//! Expanded mode: editors, settings and file-manager entry points open the HUD
//! as a regular window on the grid background.

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "expanded";

/// What the expanded window should show next.
#[derive(serde::Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// `editor`, `ring` (file-manager entry), `settings`, `history`.
    pub mode: String,
    pub op: Option<String>,
    pub paths: Vec<PathBuf>,
    pub ring: Option<String>,
}

#[derive(Default)]
pub struct Pending(pub Mutex<Option<Request>>);

pub fn open(app: &AppHandle, op: &str, paths: &[PathBuf]) -> anyhow::Result<()> {
    show(
        app,
        Request {
            mode: "editor".into(),
            op: Some(op.into()),
            paths: paths.to_vec(),
            ring: None,
        },
    )
}

pub fn show(app: &AppHandle, req: Request) -> anyhow::Result<()> {
    *app.state::<Pending>().0.lock().unwrap() = Some(req.clone());
    if let Some(w) = app.get_webview_window(LABEL) {
        use tauri::Emitter;
        w.emit("expanded://request", req)?;
        let _ = w.unminimize();
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("expanded.html".into()))
        .title("Metamorphosis")
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .build()?;
    Ok(())
}

/// The expanded window asks what to show when it loads.
#[tauri::command]
pub fn expanded_request(pending: tauri::State<Pending>) -> Option<Request> {
    pending.0.lock().unwrap().clone()
}
