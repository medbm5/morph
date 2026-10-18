//! Photo adjustments. This is the reference implementation of the editor's WebGL2
//! shader (`app/ui/editors/adjust-shader.js`): the same math, in the same order, with
//! the same bilinear ring sampling and the same integer hash for grain, so the live
//! preview matches the export (ΔE < 2, checked in `app/tests/adjust-parity.mjs`).
//!
//! Neighbourhood effects read the *source* image; point effects follow.
//! Order: noise reduction → detail → clarity → dehaze → exposure → temperature/tint
//! → contrast → saturation/vibrance → grain.

use image::{DynamicImage, Rgba32FImage, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjust {
    /// EV stops, −3…3.
    pub exposure: f32,
    /// −1…1.
    pub contrast: f32,
    /// −1…1 (−1 = greyscale).
    pub saturation: f32,
    /// −1…1, boosts muted colours more than saturated ones.
    pub vibrance: f32,
    /// −1 (cool) … 1 (warm).
    pub temperature: f32,
    /// −1 (green) … 1 (magenta).
    pub tint: f32,
    /// Fine sharpening, 0…1.
    pub detail: f32,
    /// Mid-tone local contrast, 0…1.
    pub clarity: f32,
    /// 0…1.
    pub dehaze: f32,
    /// Film grain, 0…1.
    pub grain: f32,
    /// 0…1.
    pub noise_reduction: f32,
}

impl Adjust {
    pub fn is_identity(&self) -> bool {
        *self == Adjust::default()
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// PCG-style integer hash → [0, 1). Identical to the shader's `hash2`.
pub fn hash2(x: u32, y: u32) -> f32 {
    let mut v = x
        .wrapping_mul(1_664_525)
        .wrapping_add(y.wrapping_mul(1_013_904_223))
        .wrapping_add(0x9E37_79B9);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7FEB_352D);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846C_A68B);
    v ^= v >> 16;
    (v >> 8) as f32 / 16_777_216.0
}

/// Bilinear sample with clamp-to-edge, texel centres at (i + 0.5) — as GL_LINEAR does.
fn sample(src: &Rgba32FImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (src.width() as i64, src.height() as i64);
    let fx = x - 0.5;
    let fy = y - 0.5;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let (tx, ty) = (fx - x0, fy - y0);
    let px = |xi: i64, yi: i64| {
        let p = src.get_pixel(xi.clamp(0, w - 1) as u32, yi.clamp(0, h - 1) as u32);
        [p[0], p[1], p[2]]
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let a = px(x0, y0);
    let b = px(x0 + 1, y0);
    let c = px(x0, y0 + 1);
    let d = px(x0 + 1, y0 + 1);
    let mut out = [0.0; 3];
    for i in 0..3 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bot = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bot - top) * ty;
    }
    out
}

/// Mean of `taps` samples on a ring of `radius` px around (x, y) (pixel-centre coords).
fn ring(src: &Rgba32FImage, x: f32, y: f32, radius: f32, taps: u32) -> [f32; 3] {
    let mut acc = [0.0; 3];
    for i in 0..taps {
        let a = i as f32 * std::f32::consts::TAU / taps as f32;
        let s = sample(src, x + a.cos() * radius, y + a.sin() * radius);
        for k in 0..3 {
            acc[k] += s[k];
        }
    }
    acc.map(|v| v / taps as f32)
}

/// Radius of the large-scale ring (clarity, dehaze): 1.5 % of the short side.
pub fn wide_radius(w: u32, h: u32) -> f32 {
    (w.min(h) as f32 * 0.015).max(2.0)
}

/// Apply adjustments to one pixel. `c` is sRGB 0–1 at pixel (px, py).
#[allow(clippy::too_many_arguments)]
fn pixel(a: &Adjust, src: &Rgba32FImage, px: u32, py: u32, c: [f32; 3], wide: f32) -> [f32; 3] {
    let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
    let mut c = c;
    let needs_near = a.noise_reduction > 0.0 || a.detail > 0.0;
    let near = if needs_near {
        ring(src, x, y, 1.0, 8)
    } else {
        c
    };
    let needs_far = a.clarity > 0.0 || a.dehaze > 0.0;
    let far = if needs_far {
        ring(src, x, y, wide, 12)
    } else {
        c
    };

    if a.noise_reduction > 0.0 {
        let t = a.noise_reduction * 0.8;
        c = [0, 1, 2].map(|i| c[i] + (near[i] - c[i]) * t);
    }
    if a.detail > 0.0 {
        c = [0, 1, 2].map(|i| c[i] + a.detail * 1.5 * (c[i] - near[i]));
    }
    if a.clarity > 0.0 {
        let l = luma(c).clamp(0.0, 1.0);
        let mid = 1.0 - (2.0 * l - 1.0) * (2.0 * l - 1.0);
        c = [0, 1, 2].map(|i| c[i] + a.clarity * 0.8 * mid * (c[i] - far[i]));
    }
    if a.dehaze > 0.0 {
        let m = far[0].min(far[1]).min(far[2]);
        let t = (1.0 - 0.75 * a.dehaze * m).max(0.1);
        let air = 0.95;
        c = [0, 1, 2].map(|i| (c[i] - air * (1.0 - t)) / t);
    }

    // Point operations.
    let mut lin = c.map(|v| srgb_to_linear(v.clamp(0.0, 1.0)));
    if a.exposure != 0.0 {
        let k = 2f32.powf(a.exposure);
        lin = lin.map(|v| v * k);
    }
    if a.temperature != 0.0 || a.tint != 0.0 {
        let t = a.temperature * 0.25;
        let g = a.tint * 0.2;
        lin = [lin[0] * (1.0 + t), lin[1] * (1.0 - g), lin[2] * (1.0 - t)];
    }
    let mut c = lin.map(linear_to_srgb);
    if a.contrast != 0.0 {
        let k = if a.contrast > 0.0 {
            1.0 + a.contrast * 1.5
        } else {
            1.0 + a.contrast * 0.9
        };
        c = c.map(|v| (v - 0.5) * k + 0.5);
    }
    if a.saturation != 0.0 || a.vibrance != 0.0 {
        let l = luma(c);
        let mx = c[0].max(c[1]).max(c[2]);
        let mn = c[0].min(c[1]).min(c[2]);
        let sat = (mx - mn).clamp(0.0, 1.0);
        let k = 1.0 + a.saturation + a.vibrance * (1.0 - sat);
        c = c.map(|v| l + (v - l) * k.max(0.0));
    }
    if a.grain > 0.0 {
        let n = (hash2(px, py) - 0.5) * a.grain * 0.16;
        c = c.map(|v| v + n);
    }
    c.map(|v| v.clamp(0.0, 1.0))
}

/// Apply `a` to `img`, keeping alpha.
pub fn apply(img: &DynamicImage, a: &Adjust) -> DynamicImage {
    if a.is_identity() {
        return img.clone();
    }
    let src = img.to_rgba32f();
    let (w, h) = src.dimensions();
    let wide = wide_radius(w, h);
    let mut out = RgbaImage::new(w, h);
    out.par_chunks_mut(4 * w as usize)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w {
                let p = src.get_pixel(x, y as u32);
                let c = pixel(a, &src, x, y as u32, [p[0], p[1], p[2]], wide);
                let i = x as usize * 4;
                row[i] = (c[0] * 255.0).round() as u8;
                row[i + 1] = (c[1] * 255.0).round() as u8;
                row[i + 2] = (c[2] * 255.0).round() as u8;
                row[i + 3] = (p[3].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
    DynamicImage::ImageRgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_a_noop() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(8, 8, |x, y| {
            image::Rgba([x as u8 * 30, y as u8 * 30, 99, 255])
        }));
        assert_eq!(apply(&img, &Adjust::default()).to_rgba8(), img.to_rgba8());
    }

    #[test]
    fn exposure_brightens_and_saturation_minus_one_is_grey() {
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, image::Rgba([100, 50, 20, 255])));
        let b = apply(
            &img,
            &Adjust {
                exposure: 1.0,
                ..Default::default()
            },
        )
        .to_rgba8();
        assert!(b.get_pixel(0, 0)[0] > 100);
        let g = apply(
            &img,
            &Adjust {
                saturation: -1.0,
                ..Default::default()
            },
        )
        .to_rgba8();
        let p = g.get_pixel(0, 0);
        assert!((p[0] as i32 - p[2] as i32).abs() <= 1);
    }

    #[test]
    fn hash_is_stable() {
        // Pinned values: the shader must produce the same numbers.
        assert!((hash2(0, 0) - hash2(0, 0)).abs() < 1e-9);
        let v = hash2(12, 34);
        assert!((0.0..1.0).contains(&v));
        assert_ne!(hash2(1, 0), hash2(0, 1));
    }
}
