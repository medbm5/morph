//! Windows implementation.

use crate::overlay::Handle;
use crate::Rect;
use anyhow::{Context, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::core::BOOL;
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    CreateEllipticRgn, GetMonitorInfoW, MonitorFromPoint, SetWindowRgn, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(crate) fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn hwnd(h: Handle) -> HWND {
    HWND(h.0 as *mut _)
}

pub fn cursor() -> Option<(i32, i32)> {
    let mut pt = POINT::default();
    unsafe { GetCursorPos(&mut pt).ok()? };
    Some((pt.x, pt.y))
}

pub fn work_area_at(x: i32, y: i32) -> Option<Rect> {
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut info).as_bool() {
            return None;
        }
        let r = info.rcWork;
        Some(Rect {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        })
    }
}

pub fn reduced_motion() -> bool {
    let mut enabled = BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    !enabled.as_bool()
}

pub fn overlay_prepare(h: Handle) -> Result<()> {
    unsafe {
        let w = hwnd(h);
        let ex = GetWindowLongPtrW(w, GWL_EXSTYLE);
        let ex = (ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(w, GWL_EXSTYLE, ex);
    }
    Ok(())
}

pub fn overlay_show(h: Handle, r: Rect) -> Result<()> {
    unsafe {
        SetWindowPos(
            hwnd(h),
            Some(HWND_TOPMOST),
            r.x,
            r.y,
            r.w,
            r.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
        .context("positioning overlay")?;
    }
    Ok(())
}

pub fn overlay_hide(h: Handle) -> Result<()> {
    unsafe {
        let _ = ShowWindow(hwnd(h), SW_HIDE);
    }
    Ok(())
}

pub fn overlay_circle(h: Handle, size: i32, diameter: i32) -> Result<()> {
    unsafe {
        let off = (size - diameter) / 2;
        let rgn = CreateEllipticRgn(off, off, off + diameter, off + diameter);
        // The system owns the region after SetWindowRgn.
        SetWindowRgn(hwnd(h), Some(rgn), true);
    }
    Ok(())
}

pub fn reveal(path: &Path) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let w = wide(path.as_os_str());
        let pidl: *mut ITEMIDLIST = ILCreateFromPathW(PCWSTR(w.as_ptr()));
        if pidl.is_null() {
            anyhow::bail!("can't resolve {}", path.display());
        }
        let r = SHOpenFolderAndSelectItems(pidl, None, 0);
        ILFree(Some(pidl));
        r.context("opening Explorer")?;
    }
    Ok(())
}

/// Selected items of the Explorer window (active tab) or desktop under (x, y).
pub fn selection_at(x: i32, y: i32) -> Result<Vec<PathBuf>> {
    let under = unsafe { WindowFromPoint(POINT { x, y }) };
    if under.is_invalid() {
        return Ok(vec![]);
    }
    selection_of_window(unsafe { GetAncestor(under, GA_ROOT) })
}

/// Selected items of the foreground Explorer window (keyboard hotkey).
pub fn selection_foreground() -> Result<Vec<PathBuf>> {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() {
        return Ok(vec![]);
    }
    selection_of_window(fg)
}

fn selection_of_window(root: HWND) -> Result<Vec<PathBuf>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let class = class_name(root);
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;

        if class == "Progman" || class == "WorkerW" {
            let mut found = 0i32;
            let disp = windows.FindWindowSW(
                &VARIANT::from(CSIDL_DESKTOP as i32),
                &VARIANT::default(),
                SWC_DESKTOP,
                &mut found,
                SWFO_NEEDDISPATCH,
            )?;
            let sp: IServiceProvider = disp.cast()?;
            let browser: IShellBrowser = sp.QueryService(&SID_STopLevelBrowser)?;
            return selection_of(&browser);
        }
        if class != "CabinetWClass" && class != "ExploreWClass" {
            return Ok(vec![]);
        }

        let count = windows.Count()?;
        for i in 0..count {
            let Ok(disp) = windows.Item(&VARIANT::from(i)) else {
                continue;
            };
            let Ok(app) = disp.cast::<IWebBrowserApp>() else {
                continue;
            };
            let Ok(h) = app.HWND() else { continue };
            if h.0 != root.0 as isize {
                continue;
            }
            let Ok(sp) = disp.cast::<IServiceProvider>() else {
                continue;
            };
            let Ok(browser) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else {
                continue;
            };
            // Windows 11 tabs share one top-level window: take the visible tab.
            if let Ok(tab) = browser.GetWindow() {
                if !IsWindowVisible(tab).as_bool() {
                    continue;
                }
            }
            return selection_of(&browser);
        }
        Ok(vec![])
    }
}

unsafe fn selection_of(browser: &IShellBrowser) -> Result<Vec<PathBuf>> {
    let view = browser.QueryActiveShellView()?;
    let fv: IFolderView2 = view.cast()?;
    let items = match fv.GetSelection(false) {
        Ok(a) => a,
        Err(_) => return Ok(vec![]),
    };
    let n = items.GetCount()?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let item = items.GetItemAt(i)?;
        if let Ok(p) = item.GetDisplayName(SIGDN_FILESYSPATH) {
            let s = p.to_string();
            CoTaskMemFree(Some(p.0 as *const _));
            if let Ok(s) = s {
                out.push(PathBuf::from(s));
            }
        }
    }
    Ok(out)
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
