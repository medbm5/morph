//! Encode pixels to each output format.

use anyhow::{bail, Context, Result};
use image::codecs::avif::AvifEncoder;
use image::codecs::bmp::BmpEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::tiff::TiffEncoder;
use image::{DynamicImage, ImageEncoder, Rgb, RgbImage};
use std::io::{BufWriter, Write};
use std::path::Path;

/// Formats we can write.
pub const OUTPUTS: &[&str] = &["jpg", "png", "webp", "tiff", "avif", "bmp", "gif"];

#[derive(Debug, Clone, Copy)]
pub struct EncodeOptions {
    /// 1–100 for lossy formats.
    pub quality: u8,
    /// WebP: lossless instead of lossy.
    pub lossless: bool,
    /// AVIF encoder speed 1 (slow, small) – 10 (fast).
    pub avif_speed: u8,
    /// Background used when dropping alpha (JPG, BMP).
    pub matte: [u8; 3],
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            quality: 90,
            lossless: false,
            avif_speed: 6,
            matte: [255, 255, 255],
        }
    }
}

/// Encode `img` as `ext` into `path`.
pub fn encode(img: &DynamicImage, ext: &str, path: &Path, opts: &EncodeOptions) -> Result<()> {
    let bytes = encode_to_vec(img, ext, opts)?;
    let mut f = BufWriter::new(std::fs::File::create(path)?);
    f.write_all(&bytes)?;
    f.flush()?;
    Ok(())
}

/// Encode `img` as `ext` into memory (used by target-size compression).
pub fn encode_to_vec(img: &DynamicImage, ext: &str, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let q = opts.quality.clamp(1, 100);
    match ext {
        "jpg" | "jpeg" => {
            let rgb = flatten(img, opts.matte);
            JpegEncoder::new_with_quality(&mut out, q)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .context("encoding JPG")?;
        }
        "png" => {
            let img = normalize_depth(img);
            PngEncoder::new_with_quality(&mut out, CompressionType::Best, FilterType::Adaptive)
                .write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    img.color().into(),
                )
                .context("encoding PNG")?;
        }
        "webp" => {
            let rgba = img.to_rgba8();
            let has_alpha = rgba.pixels().any(|p| p[3] < 255);
            let enc = if has_alpha {
                webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height())
            } else {
                let rgb = img.to_rgb8();
                return encode_webp_rgb(&rgb, opts);
            };
            let mem = if opts.lossless {
                enc.encode_lossless()
            } else {
                enc.encode(q as f32)
            };
            out.extend_from_slice(&mem);
        }
        "tiff" | "tif" => {
            let img = normalize_depth(img);
            let mut cur = std::io::Cursor::new(&mut out);
            TiffEncoder::new(&mut cur)
                .write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    img.color().into(),
                )
                .context("encoding TIFF")?;
        }
        "avif" => {
            let rgba = img.to_rgba8();
            AvifEncoder::new_with_speed_quality(&mut out, opts.avif_speed.clamp(1, 10), q)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .context("encoding AVIF")?;
        }
        "bmp" => {
            let rgb = flatten(img, opts.matte);
            BmpEncoder::new(&mut out)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .context("encoding BMP")?;
        }
        "gif" => {
            let rgba = img.to_rgba8();
            let mut enc = image::codecs::gif::GifEncoder::new_with_speed(&mut out, 10);
            enc.encode(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .context("encoding GIF")?;
        }
        other => bail!("can't write .{other} images"),
    }
    Ok(out)
}

fn encode_webp_rgb(rgb: &RgbImage, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let enc = webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
    let mem = if opts.lossless {
        enc.encode_lossless()
    } else {
        enc.encode(opts.quality.clamp(1, 100) as f32)
    };
    Ok(mem.to_vec())
}

/// Composite onto a solid matte, dropping alpha.
pub fn flatten(img: &DynamicImage, matte: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p[3] as u32;
        let mix = |c: u8, m: u8| ((c as u32 * a + m as u32 * (255 - a) + 127) / 255) as u8;
        *o = Rgb([
            mix(p[0], matte[0]),
            mix(p[1], matte[1]),
            mix(p[2], matte[2]),
        ]);
    }
    out
}

/// PNG/TIFF can store 8- or 16-bit integer data; convert float images down.
fn normalize_depth(img: &DynamicImage) -> DynamicImage {
    match img {
        DynamicImage::ImageRgb32F(_) => DynamicImage::ImageRgb16(img.to_rgb16()),
        DynamicImage::ImageRgba32F(_) => DynamicImage::ImageRgba16(img.to_rgba16()),
        other => other.clone(),
    }
}
