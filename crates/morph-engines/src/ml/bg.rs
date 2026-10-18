//! Background removal with U²-Net (small): predicts a saliency mask used as alpha.

use super::{model, to_nchw, with_session};
use anyhow::Result;
use image::{imageops::FilterType, DynamicImage, GenericImageView, GrayImage, Luma, RgbaImage};
use ort::value::Tensor;

const S: u32 = 320;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// The subject cut out on a transparent background.
pub fn remove(img: &DynamicImage) -> Result<DynamicImage> {
    let (w, h) = img.dimensions();
    let input = to_nchw(img, S, S, false, |c, v| (v / 255.0 - MEAN[c]) / STD[c]);
    let path = model("bgremove", "bg-u2netp.onnx")?;
    let mask = with_session(&path, |s| {
        let t = Tensor::from_array(([1usize, 3, S as usize, S as usize], input))?;
        let out = s.run(ort::inputs![t])?;
        let (_, d) = out[0].try_extract_tensor::<f32>()?;
        Ok(d.to_vec())
    })?;
    let (mn, mx) = mask
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let span = (mx - mn).max(1e-6);
    let small = GrayImage::from_fn(S, S, |x, y| {
        Luma([(((mask[(y * S + x) as usize] - mn) / span) * 255.0) as u8])
    });
    let alpha = image::imageops::resize(&small, w, h, FilterType::Triangle);
    let mut out: RgbaImage = img.to_rgba8();
    for (p, a) in out.pixels_mut().zip(alpha.pixels()) {
        p[3] = ((p[3] as u32 * a[0] as u32) / 255) as u8;
    }
    Ok(DynamicImage::ImageRgba8(out))
}
