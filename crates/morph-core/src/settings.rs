//! User settings, stored as `settings.toml` in the config folder (or next to the
//! executable in portable mode). Every field has a default and unknown fields are
//! ignored, so a file written by an older or newer version always loads.

use crate::naming::Naming;
use crate::{Family, Ring};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Tap keys that open each ring while dragging: `shift`, `ctrl`, `alt`, `super`.
    pub keys: Keys,
    /// Accent colour as `#RRGGBB`.
    pub accent: String,
    pub motion: Motion,
    pub naming: Naming,
    /// Ring layout per ring and file family, keyed `convert:image`, `tools:pdf`…
    pub layouts: BTreeMap<String, Layout>,
    /// Record jobs in the history (Undo, Run again).
    pub history: bool,
    /// Start in the tray when the session starts.
    pub autostart: bool,
    /// `auto` (follow the system), `en` or `fr`.
    pub language: String,
    /// Folders whose new files run through an operation or recipe.
    pub watch: Vec<WatchFolder>,
}

/// A watched folder: every new file in it runs through `op`; results go to the
/// `out` subfolder (never the watched folder itself, so outputs aren't re-processed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchFolder {
    pub folder: String,
    /// Operation or recipe id (`image.compress`, `recipe.thumbnail-512`).
    pub op: String,
    pub out: String,
    pub enabled: bool,
}

impl Default for WatchFolder {
    fn default() -> Self {
        Self {
            folder: String::new(),
            op: String::new(),
            out: "Metamorphosis".into(),
            enabled: true,
        }
    }
}

impl WatchFolder {
    /// Where results go.
    pub fn out_dir(&self) -> PathBuf {
        let out = crate::naming::sanitize(self.out.trim());
        Path::new(&self.folder).join(if out == "Untitled" {
            "Metamorphosis".into()
        } else {
            out
        })
    }

    /// Should a file-system event on `path` be processed? Only files directly in the
    /// folder, skipping hidden files, our own temp files and unfinished downloads.
    pub fn wants(&self, path: &Path) -> bool {
        if path.parent() != Some(Path::new(&self.folder)) || path.starts_with(self.out_dir()) {
            return false;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        let lower = name.to_ascii_lowercase();
        !(name.starts_with('.')
            || name.starts_with("~$")
            || [".tmp", ".part", ".crdownload", ".download", ".partial"]
                .iter()
                .any(|e| lower.ends_with(e)))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keys: Keys::default(),
            accent: "#FFB347".into(),
            motion: Motion::System,
            naming: Naming::default(),
            layouts: BTreeMap::new(),
            history: true,
            autostart: true,
            language: "auto".into(),
            watch: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    pub convert: String,
    pub tools: String,
    pub actions: String,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            convert: "shift".into(),
            tools: "ctrl".into(),
            actions: "alt".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    /// Follow the operating system's reduced-motion setting.
    #[default]
    System,
    Full,
    Reduced,
}

impl Motion {
    pub fn reduced(self, system: bool) -> bool {
        match self {
            Motion::System => system,
            Motion::Full => false,
            Motion::Reduced => true,
        }
    }
}

/// How one ring is arranged for one file family. Operation ids not listed keep
/// their default place after the ordered ones.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// Preferred order.
    pub order: Vec<String>,
    /// Always first, in this order.
    pub pinned: Vec<String>,
    /// Never shown.
    pub hidden: Vec<String>,
}

impl Layout {
    /// Arrange items (by their operation id) according to this layout.
    pub fn apply<T>(&self, items: Vec<T>, id: impl Fn(&T) -> &str) -> Vec<T> {
        let rank = |i: &str| {
            if let Some(p) = self.pinned.iter().position(|x| x == i) {
                (0, p)
            } else if let Some(p) = self.order.iter().position(|x| x == i) {
                (1, p)
            } else {
                (2, 0)
            }
        };
        let mut kept: Vec<(usize, T)> = items
            .into_iter()
            .filter(|t| !self.hidden.iter().any(|h| h == id(t)))
            .enumerate()
            .collect();
        // Stable: unlisted items keep their default order.
        kept.sort_by_key(|(n, t)| {
            let (group, pos) = rank(id(t));
            (group, pos, *n)
        });
        kept.into_iter().map(|(_, t)| t).collect()
    }
}

/// Layout key for a ring and a family: `convert:image`.
pub fn layout_key(ring: Ring, family: Family) -> String {
    let ring = match ring {
        Ring::Convert => "convert",
        Ring::Tools => "tools",
        Ring::Actions => "actions",
    };
    let family = serde_json::to_value(family)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    format!("{ring}:{family}")
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::components::config_dir().join("settings.toml")
    }

    /// Load from the default location; defaults if missing or unreadable.
    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("{}: {e}; using defaults", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::path())
    }

    /// Write atomically (temp file, then rename) so a crash never leaves half a file.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Layout for a selection of one family, if customised.
    pub fn layout(&self, ring: Ring, family: Family) -> Option<&Layout> {
        self.layouts.get(&layout_key(ring, family))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_survives_old_and_new_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.toml");
        let mut s = Settings {
            accent: "#4FE3FF".into(),
            motion: Motion::Reduced,
            ..Settings::default()
        };
        s.naming.conflict = crate::naming::Conflict::Skip;
        s.layouts.insert(
            "convert:image".into(),
            Layout {
                pinned: vec!["image.to-png".into()],
                ..Layout::default()
            },
        );
        s.save_to(&p).unwrap();
        assert_eq!(Settings::load_from(&p), s);

        // An older file (missing fields) and a newer one (unknown fields) both load.
        std::fs::write(
            &p,
            "accent = \"#9CFF6E\"\nfuture_option = 3\n[naming]\ntool = \"{stem}-{op}.{ext}\"\n",
        )
        .unwrap();
        let s = Settings::load_from(&p);
        assert_eq!(s.accent, "#9CFF6E");
        assert_eq!(s.naming.tool, "{stem}-{op}.{ext}");
        assert_eq!(s.naming.convert, Naming::default().convert);
        assert_eq!(s.keys, Keys::default());
    }

    #[test]
    fn layout_pins_orders_and_hides() {
        let l = Layout {
            pinned: vec!["d".into()],
            order: vec!["c".into(), "a".into()],
            hidden: vec!["b".into()],
        };
        let out = l.apply(vec!["a", "b", "c", "d", "e"], |s| s);
        assert_eq!(out, vec!["d", "c", "a", "e"]);
    }

    #[test]
    fn watch_folder_filter() {
        let w = WatchFolder {
            folder: "/in".into(),
            op: "image.compress".into(),
            ..WatchFolder::default()
        };
        assert!(w.wants(Path::new("/in/photo.jpg")));
        assert!(
            !w.wants(Path::new("/in/Metamorphosis/photo.jpg")),
            "outputs"
        );
        assert!(!w.wants(Path::new("/in/sub/photo.jpg")), "subfolders");
        assert!(!w.wants(Path::new("/in/.morph-tmp-1")), "hidden / temp");
        assert!(!w.wants(Path::new("/in/movie.mp4.crdownload")), "downloads");
        assert!(!w.wants(Path::new("/elsewhere/a.jpg")));
        let bad = WatchFolder {
            out: "../..".into(),
            ..w.clone()
        };
        assert!(bad.out_dir().starts_with("/in"));
    }

    #[test]
    fn layout_keys() {
        assert_eq!(layout_key(Ring::Tools, Family::Pdf), "tools:pdf");
    }
}
