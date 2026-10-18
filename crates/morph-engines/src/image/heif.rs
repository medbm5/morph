//! HEIC / HEIF / AVIF input.
//!
//! HEVC is patent-encumbered and libheif is LGPL, so neither is linked in:
//! - **Windows** uses the system's WIC codecs (HEIF Image Extensions + HEVC Video
//!   Extensions for HEIC; AV1 Video Extension, preinstalled on Windows 11, for AVIF).
//! - **Linux** runs libheif's `heif-dec` (or older `heif-convert`) as a separate
//!   process, from the optional `heif` component or the system package.

use anyhow::{bail, Context, Result};
use image::DynamicImage;
use std::path::Path;

/// Is a HEIF decoder available on this machine?
pub fn available() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        sidecar().is_some()
    }
}

pub fn decode(path: &Path) -> Result<DynamicImage> {
    #[cfg(windows)]
    {
        match wic::decode(path) {
            Ok(img) => return Ok(img),
            Err(e) if sidecar().is_none() => {
                return Err(e).context(
                    "Windows couldn't decode this file; install \"HEIF Image Extensions\" and \
                     \"HEVC Video Extensions\" (HEIC) or \"AV1 Video Extension\" (AVIF) from the \
                     Microsoft Store, or the Metamorphosis `heif` component",
                )
            }
            Err(e) => log::debug!("WIC failed ({e:#}); trying heif-dec"),
        }
    }
    decode_with_sidecar(path)
}

fn sidecar() -> Option<(std::path::PathBuf, bool)> {
    morph_core::components::find_tool("heif", "heif-dec")
        .map(|p| (p, true))
        .or_else(|| morph_core::components::find_tool("heif", "heif-convert").map(|p| (p, false)))
}

fn decode_with_sidecar(path: &Path) -> Result<DynamicImage> {
    let Some((tool, is_dec)) = sidecar() else {
        bail!("HEIC input needs the optional `heif` component (`morph component install heif`) or libheif's heif-dec")
    };
    let tmp = tempfile_dir()?;
    let out = tmp.join("decoded.png");
    let mut cmd = std::process::Command::new(&tool);
    if is_dec {
        cmd.arg(path).arg("-o").arg(&out);
    } else {
        cmd.arg(path).arg(&out);
    }
    let res = cmd.output().with_context(|| format!("running {}", tool.display()))?;
    if !res.status.success() || !out.exists() {
        bail!("{} failed: {}", tool.display(), String::from_utf8_lossy(&res.stderr).trim());
    }
    // heif-dec applies the HEIF transform properties (rotation/mirror) itself.
    let img = image::open(&out).context("reading heif-dec output")?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(img)
}

fn tempfile_dir() -> Result<std::path::PathBuf> {
    let d = std::env::temp_dir().join(format!("morph-heif-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(windows)]
mod wic {
    use anyhow::{Context, Result};
    use image::{DynamicImage, RgbaImage};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::*;

    /// Decode any file WIC understands into RGBA8, honouring the orientation tag.
    pub fn decode(path: &Path) -> Result<DynamicImage> {
        unsafe {
            // S_FALSE (already initialised) is fine; RPC_E_CHANGED_MODE means an STA
            // owns this thread, which still lets us create in-proc WIC objects.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .context("creating WIC factory")?;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let decoder = factory
                .CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .context("no WIC codec for this file")?;
            let frame = decoder.GetFrame(0).context("reading first frame")?;
            let orientation = read_orientation(&frame).unwrap_or(1);
            let source: IWICBitmapSource = frame.into();
            let converted = WICConvertBitmapSource(&GUID_WICPixelFormat32bppRGBA as *const GUID, &source)
                .context("converting to RGBA")?;
            let (mut w, mut h) = (0u32, 0u32);
            converted.GetSize(&mut w, &mut h)?;
            let stride = w * 4;
            let mut buf = vec![0u8; (stride * h) as usize];
            converted.CopyPixels(std::ptr::null(), stride, &mut buf).context("copying pixels")?;
            let img = RgbaImage::from_raw(w, h, buf).context("pixel buffer")?;
            let mut img = DynamicImage::ImageRgba8(img);
            if let Some(o) = image::metadata::Orientation::from_exif(orientation) {
                img.apply_orientation(o);
            }
            Ok(img)
        }
    }

    unsafe fn read_orientation(frame: &IWICBitmapFrameDecode) -> Option<u8> {
        let reader = frame.GetMetadataQueryReader().ok()?;
        let name: Vec<u16> = "System.Photo.Orientation".encode_utf16().chain(Some(0)).collect();
        let mut value = windows::Win32::System::Com::StructuredStorage::PROPVARIANT::default();
        reader.GetMetadataByName(PCWSTR(name.as_ptr()), &mut value).ok()?;
        let v = u16::try_from(&value).ok()?;
        u8::try_from(v).ok()
    }
}
