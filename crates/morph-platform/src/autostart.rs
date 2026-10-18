//! Start Metamorphosis in the tray at login: the per-user `Run` key on Windows,
//! an XDG autostart entry on Linux. Both start the app with `--background`.

use anyhow::Result;
use std::path::Path;

const NAME: &str = "Metamorphosis";

/// Turn autostart on or off for `app_exe`.
pub fn set(app_exe: &Path, on: bool) -> Result<()> {
    imp::set(NAME, app_exe, on)
}

pub fn is_enabled() -> bool {
    imp::is_enabled(NAME)
}

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use std::path::Path;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::*;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn open() -> Result<HKEY> {
        let mut key = HKEY::default();
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(RUN),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()
            .context("opening the Run key")?;
        }
        Ok(key)
    }

    pub fn set(name: &str, exe: &Path, on: bool) -> Result<()> {
        let key = open()?;
        let name = HSTRING::from(name);
        let r = unsafe {
            if on {
                let cmd = format!("\"{}\" --background", exe.display());
                let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
                let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)).ok()
            } else {
                match RegDeleteValueW(key, PCWSTR(name.as_ptr())) {
                    e if e == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND => Ok(()),
                    e => e.ok(),
                }
            }
        };
        unsafe {
            let _ = RegCloseKey(key);
        }
        r.context("writing the Run key")
    }

    pub fn is_enabled(name: &str) -> bool {
        let Ok(key) = open() else { return false };
        let name = HSTRING::from(name);
        let found =
            unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, None).is_ok() };
        unsafe {
            let _ = RegCloseKey(key);
        }
        found
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use anyhow::Result;
    use std::path::{Path, PathBuf};

    fn entry() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_default()
            .join("autostart/metamorphosis.desktop")
    }

    pub fn set(name: &str, exe: &Path, on: bool) -> Result<()> {
        let p = entry();
        if !on {
            match std::fs::remove_file(&p) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => return Ok(()),
            }
        }
        let exe = crate::shell_menu::launcher(exe);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(
            &p,
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec=\"{}\" --background\nIcon=metamorphosis\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
                exe.display()
            ),
        )?;
        Ok(())
    }

    pub fn is_enabled(_name: &str) -> bool {
        entry().exists()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn toggles() {
        // A throwaway entry name, so the real one is never touched.
        let name = "MetamorphosisAutostartTest";
        let exe = std::env::current_exe().unwrap();
        #[cfg(target_os = "linux")]
        {
            let d = tempfile::tempdir().unwrap();
            std::env::set_var("XDG_CONFIG_HOME", d.path());
            super::imp::set(name, &exe, true).unwrap();
            assert!(super::imp::is_enabled(name));
            super::imp::set(name, &exe, false).unwrap();
            assert!(!super::imp::is_enabled(name));
        }
        #[cfg(windows)]
        {
            super::imp::set(name, &exe, true).unwrap();
            assert!(super::imp::is_enabled(name));
            super::imp::set(name, &exe, false).unwrap();
            assert!(!super::imp::is_enabled(name));
            super::imp::set(name, &exe, false).unwrap();
        }
    }
}
