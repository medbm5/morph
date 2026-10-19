//! The Actions ring: Share, Float, Move to…, Copy path, Open in terminal.
//! Works on files, folders and mixed selections. These run in the app process; they
//! never write next to the source and aren't recorded for Undo.

use anyhow::{bail, Context, Result};
use morph_core::{Arity, OpContext, OpSpec, Operation, OutputKind, Registry};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const ANY: &[&str] = &["*", "folder"];

/// App-side handlers for actions that need a window (Share needs one on Windows,
/// Float and Move need the UI). The CLI leaves them unset.
pub type Hook = Arc<dyn Fn(&[PathBuf]) -> Result<()> + Send + Sync>;
pub type MoveHook = Arc<dyn Fn(&[PathBuf]) -> Result<Vec<PathBuf>> + Send + Sync>;

#[derive(Default, Clone)]
pub struct Hooks {
    pub share: Option<Hook>,
    pub float: Option<Hook>,
    pub move_to: Option<MoveHook>,
}

pub fn register(reg: &mut Registry, hooks: Hooks) {
    let spec = |id: &str, label: &str, order: i32| {
        let mut s = OpSpec::action(id, label, ANY)
            .arity(Arity::All)
            .order(order);
        s.output = OutputKind::None;
        s
    };
    reg.register(Action {
        spec: spec("action.share", "Share", 1),
        kind: Kind::Share,
        hooks: hooks.clone(),
    });
    reg.register(Action {
        spec: spec("action.float", "Float", 2),
        kind: Kind::Float,
        hooks: hooks.clone(),
    });
    reg.register(Action {
        spec: spec("action.move", "Move", 4),
        kind: Kind::Move,
        hooks: hooks.clone(),
    });
    reg.register(Action {
        spec: spec("action.copy-path", "Copy path", 5),
        kind: Kind::CopyPath,
        hooks: hooks.clone(),
    });
    reg.register(Action {
        spec: spec("action.terminal", "Terminal", 6),
        kind: Kind::Terminal,
        hooks,
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Share,
    Float,
    Move,
    CopyPath,
    Terminal,
}

struct Action {
    spec: OpSpec,
    kind: Kind,
    hooks: Hooks,
}

impl Operation for Action {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        match self.kind {
            Kind::Float => self.hooks.float.is_some(),
            Kind::Move => self.hooks.move_to.is_some(),
            Kind::Share => self.hooks.share.is_some() || cfg!(target_os = "linux"),
            _ => true,
        }
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let paths = &ctx.inputs;
        match self.kind {
            Kind::CopyPath => copy_text(&paths_text(paths))?,
            Kind::Terminal => open_terminal(&terminal_dir(paths))?,
            Kind::Share => match &self.hooks.share {
                Some(f) => f(paths)?,
                None => share_linux(paths)?,
            },
            Kind::Float => (self.hooks.float.as_ref().context("Float needs the app")?)(paths)?,
            Kind::Move => {
                return (self.hooks.move_to.as_ref().context("Move needs the app")?)(paths)
            }
        }
        Ok(vec![])
    }
}

/// One path per line (Copy path).
pub fn paths_text(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The folder a terminal should open in: the folder itself, or a file's parent.
pub fn terminal_dir(paths: &[PathBuf]) -> PathBuf {
    let p = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
    if p.is_dir() {
        p
    } else {
        p.parent().map(Path::to_path_buf).unwrap_or(p)
    }
}

/// Plain text to the clipboard.
#[cfg(not(target_os = "linux"))]
pub fn copy_text(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().context("opening the clipboard")?;
    cb.set_text(text.to_string())
        .context("writing the clipboard")
}

/// Plain text to the clipboard, served from a background thread so it outlives
/// short-lived callers (X11 selections belong to a live client).
#[cfg(target_os = "linux")]
pub fn copy_text(text: &str) -> Result<()> {
    use arboard::SetExtLinux;
    let text = text.to_string();
    std::thread::spawn(move || {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set().wait().text(text);
        }
    });
    Ok(())
}

/// Files to the clipboard so the file manager can paste real copies.
pub fn copy_files(paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        bail!("nothing to copy");
    }
    #[cfg(windows)]
    {
        crate::win_clip::copy_files(paths)
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_clip::copy_files(paths)
    }
}

/// Open a terminal in `dir`.
pub fn open_terminal(dir: &Path) -> Result<()> {
    use std::process::Command;
    #[cfg(windows)]
    {
        if crate_find("wt.exe").is_some() {
            Command::new("wt.exe")
                .arg("-d")
                .arg(dir)
                .spawn()
                .context("starting Windows Terminal")?;
        } else {
            Command::new("cmd.exe")
                .args(["/C", "start", "cmd.exe", "/K", "cd", "/d"])
                .arg(dir)
                .spawn()
                .context("starting cmd")?;
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let d = dir.to_string_lossy().to_string();
        let candidates: Vec<(&str, Vec<String>)> = vec![
            ("x-terminal-emulator", vec![]),
            ("gnome-terminal", vec![format!("--working-directory={d}")]),
            ("konsole", vec!["--workdir".into(), d.clone()]),
            ("xfce4-terminal", vec![format!("--working-directory={d}")]),
            ("tilix", vec![format!("--working-directory={d}")]),
            ("kitty", vec!["--directory".into(), d.clone()]),
            ("alacritty", vec!["--working-directory".into(), d.clone()]),
            ("wezterm", vec!["start".into(), "--cwd".into(), d.clone()]),
            ("xterm", vec![]),
        ];
        // Respect the desktop's choice first.
        let preferred = std::env::var("TERMINAL").ok();
        let mut order: Vec<&(&str, Vec<String>)> = candidates.iter().collect();
        if let Some(p) = &preferred {
            order.sort_by_key(|(n, _)| *n != p.as_str());
        }
        for (name, args) in order {
            if morph_core::components::find_on_path(name).is_some() {
                Command::new(name)
                    .args(args)
                    .current_dir(dir)
                    .spawn()
                    .with_context(|| format!("starting {name}"))?;
                return Ok(());
            }
        }
        bail!("no terminal found (set $TERMINAL)")
    }
}

#[cfg(windows)]
fn crate_find(exe: &str) -> Option<PathBuf> {
    morph_core::components::find_on_path(exe).or_else(|| {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let p = PathBuf::from(local)
            .join("Microsoft")
            .join("WindowsApps")
            .join(exe);
        p.exists().then_some(p)
    })
}

/// Linux sharing: KDE Connect to the first reachable device, else LocalSend.
pub fn share_linux(paths: &[PathBuf]) -> Result<()> {
    use std::process::Command;
    if morph_core::components::find_on_path("kdeconnect-cli").is_some() {
        let out = Command::new("kdeconnect-cli")
            .args(["-a", "--id-only"])
            .output()?;
        if let Some(dev) = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            for p in paths {
                Command::new("kdeconnect-cli")
                    .args(["-d", dev, "--share"])
                    .arg(p)
                    .status()?;
            }
            return Ok(());
        }
    }
    for exe in ["localsend", "localsend_app"] {
        if morph_core::components::find_on_path(exe).is_some() {
            Command::new(exe).args(paths).spawn()?;
            return Ok(());
        }
    }
    bail!("install KDE Connect (with a paired device) or LocalSend to share")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let d = std::env::temp_dir();
        assert_eq!(terminal_dir(std::slice::from_ref(&d)), d);
        assert_eq!(terminal_dir(&[d.join("x.txt")]), d);
        assert_eq!(
            paths_text(&[PathBuf::from("a"), PathBuf::from("b")]),
            "a\nb"
        );
    }
}

/// Clipboard round trip. Overwrites the clipboard, so it only runs when
/// MORPH_TEST_CLIPBOARD is set (CI).
#[cfg(all(test, windows))]
mod clipboard_tests {
    #[test]
    fn copies_files_as_files() {
        if std::env::var_os("MORPH_TEST_CLIPBOARD").is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.txt");
        std::fs::write(&a, "x").unwrap();
        super::copy_files(&[a.clone(), d.path().to_path_buf()]).unwrap();
        let got = crate::win_clip::read_files().unwrap();
        assert_eq!(got.len(), 2);
        assert!(got[0].ends_with("a.txt"));
    }
}
