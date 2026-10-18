//! Windows: put files on the clipboard as CF_HDROP with a "copy" drop effect,
//! exactly like Explorer's Ctrl+C, so pasting creates real copies.

use anyhow::{bail, Context, Result};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_HDROP;

#[repr(C)]
struct DropFiles {
    p_files: u32,
    pt: [i32; 2],
    f_nc: i32,
    f_wide: i32,
}

fn global(bytes: &[u8]) -> Result<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            bail!("GlobalLock failed");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        Ok(h)
    }
}

pub fn copy_files(paths: &[PathBuf]) -> Result<()> {
    // DROPFILES header, then NUL-separated wide paths, double-NUL terminated.
    let header = DropFiles {
        p_files: std::mem::size_of::<DropFiles>() as u32,
        pt: [0, 0],
        f_nc: 0,
        f_wide: 1,
    };
    let mut bytes: Vec<u8> = unsafe {
        std::slice::from_raw_parts(
            &header as *const DropFiles as *const u8,
            std::mem::size_of::<DropFiles>(),
        )
        .to_vec()
    };
    for p in paths {
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        let s = abs
            .as_os_str()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string();
        for u in std::ffi::OsStr::new(&s).encode_wide().chain(Some(0)) {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&[0, 0]);

    unsafe {
        OpenClipboard(None).context("opening the clipboard")?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let h = global(&bytes)?;
            SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(h.0)))?;
            // DROPEFFECT_COPY (1): paste copies instead of moving.
            let effect = RegisterClipboardFormatW(w!("Preferred DropEffect"));
            let h = global(&1u32.to_le_bytes())?;
            SetClipboardData(effect, Some(HANDLE(h.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Read file paths currently on the clipboard (tests and "paste as source").
pub fn read_files() -> Result<Vec<PathBuf>> {
    use windows::Win32::System::DataExchange::GetClipboardData;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    unsafe {
        OpenClipboard(None)?;
        let r = (|| -> Result<Vec<PathBuf>> {
            let h = GetClipboardData(CF_HDROP.0 as u32)?;
            let hdrop = HDROP(h.0);
            let n = DragQueryFileW(hdrop, u32::MAX, None);
            let mut out = Vec::new();
            for i in 0..n {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                out.push(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
            }
            Ok(out)
        })();
        let _ = CloseClipboard();
        r
    }
}
