//! Text regions (PP-OCRv3 DB detector) and recognition (PP-OCRv3 CRNN, English).

use super::{model, to_nchw, with_session, Region};
use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ort::value::Tensor;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Detect text regions (one box per word or line), in source pixels.
pub fn detect(img: &DynamicImage) -> Result<Vec<Region>> {
    let (w, h) = img.dimensions();
    // Long side ≤ 960, both sides multiples of 32.
    let s = (960.0 / w.max(h) as f32).min(1.0);
    let rw = (((w as f32 * s) / 32.0).round() as u32).max(1) * 32;
    let rh = (((h as f32 * s) / 32.0).round() as u32).max(1) * 32;
    let input = to_nchw(img, rw, rh, true, |c, v| (v / 255.0 - MEAN[c]) / STD[c]);
    let path = model("ml", "text-det-ppocrv3-en.onnx")?;
    let prob = with_session(&path, |sess| {
        let t = Tensor::from_array(([1usize, 3, rh as usize, rw as usize], input))?;
        let out = sess.run(ort::inputs![t])?;
        let (_, p) = out[0].try_extract_tensor::<f32>()?;
        Ok(p.to_vec())
    })?;
    let (sx, sy) = (w as f32 / rw as f32, h as f32 / rh as f32);
    Ok(components(&prob, rw as usize, rh as usize, 0.3)
        .into_iter()
        .filter(|c| c.mean >= 0.6 && c.w() >= 3 && c.h() >= 3)
        .map(|c| {
            // DB shrinks text kernels during training: grow back ("unclip").
            let (bw, bh) = (c.w() as f32, c.h() as f32);
            let d = bw * bh * 1.5 / (2.0 * (bw + bh));
            let x = (c.x0 as f32 - d).max(0.0);
            let y = (c.y0 as f32 - d).max(0.0);
            Region {
                x: x * sx,
                y: y * sy,
                w: ((bw + 2.0 * d).min(rw as f32 - x)) * sx,
                h: ((bh + 2.0 * d).min(rh as f32 - y)) * sy,
                score: c.mean,
            }
        })
        .collect())
}

struct Comp {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    mean: f32,
}

impl Comp {
    fn w(&self) -> usize {
        self.x1 - self.x0 + 1
    }
    fn h(&self) -> usize {
        self.y1 - self.y0 + 1
    }
}

/// 4-connected components of `prob > t`, with bounding box and mean probability.
fn components(prob: &[f32], w: usize, h: usize, t: f32) -> Vec<Comp> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start] || prob[start] <= t {
            continue;
        }
        let mut c = Comp {
            x0: usize::MAX,
            y0: usize::MAX,
            x1: 0,
            y1: 0,
            mean: 0.0,
        };
        let (mut sum, mut n) = (0f32, 0usize);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            c.x0 = c.x0.min(x);
            c.y0 = c.y0.min(y);
            c.x1 = c.x1.max(x);
            c.y1 = c.y1.max(y);
            sum += prob[i];
            n += 1;
            let mut push = |j: usize| {
                if !seen[j] && prob[j] > t {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < w {
                push(i + 1);
            }
            if y > 0 {
                push(i - w);
            }
            if y + 1 < h {
                push(i + w);
            }
        }
        c.mean = sum / n as f32;
        out.push(c);
    }
    out
}

/// Recognise the text inside one region.
pub fn recognize(img: &DynamicImage, r: &Region) -> Result<String> {
    let dict_path = model("ocr", "text-rec-dict-en.txt")?;
    let mut dict: Vec<String> = std::fs::read_to_string(&dict_path)?
        .lines()
        .map(str::to_string)
        .collect();
    dict.push(" ".into());
    let (w, h) = img.dimensions();
    let (x, y) = (r.x.max(0.0) as u32, r.y.max(0.0) as u32);
    let cw = (r.w as u32).clamp(1, w - x.min(w - 1));
    let ch = (r.h as u32).clamp(1, h - y.min(h - 1));
    let crop = img.crop_imm(x, y, cw, ch);
    let th = 48u32;
    let tw = ((th as f32 * cw as f32 / ch as f32).ceil() as u32).clamp(16, 1600);
    let input = to_nchw(&crop, tw, th, true, |_, v| (v / 255.0 - 0.5) / 0.5);
    let path = model("ocr", "text-rec-ppocrv3-en.onnx")?;
    with_session(&path, |s| {
        let t = Tensor::from_array(([1usize, 3, th as usize, tw as usize], input))?;
        let out = s.run(ort::inputs![t])?;
        let (shape, p) = out[0].try_extract_tensor::<f32>()?;
        let steps = shape[1] as usize;
        let classes = shape[2] as usize;
        let mut text = String::new();
        let mut last = 0usize;
        for t in 0..steps {
            let row = &p[t * classes..(t + 1) * classes];
            let (best, _) =
                row.iter()
                    .enumerate()
                    .fold((0, f32::MIN), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
            if best != 0 && best != last {
                if let Some(ch) = dict.get(best - 1) {
                    text.push_str(ch);
                }
            }
            last = best;
        }
        Ok(text.trim().to_string())
    })
    .context("recognising text")
}

/// Full-page OCR: regions in reading order with their text.
pub fn ocr(img: &DynamicImage) -> Result<Vec<(Region, String)>> {
    let mut regions = detect(img)?;
    // Reading order: top to bottom by line, then left to right.
    regions.sort_by(|a, b| {
        let same_line = (a.y - b.y).abs() < a.h.min(b.h) * 0.5;
        if same_line {
            a.x.partial_cmp(&b.x).unwrap()
        } else {
            a.y.partial_cmp(&b.y).unwrap()
        }
    });
    let mut out = Vec::new();
    for r in regions {
        let t = recognize(img, &r)?;
        if !t.is_empty() {
            out.push((r, t));
        }
    }
    Ok(out)
}

/// OCR result as plain text, one line per text row.
pub fn ocr_text(img: &DynamicImage) -> Result<String> {
    let words = ocr(img)?;
    let mut s = String::new();
    let mut last: Option<Region> = None;
    for (r, t) in words {
        if let Some(l) = last {
            if (r.y - l.y).abs() < l.h.min(r.h) * 0.5 {
                s.push(' ');
            } else {
                s.push('\n');
            }
        }
        s.push_str(&t);
        last = Some(r);
    }
    Ok(s)
}
