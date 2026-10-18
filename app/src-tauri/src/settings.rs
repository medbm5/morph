//! Settings commands: load, save and apply (trigger keys, autostart, accent and
//! motion in every window), ring layout editing and portable mode.

use crate::state::AppState;
use morph_core::settings::{layout_key, Settings};
use morph_core::{Family, Kind, Ring};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// What the webviews need to theme themselves.
#[derive(Serialize, Clone)]
pub struct Look {
    pub accent: String,
    pub reduced: bool,
    pub language: String,
}

pub fn look(s: &Settings) -> Look {
    Look {
        accent: s.accent.clone(),
        reduced: s.motion.reduced(morph_platform::reduced_motion()),
        language: language(s),
    }
}

/// `en` or `fr`: the setting, or the system language for `auto`.
pub fn language(s: &Settings) -> String {
    match s.language.as_str() {
        "en" | "fr" => s.language.clone(),
        _ => {
            let sys = morph_platform::locale()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if sys.starts_with("fr") { "fr" } else { "en" }.into()
        }
    }
}

pub fn triggers(s: &Settings) -> morph_input_watch::Triggers {
    use morph_input_watch::{Key, Triggers};
    let d = Triggers::default();
    Triggers {
        convert: Key::parse(&s.keys.convert).unwrap_or(d.convert),
        tools: Key::parse(&s.keys.tools).unwrap_or(d.tools),
        actions: Key::parse(&s.keys.actions).unwrap_or(d.actions),
    }
}

/// Apply settings that live outside the webviews. Called at startup and on save
/// (`explicit`): development builds only touch autostart when the user asks.
pub fn apply(app: &AppHandle, s: &Settings, explicit: bool) {
    let state = app.state::<AppState>();
    if let Some(w) = state.watch.lock().unwrap().as_ref() {
        w.set_triggers(triggers(s));
    }
    if let Ok(exe) = std::env::current_exe() {
        // Portable copies don't touch the system.
        if morph_core::components::portable_dir().is_none()
            && (explicit || !cfg!(debug_assertions))
            && morph_platform::autostart::is_enabled() != s.autostart
        {
            if let Err(e) = morph_platform::autostart::set(&exe, s.autostart) {
                log::warn!("autostart: {e:#}");
            }
        }
    }
    crate::watch::folders(app, &s.watch);
    if explicit {
        crate::tray::refresh(app);
    }
    let _ = app.emit("settings://changed", look(s));
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    let keys = [
        &settings.keys.convert,
        &settings.keys.tools,
        &settings.keys.actions,
    ];
    if keys
        .iter()
        .any(|k| morph_input_watch::Key::parse(k).is_none())
    {
        return Err("unknown trigger key".into());
    }
    if keys[0] == keys[1] || keys[1] == keys[2] || keys[0] == keys[2] {
        return Err("each ring needs its own key".into());
    }
    if !(settings.naming.convert.contains("{stem}") && settings.naming.tool.contains("{stem}")) {
        return Err("naming templates need {stem}".into());
    }
    settings.save().map_err(|e| format!("{e:#}"))?;
    *state.settings.write().unwrap() = settings.clone();
    apply(&app, &settings, true);
    Ok(())
}

#[tauri::command]
pub fn get_look(state: State<AppState>) -> Look {
    look(&state.settings())
}

/// A representative kind for each family (for the layout editor).
fn sample(family: Family) -> Option<Kind> {
    let ext = match family {
        Family::Image => "png",
        Family::Pdf => "pdf",
        Family::Video => "mp4",
        Family::Audio => "mp3",
        Family::Document => "docx",
        Family::Slides => "pptx",
        Family::Spreadsheet => "xlsx",
        Family::Subtitle => "srt",
        Family::Archive => "zip",
        Family::Folder => return Some(Kind::folder()),
        Family::Other => return None,
    };
    morph_core::kind::by_extension(ext)
}

#[derive(Serialize)]
pub struct LayoutOp {
    pub id: String,
    pub label: String,
}

/// The default ring for a family, before any layout is applied.
#[tauri::command]
pub fn layout_ops(state: State<AppState>, ring: Ring, family: Family) -> Vec<LayoutOp> {
    let Some(kind) = sample(family) else {
        return vec![];
    };
    state
        .registry()
        .for_selection(&[kind], ring)
        .into_iter()
        .map(|o| LayoutOp {
            id: o.spec().id.clone(),
            label: o.spec().label.clone(),
        })
        .collect()
}

#[tauri::command]
pub fn layout_key_for(ring: Ring, family: Family) -> String {
    layout_key(ring, family)
}

#[derive(Serialize)]
pub struct Portable {
    /// Portable mode is offered on Windows only.
    pub supported: bool,
    pub on: bool,
    pub folder: String,
}

#[tauri::command]
pub fn portable_status() -> Portable {
    let folder = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.display().to_string()))
        .unwrap_or_default();
    Portable {
        supported: cfg!(windows),
        on: morph_core::components::portable_dir().is_some(),
        folder,
    }
}

/// Make this copy portable: settings, recipes and history move next to the
/// executable. Takes effect after a restart.
#[tauri::command]
pub fn set_portable(on: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("no folder")?.to_path_buf();
    let marker = dir.join("portable");
    if on {
        // Same layout as components::config_dir/data_dir in portable mode.
        let (config, data) = (dir.join("config"), dir.join("data"));
        let from_config = morph_core::components::config_dir();
        let from_data = morph_core::components::data_dir();
        std::fs::write(&marker, "Metamorphosis portable mode\n").map_err(|e| {
            format!("can't write next to the app ({e}); copy it to a writable folder first")
        })?;
        let copy = |from: std::path::PathBuf, to: std::path::PathBuf| {
            if from.is_file() && !to.exists() {
                let _ = std::fs::create_dir_all(to.parent().unwrap());
                let _ = std::fs::copy(&from, &to);
            }
        };
        copy(
            from_config.join("settings.toml"),
            config.join("settings.toml"),
        );
        copy(from_data.join("history.json"), data.join("history.json"));
        if let Ok(rd) = std::fs::read_dir(from_config.join("recipes")) {
            for e in rd.flatten() {
                copy(e.path(), config.join("recipes").join(e.file_name()));
            }
        }
    } else if marker.exists() {
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Folder picker for watch folders.
#[tauri::command]
pub async fn pick_folder() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title("Watch folder")
        .pick_folder()
        .await
        .map(|f| f.path().display().to_string())
}

#[derive(Serialize)]
pub struct WatchOp {
    pub id: String,
    pub label: String,
    pub ring: Ring,
}

/// Operations a watch folder can apply: conversions, tools and recipes that take
/// one file at a time.
#[tauri::command]
pub fn watch_ops(state: State<AppState>) -> Vec<WatchOp> {
    let mut ops: Vec<WatchOp> = state
        .registry()
        .all()
        .filter(|o| {
            let s = o.spec();
            s.ring != Ring::Actions && s.arity == morph_core::Arity::Each
        })
        .map(|o| WatchOp {
            id: o.spec().id.clone(),
            label: o.spec().label.clone(),
            ring: o.spec().ring,
        })
        .collect();
    ops.sort_by(|a, b| (a.ring as u8, &a.label).cmp(&(b.ring as u8, &b.label)));
    ops
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    crate::expanded::show(
        &app,
        crate::expanded::Request {
            mode: "settings".into(),
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:#}"))
}
