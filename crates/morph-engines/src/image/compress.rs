//! Compress images by target size, percentage or preset, optionally resizing.

use super::encode::{encode_to_vec, EncodeOptions};
use anyhow::Result;
use image::{imageops::FilterType, DynamicImage, GenericImageView};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    /// At most this many bytes.
    Target(u64),
    /// A fixed quality (presets).
    Quality(u8),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Resize {
    pub max_width: u32,
    pub max_height: u32,
    /// 1–100; 100 = unchanged.
    pub scale: u32,
}

/// Apply max-size / scale limits, keeping the aspect ratio.
pub fn resize(img: DynamicImage, r: Resize) -> DynamicImage {
    let (w, h) = img.dimensions();
    let mut f = 1.0f64;
    if r.scale > 0 && r.scale < 100 {
        f = r.scale as f64 / 100.0;
    }
    if r.max_width > 0 && (w as f64 * f) > r.max_width as f64 {
        f = r.max_width as f64 / w as f64;
    }
    if r.max_height > 0 && (h as f64 * f) > r.max_height as f64 {
        f = f.min(r.max_height as f64 / h as f64);
    }
    if f >= 0.999 {
        return img;
    }
    let (nw, nh) = (
        ((w as f64 * f).round() as u32).max(1),
        ((h as f64 * f).round() as u32).max(1),
    );
    img.resize_exact(nw, nh, FilterType::Lanczos3)
}

/// Lossless PNG optimisation (oxipng, no zopfli: fast enough for interactive use).
pub fn optimize_png(data: &[u8]) -> Vec<u8> {
    let mut opts = oxipng::Options::from_preset(3);
    opts.strip = oxipng::StripChunks::Safe;
    oxipng::optimize_from_memory(data, &opts).unwrap_or_else(|_| data.to_vec())
}

fn encode(img: &DynamicImage, ext: &str, q: u8) -> Result<Vec<u8>> {
    let bytes = encode_to_vec(
        img,
        ext,
        &EncodeOptions {
            quality: q,
            ..Default::default()
        },
    )?;
    Ok(if ext == "png" {
        optimize_png(&bytes)
    } else {
        bytes
    })
}

/// Compress `img` as `ext` towards `goal`. Returns the encoded bytes and the
/// quality used (100 for lossless PNG).
pub fn compress(
    img: DynamicImage,
    ext: &str,
    goal: Goal,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<u8>, u8)> {
    let lossy = matches!(ext, "jpg" | "webp" | "avif");
    match goal {
        Goal::Quality(q) => Ok((encode(&img, ext, if lossy { q } else { 100 })?, q)),
        Goal::Target(target) => {
            let mut img = img;
            for _round in 0..8 {
                if cancel() {
                    anyhow::bail!(morph_core::Error::Cancelled);
                }
                if !lossy {
                    let bytes = encode(&img, ext, 100)?;
                    if bytes.len() as u64 <= target {
                        return Ok((bytes, 100));
                    }
                    img = shrink(img, bytes.len() as u64, target);
                    continue;
                }
                // Highest quality that fits, by bisection over 5–95.
                let (mut lo, mut hi) = (5u8, 95u8);
                let mut best: Option<(Vec<u8>, u8)> = None;
                let mut smallest = u64::MAX;
                while lo <= hi {
                    if cancel() {
                        anyhow::bail!(morph_core::Error::Cancelled);
                    }
                    let q = lo + (hi - lo) / 2;
                    let bytes = encode(&img, ext, q)?;
                    smallest = smallest.min(bytes.len() as u64);
                    if bytes.len() as u64 <= target {
                        best = Some((bytes, q));
                        lo = q + 1;
                    } else {
                        if q == 0 {
                            break;
                        }
                        hi = q - 1;
                    }
                }
                if let Some(b) = best {
                    return Ok(b);
                }
                // Even the lowest quality is too big: shrink the pixels and retry.
                img = shrink(img, smallest, target);
            }
            anyhow::bail!("couldn't reach the target size; try a larger target")
        }
    }
}

fn shrink(img: DynamicImage, size: u64, target: u64) -> DynamicImage {
    let f = ((target as f64 / size as f64).sqrt() * 0.92).clamp(0.2, 0.95);
    let (w, h) = img.dimensions();
    img.resize_exact(
        ((w as f64 * f) as u32).max(1),
        ((h as f64 * f) as u32).max(1),
        FilterType::Lanczos3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy(w: u32, h: u32) -> DynamicImage {
        let mut s = 12345u32;
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            image::Rgb([(x % 256) as u8, (y % 256) as u8, (s >> 24) as u8])
        }))
    }

    #[test]
    fn target_size_is_met() {
        let img = noisy(800, 600);
        let (bytes, q) = compress(img, "jpg", Goal::Target(60_000), &|| false).unwrap();
        assert!(bytes.len() <= 60_000, "{} bytes at q{q}", bytes.len());
    }

    #[test]
    fn tiny_target_shrinks_pixels() {
        let img = noisy(800, 600);
        let (bytes, _) = compress(img, "jpg", Goal::Target(8_000), &|| false).unwrap();
        assert!(bytes.len() <= 8_000);
    }

    #[test]
    fn resize_limits() {
        let img = noisy(400, 200);
        assert_eq!(
            resize(
                img.clone(),
                Resize {
                    max_width: 100,
                    ..Default::default()
                }
            )
            .dimensions(),
            (100, 50)
        );
        assert_eq!(
            resize(
                img.clone(),
                Resize {
                    scale: 50,
                    ..Default::default()
                }
            )
            .dimensions(),
            (200, 100)
        );
        assert_eq!(resize(img, Resize::default()).dimensions(), (400, 200));
    }
}
