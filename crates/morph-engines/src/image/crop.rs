//! Crop with ratio presets, an exact rectangle, and an optional exact output size.

use anyhow::{bail, Result};
use image::{imageops::FilterType, DynamicImage, GenericImageView};

/// Parse `1:1`, `16:9`, `4:5`… (`free` → None).
pub fn parse_ratio(s: &str) -> Result<Option<(f64, f64)>> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() || s == "free" || s == "none" || s == "original" {
        return Ok(None);
    }
    let Some((a, b)) = s.split_once([':', 'x', '/']) else {
        bail!("ratio `{s}` should look like 16:9")
    };
    let (a, b): (f64, f64) = (a.trim().parse()?, b.trim().parse()?);
    if a <= 0.0 || b <= 0.0 {
        bail!("ratio `{s}` must be positive");
    }
    Ok(Some((a, b)))
}

/// A crop rectangle in source pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// The largest centred rectangle of `ratio` inside (w, h).
pub fn centred(w: u32, h: u32, ratio: (f64, f64)) -> Rect {
    let want = ratio.0 / ratio.1;
    let (cw, ch) = if w as f64 / h as f64 > want {
        (((h as f64) * want).round() as u32, h)
    } else {
        (w, ((w as f64) / want).round() as u32)
    };
    let (cw, ch) = (cw.clamp(1, w), ch.clamp(1, h));
    Rect {
        x: (w - cw) / 2,
        y: (h - ch) / 2,
        w: cw,
        h: ch,
    }
}

/// Crop `img` to `rect` (clamped), then resize to exactly `out` if given.
pub fn crop(img: &DynamicImage, rect: Rect, out: Option<(u32, u32)>) -> Result<DynamicImage> {
    let (w, h) = img.dimensions();
    if rect.x >= w || rect.y >= h {
        bail!("crop starts outside the {w}×{h} image");
    }
    let rw = rect.w.min(w - rect.x).max(1);
    let rh = rect.h.min(h - rect.y).max(1);
    let cropped = img.crop_imm(rect.x, rect.y, rw, rh);
    Ok(match out {
        Some((ow, oh)) if ow > 0 && oh > 0 && (ow, oh) != (rw, rh) => {
            cropped.resize_exact(ow, oh, FilterType::Lanczos3)
        }
        _ => cropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios() {
        assert_eq!(parse_ratio("16:9").unwrap(), Some((16.0, 9.0)));
        assert_eq!(parse_ratio("free").unwrap(), None);
        assert!(parse_ratio("0:1").is_err());
        assert_eq!(
            centred(4000, 3000, (1.0, 1.0)),
            Rect {
                x: 500,
                y: 0,
                w: 3000,
                h: 3000
            }
        );
        assert_eq!(
            centred(1000, 2000, (4.0, 5.0)),
            Rect {
                x: 0,
                y: 375,
                w: 1000,
                h: 1250
            }
        );
    }

    #[test]
    fn exact_output() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::new(4000, 3000));
        let r = centred(4000, 3000, (1.0, 1.0));
        assert_eq!(
            crop(&img, r, Some((1080, 1080))).unwrap().dimensions(),
            (1080, 1080)
        );
        assert_eq!(
            crop(
                &img,
                Rect {
                    x: 10,
                    y: 20,
                    w: 300,
                    h: 200
                },
                None
            )
            .unwrap()
            .dimensions(),
            (300, 200)
        );
    }
}
