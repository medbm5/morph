//! File watching: recipes hot-reload when their folder changes, and watch folders
//! run new files through an operation.

use crate::state::AppState;
use morph_core::settings::WatchFolder;
use morph_core::{Job, Runner};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

static RECIPES: Mutex<Option<RecommendedWatcher>> = Mutex::new(None);
static FOLDERS: Mutex<Option<RecommendedWatcher>> = Mutex::new(None);

/// Reload the registry whenever a recipe file is added, changed or removed.
pub fn recipes(app: &AppHandle) {
    let dir = morph_core::recipe::user_dir();
    let _ = std::fs::create_dir_all(&dir);
    let (tx, rx) = mpsc::channel::<()>();
    let mut w = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            let toml = e
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|x| x == "toml"));
            if toml && !matches!(e.kind, EventKind::Access(_)) {
                let _ = tx.send(());
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("recipe watcher: {e}");
            return;
        }
    };
    if let Err(e) = w.watch(&dir, RecursiveMode::NonRecursive) {
        log::warn!("watching {}: {e}", dir.display());
        return;
    }
    *RECIPES.lock().unwrap() = Some(w);
    let app = app.clone();
    std::thread::Builder::new()
        .name("morph-recipes".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                // Editors save in bursts (temp file, rename, touch): settle first.
                while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
                log::info!("recipes changed; reloading");
                crate::commands::reload_registry_for(&app);
                let _ = app.emit("registry://changed", ());
            }
        })
        .expect("spawning recipe watcher");
}

/// (Re)start watching the configured folders.
pub fn folders(app: &AppHandle, list: &[WatchFolder]) {
    let list: Vec<WatchFolder> = list
        .iter()
        .filter(|w| w.enabled && !w.folder.is_empty() && !w.op.is_empty())
        .cloned()
        .collect();
    // Dropping the old watcher ends its worker thread (the channel closes).
    *FOLDERS.lock().unwrap() = None;
    if list.is_empty() {
        return;
    }
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut w = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(e) = res {
            if matches!(e.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                for p in e.paths {
                    let _ = tx.send(p);
                }
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("folder watcher: {e}");
            return;
        }
    };
    for f in &list {
        match w.watch(std::path::Path::new(&f.folder), RecursiveMode::NonRecursive) {
            Ok(()) => log::info!("watching {} → {}", f.folder, f.op),
            Err(e) => log::warn!("watching {}: {e}", f.folder),
        }
    }
    *FOLDERS.lock().unwrap() = Some(w);
    let app = app.clone();
    std::thread::Builder::new()
        .name("morph-watch".into())
        .spawn(move || worker(app, list, rx))
        .expect("spawning folder watcher");
}

fn worker(app: AppHandle, list: Vec<WatchFolder>, rx: mpsc::Receiver<PathBuf>) {
    // Files seen but not yet settled, and when each was last touched.
    let mut pending: HashMap<PathBuf, (usize, Instant, u64)> = HashMap::new();
    // Recently processed, so a late Modify event doesn't run it twice.
    let mut done: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(p) => {
                if let Some(i) = list.iter().position(|w| w.wants(&p)) {
                    if done
                        .get(&p)
                        .is_none_or(|t| t.elapsed() > Duration::from_secs(30))
                    {
                        pending.insert(p, (i, Instant::now(), u64::MAX));
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        // A file is ready once its size has held for a second (copies and downloads
        // finish writing) and it can be opened.
        let mut ready = Vec::new();
        for (p, (i, since, size)) in pending.iter_mut() {
            let Ok(meta) = std::fs::metadata(p) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            if meta.len() != *size {
                *size = meta.len();
                *since = Instant::now();
            } else if since.elapsed() >= Duration::from_secs(1) && std::fs::File::open(p).is_ok() {
                ready.push((p.clone(), *i));
            }
        }
        pending.retain(|p, _| p.exists() && !ready.iter().any(|(r, _)| r == p));
        for (p, i) in ready {
            done.insert(p.clone(), Instant::now());
            process(&app, &list[i], p);
        }
        done.retain(|_, t| t.elapsed() < Duration::from_secs(60));
    }
}

fn process(app: &AppHandle, w: &WatchFolder, path: PathBuf) {
    let state = app.state::<AppState>();
    let registry = state.registry();
    let settings = state.settings();
    let Some(op) = registry.get(&w.op) else {
        log::warn!("watch folder {}: unknown operation {}", w.folder, w.op);
        return;
    };
    match morph_core::detect(&path) {
        Ok(k) if op.spec().accepts_kind(&k) => {}
        _ => {
            log::debug!("watch: {} doesn't take {}", w.op, path.display());
            return;
        }
    }
    let job = Job {
        op: w.op.clone(),
        inputs: vec![path.clone()],
        params: Default::default(),
        out_dir: Some(w.out_dir()),
    };
    let runner = Runner::new(&registry).naming(settings.naming.clone());
    match runner.run(&job, |_| {}) {
        Ok(report) => {
            for (_, e) in &report.errors {
                log::warn!("watch {}: {e}", path.display());
            }
            if !report.outputs.is_empty() {
                log::info!("watch: {} → {:?}", path.display(), report.outputs);
                if settings.history {
                    let label = op.spec().label.clone();
                    let _ = morph_core::history::History::default().record(
                        &job.op,
                        &label,
                        &job.inputs,
                        &report.outputs,
                        &job.params,
                    );
                }
                let _ = app.emit(
                    "watch://processed",
                    serde_json::json!({ "input": path, "outputs": report.outputs }),
                );
            }
        }
        Err(e) => log::warn!("watch {}: {e:#}", path.display()),
    }
}
