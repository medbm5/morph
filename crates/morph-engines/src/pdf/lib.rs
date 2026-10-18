//! Locate and bind the PDFium shared library (bundled per OS, BSD/Apache).

use anyhow::{anyhow, Result};
use pdfium_render::prelude::*;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

static PDFIUM: OnceLock<std::result::Result<Mutex<Pdfium>, String>> = OnceLock::new();

fn lib_name() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Places the library may live, most specific first.
pub fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("MORPH_PDFIUM") {
        out.push(PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        out.push(dir.join(lib_name()));
        out.push(dir.join("pdfium").join(lib_name()));
        out.push(dir.join("resources").join("pdfium").join(lib_name()));
        // Linux packages: /usr/bin/metamorphosis → /usr/lib/Metamorphosis/pdfium/
        out.push(dir.join("../lib/Metamorphosis/pdfium").join(lib_name()));
        out.push(dir.join("../lib/metamorphosis/pdfium").join(lib_name()));
        // `cargo test` binaries live in target/<profile>/deps.
        out.push(dir.join("..").join(lib_name()));
    }
    out.push(morph_core::components::component_dir("pdfium").join(lib_name()));
    out
}

/// Is PDFium loadable?
pub fn available() -> bool {
    pdfium().is_ok()
}

/// The process-wide PDFium instance (PDFium binds once per process).
pub fn pdfium() -> Result<MutexGuard<'static, Pdfium>> {
    let cell = PDFIUM.get_or_init(|| {
        for c in candidates() {
            if c.is_file() {
                match Pdfium::bind_to_library(&c) {
                    Ok(b) => return Ok(Mutex::new(Pdfium::new(b))),
                    Err(e) => log::warn!("couldn't load {}: {e}", c.display()),
                }
            }
        }
        Pdfium::bind_to_system_library()
            .map(|b| Mutex::new(Pdfium::new(b)))
            .map_err(|e| format!("PDFium library not found ({e}); run scripts/fetch_pdfium.py or reinstall Metamorphosis"))
    });
    match cell {
        Ok(m) => Ok(m.lock().unwrap_or_else(|p| p.into_inner())),
        Err(e) => Err(anyhow!(e.clone())),
    }
}
