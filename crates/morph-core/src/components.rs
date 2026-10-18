//! Where optional components (HEIC codec, FFmpeg, pandoc, LibreOffice, ML models)
//! live, and how engines find their executables. Downloading is the updater's job;
//! this module only looks on disk.

use std::path::{Path, PathBuf};

/// Root folder for optional components.
///
/// `MORPH_COMPONENTS_DIR` wins; portable mode (a `portable` file next to the
/// executable) keeps components beside the app; otherwise the per-user data dir.
pub fn root() -> PathBuf {
    if let Some(d) = std::env::var_os("MORPH_COMPONENTS_DIR") {
        return PathBuf::from(d);
    }
    if let Some(dir) = portable_dir() {
        return dir.join("components");
    }
    data_dir().join("components")
}

/// Per-user data folder (`%LOCALAPPDATA%\morph`, `~/.local/share/morph`).
pub fn data_dir() -> PathBuf {
    if let Some(dir) = portable_dir() {
        return dir.join("data");
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("morph")
}

/// Per-user config folder (`%APPDATA%\morph`, `~/.config/morph`).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = portable_dir() {
        return dir.join("config");
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("morph")
}

/// The executable's folder when running in portable mode.
pub fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("portable").exists().then(|| dir.to_path_buf())
}

/// Folder of one component (`components/ffmpeg`).
pub fn component_dir(name: &str) -> PathBuf {
    root().join(name)
}

/// Is a component's marker file present (written by the installer after checksum checks)?
pub fn is_installed(name: &str) -> bool {
    component_dir(name).join(".installed").exists()
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) && Path::new(name).extension().is_none() {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Find a tool: inside the component folder (top level or `bin/`), next to our own
/// executable (bundled sidecars), then on `PATH`.
pub fn find_tool(component: &str, exe: &str) -> Option<PathBuf> {
    let file = exe_name(exe);
    let comp = component_dir(component);
    for cand in [comp.join(&file), comp.join("bin").join(&file)] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        for cand in [dir.join(&file), dir.join("sidecars").join(&file)] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    find_on_path(exe)
}

/// Look up an executable on `PATH`.
pub fn find_on_path(exe: &str) -> Option<PathBuf> {
    let file = exe_name(exe);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&file))
            .find(|p| p.is_file())
    })
}
