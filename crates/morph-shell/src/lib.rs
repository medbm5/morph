//! Explorer context menu for Metamorphosis: an `IExplorerCommand` in-process COM
//! server with a "Metamorphosis ▸ Convert / Tools / Actions" submenu. Each entry
//! launches `metamorphosis.exe --ring <ring> <selected paths…>`, which opens the ring
//! in expanded mode (a running instance receives the paths instead).
//!
//! Registered per user (HKCU, no admin) by the app; see `morph_platform::shell_menu`.

#![cfg(windows)]
#![allow(non_snake_case)]
// The MSVC linker notes that COM exports "should be PRIVATE"; harmless for us.
#![allow(linker_messages)]

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{implement, Interface, Ref, BOOL, GUID, HRESULT, PWSTR};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOTIMPL, E_POINTER, HINSTANCE, S_FALSE,
    S_OK,
};
use windows::Win32::System::Com::{IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::UI::Shell::*;

/// CLSID of the root command: {6D6F7270-6873-4E52-A1C3-4D45544D4F52}.
pub const CLSID_MORPH_MENU: GUID = GUID::from_u128(0x6D6F7270_6873_4E52_A1C3_4D45544D4F52);

static MODULE: AtomicIsize = AtomicIsize::new(0);

#[no_mangle]
extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(module.0 as isize, Ordering::Relaxed);
    }
    BOOL(1)
}

/// `metamorphosis.exe` next to this DLL.
fn app_exe() -> PathBuf {
    let mut buf = [0u16; 1024];
    let n = unsafe {
        GetModuleFileNameW(
            Some(HINSTANCE(MODULE.load(Ordering::Relaxed) as *mut _).into()),
            &mut buf,
        )
    } as usize;
    let dll = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    dll.with_file_name("metamorphosis.exe")
}

fn co_str(s: &str) -> windows::core::Result<PWSTR> {
    let wide: Vec<u16> = s.encode_utf16().chain(Some(0)).collect();
    unsafe { SHStrDupW(windows::core::PCWSTR(wide.as_ptr())) }
}

fn selected_paths(items: Ref<IShellItemArray>) -> Vec<String> {
    let Some(items) = items.as_ref() else {
        return vec![];
    };
    let mut out = Vec::new();
    unsafe {
        let n = items.GetCount().unwrap_or(0);
        for i in 0..n {
            if let Ok(item) = items.GetItemAt(i) {
                if let Ok(p) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                    if let Ok(s) = p.to_string() {
                        out.push(s);
                    }
                    windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
                }
            }
        }
    }
    out
}

/// Root command ("Metamorphosis") or one of its subcommands (`ring` = Some).
#[implement(IExplorerCommand)]
struct Command {
    ring: Option<&'static str>,
}

const SUBS: [(&str, &str); 3] = [
    ("convert", "Convert…"),
    ("tools", "Tools…"),
    ("actions", "Actions…"),
];

impl IExplorerCommand_Impl for Command_Impl {
    fn GetTitle(&self, _items: Ref<IShellItemArray>) -> windows::core::Result<PWSTR> {
        let title = match self.ring {
            None => "Metamorphosis",
            Some(r) => SUBS
                .iter()
                .find(|(k, _)| *k == r)
                .map(|(_, t)| *t)
                .unwrap_or(r),
        };
        co_str(title)
    }

    fn GetIcon(&self, _items: Ref<IShellItemArray>) -> windows::core::Result<PWSTR> {
        co_str(&format!("{},0", app_exe().display()))
    }

    fn GetToolTip(&self, _items: Ref<IShellItemArray>) -> windows::core::Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> windows::core::Result<GUID> {
        Ok(match self.ring {
            None => CLSID_MORPH_MENU,
            Some("convert") => GUID::from_u128(0x6D6F7270_6873_4E52_A1C3_4D45544D4F01),
            Some("tools") => GUID::from_u128(0x6D6F7270_6873_4E52_A1C3_4D45544D4F02),
            Some(_) => GUID::from_u128(0x6D6F7270_6873_4E52_A1C3_4D45544D4F03),
        })
    }

    fn GetState(
        &self,
        _items: Ref<IShellItemArray>,
        _ok_to_be_slow: BOOL,
    ) -> windows::core::Result<u32> {
        Ok(ECS_ENABLED.0 as u32)
    }

    fn Invoke(
        &self,
        items: Ref<IShellItemArray>,
        _ctx: Ref<IBindCtx>,
    ) -> windows::core::Result<()> {
        let ring = self.ring.unwrap_or("convert");
        let paths = selected_paths(items);
        let _ = std::process::Command::new(app_exe())
            .arg("--ring")
            .arg(ring)
            .args(paths)
            .spawn();
        Ok(())
    }

    fn GetFlags(&self) -> windows::core::Result<u32> {
        Ok(if self.ring.is_none() {
            ECF_HASSUBCOMMANDS.0 as u32
        } else {
            ECF_DEFAULT.0 as u32
        })
    }

    fn EnumSubCommands(&self) -> windows::core::Result<IEnumExplorerCommand> {
        if self.ring.is_some() {
            return Err(E_NOTIMPL.into());
        }
        Ok(Subcommands {
            next: std::cell::Cell::new(0),
        }
        .into())
    }
}

#[implement(IEnumExplorerCommand)]
struct Subcommands {
    next: std::cell::Cell<usize>,
}

impl IEnumExplorerCommand_Impl for Subcommands_Impl {
    fn Next(&self, celt: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        let mut n = 0u32;
        while n < celt && self.next.get() < SUBS.len() {
            let cmd: IExplorerCommand = Command {
                ring: Some(SUBS[self.next.get()].0),
            }
            .into();
            unsafe { out.add(n as usize).write(Some(cmd)) };
            self.next.set(self.next.get() + 1);
            n += 1;
        }
        if !fetched.is_null() {
            unsafe { *fetched = n };
        }
        if n == celt {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> windows::core::Result<()> {
        self.next
            .set((self.next.get() + celt as usize).min(SUBS.len()));
        Ok(())
    }

    fn Reset(&self) -> windows::core::Result<()> {
        self.next.set(0);
        Ok(())
    }

    fn Clone(&self) -> windows::core::Result<IEnumExplorerCommand> {
        Ok(Subcommands {
            next: std::cell::Cell::new(self.next.get()),
        }
        .into())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<windows::core::IUnknown>,
        riid: *const GUID,
        object: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let cmd: IExplorerCommand = Command { ring: None }.into();
        unsafe { cmd.query(riid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

#[no_mangle]
unsafe extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    riid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() || clsid.is_null() {
        return E_POINTER;
    }
    if *clsid != CLSID_MORPH_MENU {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    factory.query(riid, out)
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};

    fn take(p: PWSTR) -> String {
        let s = unsafe { p.to_string().unwrap() };
        unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
        s
    }

    #[test]
    fn menu_through_the_class_factory() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut factory: *mut c_void = std::ptr::null_mut();
            DllGetClassObject(&CLSID_MORPH_MENU, &IClassFactory::IID, &mut factory)
                .ok()
                .unwrap();
            let factory = IClassFactory::from_raw(factory);
            let root: IExplorerCommand = factory.CreateInstance(None).unwrap();
            assert_eq!(take(root.GetTitle(None).unwrap()), "Metamorphosis");
            assert_eq!(
                root.GetFlags().unwrap() & ECF_HASSUBCOMMANDS.0 as u32,
                ECF_HASSUBCOMMANDS.0 as u32
            );
            let subs = root.EnumSubCommands().unwrap();
            let mut got = vec![];
            loop {
                let mut item = [None];
                let mut n = 0;
                let _ = subs.Next(&mut item, Some(&mut n));
                if n == 0 {
                    break;
                }
                got.push(take(item[0].as_ref().unwrap().GetTitle(None).unwrap()));
            }
            assert_eq!(got, vec!["Convert…", "Tools…", "Actions…"]);
            // Unknown class: refused.
            let mut p: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                DllGetClassObject(&GUID::zeroed(), &IClassFactory::IID, &mut p),
                CLASS_E_CLASSNOTAVAILABLE
            );
        }
    }
}
