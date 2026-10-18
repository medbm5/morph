//! Commands the webviews call.

use crate::state::AppState;
use morph_core::{detect, Family, Job, JobEvent, Kind, OutputKind, Params, Ring, Runner};
use morph_input_watch::RingKind;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// One ring segment.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub label: String,
    /// Name the first output would get (`Page 001 Cropped.jpg`).
    pub out: Option<String>,
    pub editor: Option<String>,
    /// `INSTALL` when the op needs a missing optional component.
    pub badge: Option<String>,
    pub disabled: bool,
    pub component: Option<String>,
}

/// Centre readout for the selection.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub name: String,
    pub size: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub count: usize,
    pub dim: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RingItems {
    pub items: Vec<Item>,
    pub file: FileInfo,
}

fn ring_of(r: RingKind) -> Ring {
    match r {
        RingKind::Convert => Ring::Convert,
        RingKind::Tools => Ring::Tools,
        RingKind::Actions => Ring::Actions,
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

fn file_info(paths: &[PathBuf], kinds: &[Kind]) -> FileInfo {
    let size: u64 = paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    let first = kinds.first().map(Kind::label).unwrap_or_default();
    let kind = if kinds.iter().all(|k| k.label() == first) {
        first
    } else {
        "MIXED".into()
    };
    let name = match paths {
        [one] => one
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        many => format!("{} items", many.len()),
    };
    FileInfo {
        name,
        size: if paths.len() > 1 {
            format!("{} ITEMS", paths.len())
        } else {
            human_size(size)
        },
        kind,
        count: paths.len(),
        dim: None,
    }
}

/// Ring segments for a selection.
#[tauri::command]
pub fn ring_items(
    state: State<AppState>,
    paths: Vec<PathBuf>,
    ring: RingKind,
) -> Result<RingItems, String> {
    log::debug!("ring_items {ring:?} for {} path(s)", paths.len());
    let kinds: Vec<Kind> = paths.iter().filter_map(|p| detect(p).ok()).collect();
    if kinds.len() != paths.len() || paths.is_empty() {
        return Err("unreadable selection".into());
    }
    let registry = state.registry();
    let settings = state.settings();
    let naming = settings.naming.clone();
    let items: Vec<Item> = registry
        .for_selection(&kinds, ring_of(ring))
        .into_iter()
        .map(|op| {
            let s = op.spec();
            let first = &paths[0];
            let stem = morph_core::output::stem_of(first);
            let out = match &s.output {
                OutputKind::Ext(ext) => Some(naming.render(&stem, s.suffix.as_deref(), ext)),
                OutputKind::SameAsInput => {
                    Some(naming.render(&stem, s.suffix.as_deref(), &kinds[0].ext))
                }
                OutputKind::Folder => Some(naming.render(&stem, s.suffix.as_deref(), "")),
                OutputKind::None => None,
            };
            let available = op.available();
            Item {
                id: s.id.clone(),
                label: s.label.clone(),
                out,
                editor: s.editor.clone(),
                badge: (!available).then(|| "INSTALL".to_string()),
                disabled: !available,
                component: s.component.clone(),
            }
        })
        .collect();
    // The user's layout for this ring and family (when the selection is one family).
    let family = kinds[0].family;
    let items = match settings.layout(ring_of(ring), family) {
        Some(l) if kinds.iter().all(|k| k.family == family) => l.apply(items, |i| &i.id),
        _ => items,
    };
    let mut file = file_info(&paths, &kinds);
    if kinds.len() == 1 && kinds[0].family == Family::Image {
        file.dim = image_dims(&paths[0]);
    }
    Ok(RingItems { items, file })
}

fn image_dims(p: &std::path::Path) -> Option<String> {
    let (w, h) = morph_engines::image::probe_size(p)?;
    Some(format!("{w} × {h}"))
}

static JOB_IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobPayload {
    job: u64,
    #[serde(flatten)]
    event: JobEvent,
}

/// Run an operation on `paths` in the background. Progress arrives as `job://event`.
#[tauri::command]
pub fn run_op(
    app: AppHandle,
    state: State<AppState>,
    op: String,
    paths: Vec<PathBuf>,
    params: Option<Params>,
) -> u64 {
    let id = JOB_IDS.fetch_add(1, Ordering::Relaxed);
    log::info!("job {id}: {op} on {} file(s)", paths.len());
    let cancel = Arc::new(AtomicBool::new(false));
    state.jobs.lock().unwrap().insert(id, cancel.clone());
    let registry = state.registry();
    let settings = state.settings();
    std::thread::Builder::new()
        .name(format!("morph-job-{id}"))
        .spawn(move || {
            let mut runner = Runner::new(&registry).naming(settings.naming.clone());
            runner.cancel = cancel;
            let job = Job {
                op,
                inputs: paths,
                params: params.unwrap_or_default(),
                out_dir: None,
            };
            let emitter = app.clone();
            let result = runner.run(&job, move |event| {
                let _ = emitter.emit("job://event", JobPayload { job: id, event });
            });
            // Actions (Copy, Move, Share…) aren't undoable jobs: Undo must never trash originals.
            let is_action = registry
                .get(&job.op)
                .is_some_and(|o| o.spec().ring == Ring::Actions);
            if let (Ok(report), false, true) = (&result, is_action, settings.history) {
                if !report.outputs.is_empty() {
                    let label = registry
                        .get(&job.op)
                        .map(|o| o.spec().label.clone())
                        .unwrap_or_default();
                    let h = morph_core::history::History::default();
                    match h.record(&job.op, &label, &job.inputs, &report.outputs, &job.params) {
                        Ok(e) => {
                            let _ = app.emit(
                                "history://recorded",
                                serde_json::json!({ "job": id, "entry": e }),
                            );
                        }
                        Err(e) => log::warn!("history: {e}"),
                    }
                }
            }
            if let Err(e) = result {
                let _ = app.emit(
                    "job://event",
                    JobPayload {
                        job: id,
                        event: JobEvent::Failed {
                            input: None,
                            error: e.to_string(),
                        },
                    },
                );
                let _ = app.emit(
                    "job://event",
                    JobPayload {
                        job: id,
                        event: JobEvent::Finished {
                            saved: 0,
                            failed: 1,
                            cancelled: false,
                        },
                    },
                );
            }
            app.state::<AppState>().jobs.lock().unwrap().remove(&id);
        })
        .expect("spawning job thread");
    id
}

#[tauri::command]
pub fn cancel_job(state: State<AppState>, job: u64) {
    if let Some(flag) = state.jobs.lock().unwrap().get(&job) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Close the ring overlay and forget the drag.
#[tauri::command]
pub fn hide_ring(app: AppHandle, state: State<AppState>) {
    log::debug!("hide ring");
    crate::overlay::hide(&app);
    state.ring.lock().unwrap().open = None;
    if let Some(w) = state.watch.lock().unwrap().as_ref() {
        w.close_ring();
    }
}

/// The overlay painted its first frame: record show latency.
#[tauri::command]
pub fn ring_shown(state: State<AppState>) -> Option<f64> {
    let mut s = state.ring.lock().unwrap();
    let ms = s
        .opened_at
        .take()
        .map(|t| t.elapsed().as_secs_f64() * 1000.0);
    if let Some(ms) = ms {
        s.last_latency_ms = Some(ms);
        log::info!("ring first frame after {ms:.1} ms");
    }
    ms
}

#[tauri::command]
pub fn reveal(path: PathBuf) -> Result<(), String> {
    morph_platform::reveal(&path).map_err(|e| format!("{e:#}"))
}

/// Open an operation's editor in expanded mode.
#[tauri::command]
pub fn open_editor(app: AppHandle, op: String, paths: Vec<PathBuf>) -> Result<(), String> {
    crate::expanded::open(&app, &op, &paths).map_err(|e| format!("{e:#}"))
}

/// Full spec of an operation (parameters drive the generic editor form).
#[tauri::command]
pub fn op_spec(state: State<AppState>, op: String) -> Result<morph_core::OpSpec, String> {
    state
        .registry()
        .get(&op)
        .map(|o| o.spec().clone())
        .ok_or_else(|| format!("unknown operation {op}"))
}

/// Target panel info for a selection.
#[tauri::command]
pub fn describe(paths: Vec<PathBuf>) -> Result<FileInfo, String> {
    let kinds: Vec<Kind> = paths
        .iter()
        .map(|p| detect(p).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let mut info = file_info(&paths, &kinds);
    if kinds.len() == 1 && kinds[0].family == Family::Image {
        info.dim = image_dims(&paths[0]);
    }
    info.kind = match kinds.as_slice() {
        [k] => format!("{} {}", k.label(), family_word(k.family)),
        _ => info.kind,
    };
    Ok(info)
}

fn family_word(f: Family) -> &'static str {
    match f {
        Family::Image => "IMAGE",
        Family::Pdf => "DOCUMENT",
        Family::Video => "VIDEO",
        Family::Audio => "AUDIO",
        Family::Document => "DOCUMENT",
        Family::Slides => "SLIDES",
        Family::Spreadsheet => "SPREADSHEET",
        Family::Subtitle => "SUBTITLES",
        Family::Archive => "ARCHIVE",
        Family::Folder => "",
        Family::Other => "FILE",
    }
}

/// Metadata fields of an image (EXIF, GPS, XMP, IPTC) for the metadata editor.
#[tauri::command]
pub fn image_metadata(path: PathBuf) -> Result<Vec<morph_engines::image::metadata::Field>, String> {
    morph_engines::image::metadata::read(&path).map_err(|e| format!("{e:#}"))
}

/// Pixel size of an image (editors size their canvas from it).
#[tauri::command]
pub fn image_size(path: PathBuf) -> Option<(u32, u32)> {
    morph_engines::image::probe_size(&path)
}

/// Diagnostics from the webviews.
#[tauri::command]
pub fn trace(msg: String) {
    log::debug!("ui: {msg}");
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub svg: String,
    pub width: u32,
    pub height: u32,
    /// Files behind `morph:0`, `morph:1`… in the SVG.
    pub sources: Vec<PathBuf>,
}

/// SVG preview of a composition (annotate, background, collage) — the same scene the export renders.
#[tauri::command]
pub async fn compose_preview(
    op: String,
    paths: Vec<PathBuf>,
    params: Params,
) -> Result<Preview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let inputs: Vec<(PathBuf, Kind)> = paths
            .iter()
            .map(|p| detect(p).map(|k| (p.clone(), k)))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        let (scene, sources) = morph_engines::image::tools2::preview(&op, &inputs, &params)
            .map_err(|e| format!("{e:#}"))?;
        Ok(Preview {
            svg: scene.svg,
            width: scene.width,
            height: scene.height,
            sources,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Faces and/or text found locally in an image (Redact editor's "Detect").
#[tauri::command]
pub async fn detect_regions(
    path: PathBuf,
    faces: bool,
    text: bool,
) -> Result<Vec<morph_engines::ml::Region>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let kind = detect(&path).map_err(|e| e.to_string())?;
        let img =
            morph_engines::image::decode::decode(&path, &kind).map_err(|e| format!("{e:#}"))?;
        let mut p = Params::new();
        let auto = match (faces, text) {
            (true, true) => "faces+text",
            (true, false) => "faces",
            (false, true) => "text",
            _ => "none",
        };
        p.set("auto", auto);
        morph_engines::ml::ops::redact_regions(&img, &p).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Optional components and whether they are installed.
#[tauri::command]
pub fn components_status() -> Vec<morph_updater::Status> {
    morph_updater::status()
}

/// Download, verify and install a component; progress arrives as `component://progress`.
#[tauri::command]
pub async fn install_component(app: AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let emitter = app.clone();
        let n = name.clone();
        let r = morph_updater::install(&name, &move |done, total, _| {
            let _ = emitter.emit(
                "component://progress",
                serde_json::json!({ "name": n, "done": done, "total": total }),
            );
        })
        .map_err(|e| format!("{e:#}"));
        let _ = app.emit("component://changed", &name);
        r
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Show the components screen (from an INSTALL COMPONENT segment).
#[tauri::command]
pub fn open_components(app: AppHandle, focus: Option<String>) -> Result<(), String> {
    crate::expanded::show(
        &app,
        crate::expanded::Request {
            mode: "components".into(),
            op: focus,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:#}"))
}

/// Undo the last job: its outputs go to the Recycle Bin / Trash.
#[tauri::command]
pub fn undo_last() -> Result<Option<String>, String> {
    let h = morph_core::history::History::default();
    let Some(e) = h.last_undoable() else {
        return Ok(None);
    };
    let paths = morph_core::history::existing(&e.outputs);
    morph_platform::trash(&paths).map_err(|e| format!("{e:#}"))?;
    h.mark_undone(e.id).map_err(|e| e.to_string())?;
    log::info!("undid job {} ({} file(s) to trash)", e.id, paths.len());
    Ok(Some(format!("{} · {} file(s)", e.label, paths.len())))
}

/// Recent jobs, newest first.
#[tauri::command]
pub fn history() -> Vec<morph_core::history::Entry> {
    let mut v = morph_core::history::History::default().load();
    v.reverse();
    v
}

/// Save the drag-chain that led to `job` as a recipe in the user's recipe folder.
#[tauri::command]
pub fn save_recipe(app: AppHandle, job: u64, name: String) -> Result<String, String> {
    use morph_core::recipe::{RecipeFile, StepFile};
    let steps = morph_core::history::History::default().chain(job);
    if steps.is_empty() {
        return Err("nothing to save".into());
    }
    let step = steps
        .iter()
        .map(|s| StepFile {
            op: s.op.clone(),
            params: serde_json::from_value(serde_json::Value::Object(s.params.0.clone()))
                .unwrap_or_default(),
        })
        .collect();
    let file = RecipeFile {
        name: name.trim().to_string(),
        ring: "tools".into(),
        step,
        ..Default::default()
    };
    let dir = morph_core::recipe::user_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let slug = file.id().trim_start_matches("recipe.").to_string();
    let path = dir.join(format!("{slug}.toml"));
    std::fs::write(&path, file.to_toml()).map_err(|e| e.to_string())?;
    reload_registry_for(&app);
    Ok(path.display().to_string())
}

/// Rebuild the registry (engines + recipes) after recipe changes.
pub fn reload_registry_for(app: &AppHandle) {
    let (mut reg, errors) = morph_engines::registry_with_recipes();
    for (p, e) in errors {
        log::warn!("skipping recipe {}: {e}", p.display());
    }
    morph_platform::actions::register(&mut reg, crate::actions::hooks(app));
    log::info!("{} operations registered", reg.len());
    *app.state::<AppState>().registry.write().unwrap() = Arc::new(reg);
}

/// Show a failed job's full message (a recipe command's stderr) in expanded mode.
#[tauri::command]
pub fn show_error(app: AppHandle, title: String, text: String) -> Result<(), String> {
    crate::expanded::show(
        &app,
        crate::expanded::Request {
            mode: "error".into(),
            op: Some(title),
            text: Some(text),
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn open_history(app: AppHandle) -> Result<(), String> {
    crate::expanded::show(
        &app,
        crate::expanded::Request {
            mode: "history".into(),
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub duration: f64,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

#[tauri::command]
pub async fn video_info(path: PathBuf) -> Result<VideoInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = morph_engines::media::ffmpeg::probe(&path).map_err(|e| format!("{e:#}"))?;
        let v = p.video.clone().unwrap_or_default();
        Ok(VideoInfo {
            duration: p.duration,
            fps: if v.fps > 0.0 { v.fps } else { 30.0 },
            width: v.width,
            height: v.height,
            has_audio: p.audio.is_some(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn frame_cache() -> PathBuf {
    let d = morph_core::components::data_dir()
        .join("cache")
        .join("frames");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn cache_key(path: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .hash(&mut h);
    format!("{:016x}", h.finish())
}

/// One exact frame at `t` seconds as a JPEG in the cache (the editor's preview).
#[tauri::command]
pub async fn video_frame(path: PathBuf, t: f64, width: u32) -> Result<PathBuf, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let out = frame_cache().join(format!("{}-{:.3}-{width}.jpg", cache_key(&path), t));
        if !out.exists() {
            let args: Vec<String> = vec![
                "-ss".into(),
                format!("{:.3}", t.max(0.0)),
                "-i".into(),
                path.to_string_lossy().into_owned(),
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                format!("scale={width}:-2"),
                "-update".into(),
                "1".into(),
                "-q:v".into(),
                "3".into(),
                "-f".into(),
                "image2".into(),
                out.to_string_lossy().into_owned(),
            ];
            morph_engines::media::ffmpeg::run(&args, 0.0, &morph_core::Progress::silent())
                .map_err(|e| format!("{e:#}"))?;
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `count` evenly spaced proxy frames for fast scrubbing (one ffmpeg pass).
#[tauri::command]
pub async fn video_strip(path: PathBuf, count: u32, width: u32) -> Result<Vec<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = morph_engines::media::ffmpeg::probe(&path).map_err(|e| format!("{e:#}"))?;
        let dir = frame_cache().join(format!("{}-strip-{count}-{width}", cache_key(&path)));
        let pattern = dir.join("f%04d.jpg");
        if !dir.join(".done").exists() {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let rate = count as f64 / p.duration.max(0.1);
            let args: Vec<String> = vec![
                "-i".into(),
                path.to_string_lossy().into_owned(),
                "-vf".into(),
                format!("fps={rate:.6},scale={width}:-2"),
                "-q:v".into(),
                "5".into(),
                "-f".into(),
                "image2".into(),
                pattern.to_string_lossy().into_owned(),
            ];
            morph_engines::media::ffmpeg::run(&args, p.duration, &morph_core::Progress::silent())
                .map_err(|e| format!("{e:#}"))?;
            let _ = std::fs::write(dir.join(".done"), "");
        }
        let mut frames: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .collect();
        frames.sort();
        Ok(frames)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Waveform peaks for the audio editor.
#[tauri::command]
pub async fn audio_peaks(path: PathBuf, buckets: usize) -> Result<Vec<(f32, f32)>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        morph_engines::media::audio::peaks(&path, buckets.clamp(10, 4000))
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Integrated loudness, range and true peak (before/after meters).
#[tauri::command]
pub async fn audio_loudness(
    path: PathBuf,
) -> Result<morph_engines::media::audio::Loudness, String> {
    tauri::async_runtime::spawn_blocking(move || {
        morph_engines::media::audio::measure(&path).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Run an operation into the preview cache (A/B listening) and return the result.
#[tauri::command]
pub async fn preview_op(
    state: State<'_, AppState>,
    op: String,
    paths: Vec<PathBuf>,
    params: Params,
) -> Result<PathBuf, String> {
    let registry = state.registry();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = morph_core::components::data_dir()
            .join("cache")
            .join("preview");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let job = Job {
            op,
            inputs: paths,
            params,
            out_dir: Some(dir),
        };
        let r = Runner::new(&registry)
            .run(&job, |_| {})
            .map_err(|e| e.to_string())?;
        if let Some((_, e)) = r.errors.first() {
            return Err(e.clone());
        }
        r.outputs
            .into_iter()
            .next()
            .ok_or_else(|| "no output".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Page thumbnails of a PDF (cached JPEGs) for the organizer and merge editors.
#[tauri::command]
pub async fn pdf_thumbs(path: PathBuf, width: u32) -> Result<Vec<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = frame_cache().join(format!("{}-pdf-{width}", cache_key(&path)));
        if !dir.join(".done").exists() {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            morph_engines::pdf::render_thumbs(&path, width, &dir).map_err(|e| format!("{e:#}"))?;
            let _ = std::fs::write(dir.join(".done"), "");
        }
        let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .collect();
        v.sort();
        Ok(v)
    })
    .await
    .map_err(|e| e.to_string())?
}
