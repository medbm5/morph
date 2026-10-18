//! File-manager menu entries that open the ring in expanded mode:
//! Explorer (IExplorerCommand, per-user registration), Dolphin service menu,
//! Nautilus scripts, Nemo actions and Thunar custom actions.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub const RINGS: [(&str, &str); 3] = [
    ("convert", "Convert…"),
    ("tools", "Tools…"),
    ("actions", "Actions…"),
];

/// Install every integration available on this system. Returns what was installed.
pub fn install(app_exe: &Path) -> Result<Vec<String>> {
    #[cfg(windows)]
    {
        let dll = app_exe.with_file_name("morph_shell.dll");
        win::install(&dll)?;
        Ok(vec!["Explorer".into()])
    }
    #[cfg(target_os = "linux")]
    {
        linux::install(app_exe)
    }
}

pub fn uninstall() -> Result<()> {
    #[cfg(windows)]
    {
        win::uninstall()
    }
    #[cfg(target_os = "linux")]
    {
        linux::uninstall()
    }
}

/// The command to launch (AppImages relaunch through `$APPIMAGE`).
pub fn launcher(app_exe: &Path) -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_exe.to_path_buf())
}

#[cfg(windows)]
mod win {
    use anyhow::{Context, Result};
    use std::path::Path;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::*;
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    const CLSID: &str = "{6D6F7270-6873-4E52-A1C3-4D45544D4F52}";
    const KEYS: [&str; 3] = [
        r"Software\Classes\*\shell\Metamorphosis",
        r"Software\Classes\Directory\shell\Metamorphosis",
        r"Software\Classes\Directory\Background\shell\Metamorphosis",
    ];

    fn set(path: &str, name: Option<&str>, value: &str) -> Result<()> {
        unsafe {
            let mut key = HKEY::default();
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(path),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
            .ok()
            .with_context(|| format!("creating HKCU\\{path}"))?;
            let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let name = name.map(HSTRING::from);
            let r = RegSetValueExW(
                key,
                name.as_ref()
                    .map(|n| PCWSTR(n.as_ptr()))
                    .unwrap_or(PCWSTR::null()),
                None,
                REG_SZ,
                Some(bytes),
            );
            let _ = RegCloseKey(key);
            r.ok().context("writing registry value")?;
        }
        Ok(())
    }

    pub fn install(dll: &Path) -> Result<()> {
        let server = format!(r"Software\Classes\CLSID\{CLSID}\InprocServer32");
        set(
            &format!(r"Software\Classes\CLSID\{CLSID}"),
            None,
            "Metamorphosis context menu",
        )?;
        set(&server, None, &dll.display().to_string())?;
        set(&server, Some("ThreadingModel"), "Apartment")?;
        let icon = dll
            .with_file_name("metamorphosis.exe")
            .display()
            .to_string();
        for k in KEYS {
            set(k, Some("ExplorerCommandHandler"), CLSID)?;
            set(k, Some("MUIVerb"), "Metamorphosis")?;
            set(k, Some("Icon"), &icon)?;
        }
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        unsafe {
            for k in KEYS
                .iter()
                .copied()
                .chain([format!(r"Software\Classes\CLSID\{CLSID}").leak() as &str])
            {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(k));
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &HSTRING::from(k));
            }
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{launcher, RINGS};
    use anyhow::Result;
    use std::path::{Path, PathBuf};

    fn home() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
    }
    fn data() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local/share"))
    }
    fn config() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
    }

    fn executable(p: &Path, text: &str) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, text)?;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn quote(p: &Path) -> String {
        format!("'{}'", p.display().to_string().replace('\'', r"'\''"))
    }

    pub fn install(app_exe: &Path) -> Result<Vec<String>> {
        let exe = launcher(app_exe);
        let mut done = Vec::new();

        // Dolphin (KDE): a service menu with a submenu. Plasma 6 wants it executable.
        let mut desktop = String::from("[Desktop Entry]\nType=Service\nMimeType=all/all;inode/directory;\nX-KDE-Submenu=Metamorphosis\nX-KDE-Priority=TopLevel\nActions=");
        desktop.push_str(&RINGS.map(|(k, _)| k).join(";"));
        desktop.push_str(";\n");
        for (k, label) in RINGS {
            desktop.push_str(&format!(
                "\n[Desktop Action {k}]\nName={label}\nIcon=metamorphosis\nExec={} --ring {k} %F\n",
                quote(&exe)
            ));
        }
        executable(
            &data().join("kio/servicemenus/metamorphosis.desktop"),
            &desktop,
        )?;
        done.push("Dolphin".into());

        // Nautilus (GNOME): scripts under a Metamorphosis folder.
        for (k, label) in RINGS {
            let script = format!(
                "#!/bin/sh\n# Metamorphosis: open the {k} ring for the selection.\nprintf '%s' \"$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS\" | tr '\\n' '\\0' | xargs -0 {} --ring {k}\n",
                quote(&exe)
            );
            executable(
                &data()
                    .join("nautilus/scripts/Metamorphosis")
                    .join(label.trim_end_matches('…')),
                &script,
            )?;
        }
        done.push("Nautilus".into());

        // Nemo (Cinnamon).
        for (k, label) in RINGS {
            let action = format!(
                "[Nemo Action]\nName=Metamorphosis: {label}\nComment=Open the {k} ring\nExec={} --ring {k} %F\nIcon-Name=metamorphosis\nSelection=notnone\nExtensions=any;dir;\n",
                quote(&exe)
            );
            let p = data()
                .join("nemo/actions")
                .join(format!("metamorphosis-{k}.nemo_action"));
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(p, action)?;
        }
        done.push("Nemo".into());

        // Thunar (Xfce): custom actions in uca.xml, merged with the user's own.
        let uca = config().join("Thunar/uca.xml");
        let mut xml = std::fs::read_to_string(&uca).unwrap_or_else(|_| {
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into()
        });
        xml = strip_ours(&xml);
        let mut ours = String::new();
        for (k, label) in RINGS {
            ours.push_str(&format!(
                "<action><icon>metamorphosis</icon><name>Metamorphosis: {label}</name><unique-id>morph-{k}</unique-id><command>{} --ring {k} %F</command><description>Open the {k} ring</description><patterns>*</patterns><directories/><audio-files/><image-files/><other-files/><text-files/><video-files/></action>\n",
                quote(&exe).replace('&', "&amp;").replace('<', "&lt;")
            ));
        }
        if let Some(i) = xml.rfind("</actions>") {
            xml.insert_str(i, &ours);
            std::fs::create_dir_all(uca.parent().unwrap())?;
            std::fs::write(&uca, xml)?;
            done.push("Thunar".into());
        }
        Ok(done)
    }

    /// Remove our previous `<action>` entries from a Thunar uca.xml.
    fn strip_ours(xml: &str) -> String {
        let mut out = String::new();
        let mut rest = xml;
        while let Some(start) = rest.find("<action>") {
            let end = rest[start..]
                .find("</action>")
                .map(|e| start + e + "</action>".len())
                .unwrap_or(rest.len());
            let block = &rest[start..end];
            out.push_str(&rest[..start]);
            if !block.contains("<unique-id>morph-") {
                out.push_str(block);
            }
            rest = rest[end..].trim_start_matches('\n');
        }
        out.push_str(rest);
        out
    }

    pub fn uninstall() -> Result<()> {
        let _ = std::fs::remove_file(data().join("kio/servicemenus/metamorphosis.desktop"));
        let _ = std::fs::remove_dir_all(data().join("nautilus/scripts/Metamorphosis"));
        for (k, _) in RINGS {
            let _ = std::fs::remove_file(
                data()
                    .join("nemo/actions")
                    .join(format!("metamorphosis-{k}.nemo_action")),
            );
        }
        let uca = config().join("Thunar/uca.xml");
        if let Ok(x) = std::fs::read_to_string(&uca) {
            let _ = std::fs::write(&uca, strip_ours(&x));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn strips_only_our_thunar_actions() {
            let x = "<actions>\n<action><name>Mine</name><unique-id>1</unique-id></action>\n<action><unique-id>morph-convert</unique-id></action>\n</actions>\n";
            let s = super::strip_ours(x);
            assert!(s.contains("Mine") && !s.contains("morph-convert"));
        }
    }
}
