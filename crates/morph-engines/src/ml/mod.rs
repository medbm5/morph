//! Local AI (ONNX Runtime, CPU): faces, text regions, OCR, background removal.
//!
//! ONNX Runtime and the models are an optional component (`morph component install ml`):
//! the runtime is loaded dynamically from the component folder, so the base install
//! carries none of it. Nothing here touches the network.

pub mod bg;
pub mod face;
pub mod ops;
pub mod text;

use anyhow::{anyhow, Result};
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use ort::session::Session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// A detected region in source pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

impl Region {
    pub fn iou(&self, o: &Region) -> f32 {
        let x1 = self.x.max(o.x);
        let y1 = self.y.max(o.y);
        let x2 = (self.x + self.w).min(o.x + o.w);
        let y2 = (self.y + self.h).min(o.y + o.h);
        let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let union = self.w * self.h + o.w * o.h - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    /// Grow by `f` of its size on every side (redaction margins), clamped to the image.
    pub fn grow(&self, f: f32, w: u32, h: u32) -> Region {
        let (dx, dy) = (self.w * f, self.h * f);
        let x = (self.x - dx).max(0.0);
        let y = (self.y - dy).max(0.0);
        Region {
            x,
            y,
            w: (self.w + 2.0 * dx).min(w as f32 - x),
            h: (self.h + 2.0 * dy).min(h as f32 - y),
            score: self.score,
        }
    }
}

/// Greedy non-maximum suppression.
pub fn nms(mut r: Vec<Region>, iou: f32) -> Vec<Region> {
    r.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep: Vec<Region> = Vec::new();
    for c in r {
        if keep.iter().all(|k| k.iou(&c) < iou) {
            keep.push(c);
        }
    }
    keep
}

fn lib_path() -> PathBuf {
    let dir = morph_core::components::component_dir("ml");
    dir.join(if cfg!(windows) {
        "onnxruntime.dll"
    } else {
        "libonnxruntime.so"
    })
}

/// Is the ML component installed?
pub fn available() -> bool {
    morph_core::components::is_installed("ml") && lib_path().is_file()
}

/// Path of a model file inside a component.
pub fn model(component: &str, file: &str) -> Result<PathBuf> {
    let p = morph_core::components::component_dir(component).join(file);
    if !p.is_file() {
        return Err(anyhow!(morph_core::Error::ComponentMissing(
            component.into()
        )));
    }
    Ok(p)
}

static RUNTIME: OnceLock<std::result::Result<ort::environment::Environment, String>> =
    OnceLock::new();
static SESSIONS: OnceLock<Mutex<HashMap<PathBuf, Session>>> = OnceLock::new();

fn runtime() -> Result<&'static ort::environment::Environment> {
    RUNTIME
        .get_or_init(|| {
            if !available() {
                return Err(morph_core::Error::ComponentMissing("ml".into()).to_string());
            }
            let env =
                ort::init_from(lib_path()).map_err(|e| format!("loading ONNX Runtime: {e}"))?;
            env.with_name("metamorphosis")
                .with_telemetry(false)
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow!(e.clone()))
}

/// Run `f` with a cached session for `model`.
pub fn with_session<T>(
    model: &std::path::Path,
    f: impl FnOnce(&mut Session) -> Result<T>,
) -> Result<T> {
    let env = runtime()?;
    let map = SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().unwrap_or_else(|p| p.into_inner());
    if !map.contains_key(model) {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);
        let s = Session::builder(env)
            .map_err(|e| anyhow!("{e}"))?
            .with_intra_threads(threads)
            .map_err(|e| anyhow!("{e}"))?
            .commit_from_file(model)
            .map_err(|e| anyhow!("loading {}: {e}", model.display()))?;
        map.insert(model.to_path_buf(), s);
    }
    f(map.get_mut(model).unwrap())
}

/// Resize to exactly (w, h) and lay out as NCHW f32 with a per-channel transform.
pub fn to_nchw(
    img: &DynamicImage,
    w: u32,
    h: u32,
    bgr: bool,
    f: impl Fn(usize, f32) -> f32,
) -> Vec<f32> {
    let rgb = if img.dimensions() == (w, h) {
        img.to_rgb8()
    } else {
        img.resize_exact(w, h, FilterType::Triangle).to_rgb8()
    };
    let plane = (w * h) as usize;
    let mut out = vec![0f32; 3 * plane];
    for (i, p) in rgb.pixels().enumerate() {
        for c in 0..3 {
            let src = if bgr { p[2 - c] } else { p[c] };
            out[c * plane + i] = f(c, src as f32);
        }
    }
    out
}
