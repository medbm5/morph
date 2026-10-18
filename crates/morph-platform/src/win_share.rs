//! Windows Share UI (the system share sheet) for files.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Mutex;
use windows::core::{factory, AgileReference, Interface, HSTRING};
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile, StorageFolder};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::IDataTransferManagerInterop;
use windows_collections::IIterable;

/// The handler registered for the last shared window (removed before re-registering).
static TOKEN: Mutex<Option<(isize, i64)>> = Mutex::new(None);

/// Resolve paths to storage items on a background (MTA) thread: blocking on WinRT
/// async operations from the UI thread would deadlock.
fn storage_items(paths: &[PathBuf]) -> Result<Vec<Option<IStorageItem>>> {
    let paths = paths.to_vec();
    let agile = std::thread::spawn(move || -> Result<Vec<AgileReference<IStorageItem>>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        paths
            .iter()
            .map(|p| -> Result<AgileReference<IStorageItem>> {
                let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                let s = HSTRING::from(abs.to_string_lossy().trim_start_matches(r"\\?\"));
                let item: IStorageItem = if abs.is_dir() {
                    StorageFolder::GetFolderFromPathAsync(&s)?.join()?.cast()?
                } else {
                    StorageFile::GetFileFromPathAsync(&s)?.join()?.cast()?
                };
                Ok(AgileReference::new(&item)?)
            })
            .collect()
    })
    .join()
    .map_err(|_| anyhow::anyhow!("share thread panicked"))??;
    agile
        .iter()
        .map(|a| a.resolve().map(Some).map_err(anyhow::Error::from))
        .collect()
}

/// Show the share sheet for `paths` over window `hwnd`. Call on that window's UI thread.
pub fn share(hwnd: isize, paths: &[PathBuf]) -> Result<()> {
    let items: IIterable<IStorageItem> = storage_items(paths)?.into();
    let items = AgileReference::new(&items)?;
    let count = paths.len();
    let interop =
        factory::<DataTransferManager, IDataTransferManagerInterop>().context("share interop")?;
    let h = HWND(hwnd as *mut _);
    let dtm: DataTransferManager =
        unsafe { interop.GetForWindow(h) }.context("DataTransferManager")?;
    if let Some((w, t)) = TOKEN.lock().unwrap().take() {
        if w == hwnd {
            let _ = dtm.RemoveDataRequested(t);
        }
    }
    let handler =
        TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                let data = args.Request()?.Data()?;
                let props = data.Properties()?;
                props.SetTitle(&HSTRING::from("Metamorphosis"))?;
                props.SetDescription(&HSTRING::from(format!("{count} item(s)")))?;
                data.SetStorageItemsReadOnly(&items.resolve()?)?;
            }
            Ok(())
        });
    let token = dtm.DataRequested(&handler)?;
    *TOKEN.lock().unwrap() = Some((hwnd, token));
    unsafe { interop.ShowShareUIForWindow(h) }.context("showing the share sheet")?;
    Ok(())
}
