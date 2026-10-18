//! The overlay as an OLE drop target.
//!
//! The ring window is created hidden and reused, so WebView2's child windows
//! don't exist when the webview installs its own drop handler. We register our
//! `IDropTarget` on the overlay and all of its children once it has been shown,
//! accept only file drags (CF_HDROP) and always answer COPY, so the drag source
//! never deletes or moves the original.

use crate::DropEvent;
use std::cell::Cell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::Arc;
use windows::core::{implement, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, POINTL};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP,
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;

pub type Callback = Arc<dyn Fn(DropEvent) + Send + Sync>;

#[implement(IDropTarget)]
struct Target {
    /// Positions are reported relative to this (top-level) window's client area.
    top: HWND,
    cb: Callback,
    files: Cell<bool>,
}

fn paths(data: &IDataObject) -> Option<Vec<PathBuf>> {
    let fmt = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    unsafe {
        let mut medium = data.GetData(&fmt).ok()?;
        let hdrop = HDROP(medium.u.hGlobal.0 as _);
        let n = DragQueryFileW(hdrop, u32::MAX, None);
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(OsString::from_wide(&buf[..len]).into());
        }
        ReleaseStgMedium(&mut medium);
        Some(out)
    }
}

impl Target_Impl {
    fn client(&self, pt: &POINTL) -> (i32, i32) {
        let mut p = POINT { x: pt.x, y: pt.y };
        unsafe {
            let _ = ScreenToClient(self.top, &mut p);
        }
        (p.x, p.y)
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: windows::core::Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let found = data.as_ref().and_then(paths);
        self.files
            .set(found.as_ref().is_some_and(|p| !p.is_empty()));
        let (x, y) = self.client(pt);
        (self.cb)(DropEvent::Enter {
            paths: found.unwrap_or_default(),
            x,
            y,
        });
        unsafe {
            *effect = if self.files.get() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            }
        };
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        if self.files.get() {
            let (x, y) = self.client(pt);
            (self.cb)(DropEvent::Over { x, y });
        }
        unsafe {
            *effect = if self.files.get() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            }
        };
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        (self.cb)(DropEvent::Leave);
        Ok(())
    }

    fn Drop(
        &self,
        data: windows::core::Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let (x, y) = self.client(pt);
        let found = data.as_ref().and_then(paths).unwrap_or_default();
        let ok = !found.is_empty();
        (self.cb)(DropEvent::Drop { paths: found, x, y });
        unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        Ok(())
    }
}

/// Register on `top` and all of its child windows. Must run on the window's UI thread.
pub fn register(top: HWND, cb: Callback) -> usize {
    let target: IDropTarget = Target {
        top,
        cb,
        files: Cell::new(false),
    }
    .into();
    let mut hwnds = vec![top];
    unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
        (*(l.0 as *mut Vec<HWND>)).push(h);
        BOOL(1)
    }
    unsafe {
        let _ = EnumChildWindows(
            Some(top),
            Some(collect),
            LPARAM(&mut hwnds as *mut _ as isize),
        );
    }
    let mut n = 0;
    for h in hwnds {
        unsafe {
            let _ = RevokeDragDrop(h);
            if RegisterDragDrop(h, &target).is_ok() {
                n += 1;
            }
        }
    }
    n
}
