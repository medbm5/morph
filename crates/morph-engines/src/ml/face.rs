//! Face detection with UltraFace (RFB-320): fast, CPU-only, MIT-licensed.

use super::{model, nms, to_nchw, with_session, Region};
use anyhow::Result;
use image::{DynamicImage, GenericImageView};
use ort::value::Tensor;

const W: u32 = 320;
const H: u32 = 240;

/// One pass over `img` (resized to 320×240); boxes in `img` pixels.
fn pass(img: &DynamicImage, threshold: f32) -> Result<Vec<Region>> {
    let (iw, ih) = img.dimensions();
    let input = to_nchw(img, W, H, false, |_, v| (v - 127.0) / 128.0);
    let path = model("ml", "face-ultraface-rfb320.onnx")?;
    with_session(&path, |s| {
        let t = Tensor::from_array(([1usize, 3, H as usize, W as usize], input))?;
        let out = s.run(ort::inputs![t])?;
        let (_, scores) = out[0].try_extract_tensor::<f32>()?;
        let (_, boxes) = out[1].try_extract_tensor::<f32>()?;
        let mut r = Vec::new();
        for i in 0..scores.len() / 2 {
            let score = scores[i * 2 + 1];
            if score < threshold {
                continue;
            }
            let b = &boxes[i * 4..i * 4 + 4];
            let (x1, y1) = (
                b[0].clamp(0.0, 1.0) * iw as f32,
                b[1].clamp(0.0, 1.0) * ih as f32,
            );
            let (x2, y2) = (
                b[2].clamp(0.0, 1.0) * iw as f32,
                b[3].clamp(0.0, 1.0) * ih as f32,
            );
            if x2 > x1 && y2 > y1 {
                r.push(Region {
                    x: x1,
                    y: y1,
                    w: x2 - x1,
                    h: y2 - y1,
                    score,
                });
            }
        }
        Ok(r)
    })
}

/// Detect faces. Large photos are also scanned in overlapping tiles so small faces
/// aren't lost when the whole frame is shrunk to 320×240.
pub fn detect(img: &DynamicImage) -> Result<Vec<Region>> {
    let threshold = 0.7;
    let (w, h) = img.dimensions();
    let mut all = pass(img, threshold)?;
    if w.max(h) > 960 {
        let (tw, th) = (w * 3 / 5, h * 3 / 5);
        for (tx, ty) in [
            (0, 0),
            (w - tw, 0),
            (0, h - th),
            (w - tw, h - th),
            ((w - tw) / 2, (h - th) / 2),
        ] {
            let tile = img.crop_imm(tx, ty, tw, th);
            for mut r in pass(&tile, threshold)? {
                r.x += tx as f32;
                r.y += ty as f32;
                all.push(r);
            }
        }
    }
    Ok(nms(all, 0.3))
}
