//! Decode any supported image into a `DynamicImage`, applying EXIF orientation.

use super::heif;
use anyhow::{bail, Context, Result};
use image::{DynamicImage, ImageDecoder, ImageReader};
use morph_core::Kind;
use std::path::Path;

/// Decode `path` (of detected `kind`) into pixels with orientation applied.
pub fn decode(path: &Path, kind: &Kind) -> Result<DynamicImage> {
    match kind.ext.as_str() {
        "svg" => decode_svg(path, None),
        "heic" | "avif" => heif::decode(path).or_else(|e| {
            // AVIF may still decode through `image` if it was built with a native AV1 decoder.
            if kind.ext == "avif" {
                decode_raster(path).map_err(|_| e)
            } else {
                Err(e)
            }
        }),
        _ => decode_raster(path),
    }
}

fn decode_raster(path: &Path) -> Result<DynamicImage> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .with_guessed_format()?;
    let mut decoder = reader.into_decoder().with_context(|| format!("reading {}", path.display()))?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).with_context(|| format!("decoding {}", path.display()))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

/// Rasterize an SVG. `width` overrides its intrinsic width (height keeps the ratio).
pub fn decode_svg(path: &Path, width: Option<u32>) -> Result<DynamicImage> {
    let data = std::fs::read(path)?;
    let mut opt = resvg::usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        ..Default::default()
    };
    // System fonts only: never fetched from the network.
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).context("parsing SVG")?;
    let size = tree.size();
    let scale = match width {
        Some(w) if w > 0 => w as f32 / size.width(),
        _ => 1.0,
    };
    let w = (size.width() * scale).ceil().max(1.0) as u32;
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    if w > 32_768 || h > 32_768 {
        bail!("SVG is too large to rasterize ({w}×{h})");
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).context("allocating SVG canvas")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied RGBA; un-premultiply for `image`.
    let mut rgba = pixmap.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let buf = image::RgbaImage::from_raw(w, h, rgba).context("SVG buffer")?;
    Ok(DynamicImage::ImageRgba8(buf))
}
