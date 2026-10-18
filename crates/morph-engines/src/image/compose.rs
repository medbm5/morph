//! Compositions built as SVG scenes: annotations, backgrounds and collages.
//!
//! One builder serves both sides: the editor shows the SVG as its live preview
//! (image hrefs swapped for the webview's file URLs) and the export renders the same
//! SVG with resvg (hrefs resolved to the decoded, orientation-corrected pixels).

use super::decode::decode;
use anyhow::{bail, Context, Result};
use image::{DynamicImage, GenericImageView};
use morph_core::Kind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// An SVG scene referring to source images as `morph:0`, `morph:1`…
#[derive(Debug, Clone, Serialize)]
pub struct Scene {
    pub svg: String,
    pub width: u32,
    pub height: u32,
}

/// A decoded source image and its file.
pub struct Source {
    pub path: PathBuf,
    pub img: Arc<DynamicImage>,
}

impl Source {
    pub fn load(path: &std::path::Path, kind: &Kind) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            img: Arc::new(decode(path, kind)?),
        })
    }
    pub fn size(&self) -> (u32, u32) {
        self.img.dimensions()
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Accept `#rgb`, `#rrggbb`, `#rrggbbaa`; anything else falls back.
fn color(v: Option<&Value>, fallback: &str) -> String {
    let s = v.and_then(Value::as_str).unwrap_or(fallback).trim();
    let ok = s.starts_with('#')
        && matches!(s.len(), 4 | 7 | 9)
        && s[1..].chars().all(|c| c.is_ascii_hexdigit());
    let s = if ok { s } else { fallback };
    if s.len() == 9 {
        // SVG 1.1 has no #rrggbbaa: split into colour + opacity.
        let a = u8::from_str_radix(&s[7..9], 16).unwrap_or(255) as f32 / 255.0;
        return format!(
            "{}\" fill-opacity=\"{a:.3}\" stroke-opacity=\"{a:.3}",
            &s[..7]
        );
    }
    s.to_string()
}

fn num(v: &Value, k: &str, d: f64) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(d)
}

// ---------------------------------------------------------------- annotate

/// Annotations over the image. `shapes` is the editor's JSON list (image pixels).
pub fn annotate(src: &Source, shapes: &Value) -> Result<Scene> {
    let (w, h) = src.size();
    let base = (w.max(h) as f64 / 400.0).max(1.0); // default stroke scales with the image
    let mut s = String::new();
    let mut defs = String::new();
    let mut body = String::new();
    write!(
        body,
        r#"<image href="morph:0" x="0" y="0" width="{w}" height="{h}"/>"#
    )?;
    let list = shapes.as_array().cloned().unwrap_or_default();
    for (i, sh) in list.iter().enumerate() {
        let kind = sh.get("type").and_then(Value::as_str).unwrap_or("");
        let col = color(sh.get("color"), "#FF3B30");
        let sw = num(sh, "width", 3.0 * base);
        match kind {
            "arrow" => {
                let (x1, y1, x2, y2) = (
                    num(sh, "x1", 0.0),
                    num(sh, "y1", 0.0),
                    num(sh, "x2", 0.0),
                    num(sh, "y2", 0.0),
                );
                let ang = (y2 - y1).atan2(x2 - x1);
                let head = sw * 4.5;
                let (lx, ly) = (x2 - head * 0.8 * ang.cos(), y2 - head * 0.8 * ang.sin());
                let p = |a: f64| (x2 - head * (ang + a).cos(), y2 - head * (ang + a).sin());
                let (ax, ay) = p(0.45);
                let (bx, by) = p(-0.45);
                write!(
                    body,
                    r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{lx:.1}" y2="{ly:.1}" stroke="{col}" stroke-width="{sw:.1}" stroke-linecap="round"/>"#
                )?;
                write!(
                    body,
                    r#"<polygon points="{x2:.1},{y2:.1} {ax:.1},{ay:.1} {bx:.1},{by:.1}" fill="{col}"/>"#
                )?;
            }
            "rect" | "ellipse" => {
                let (x, y, rw, rh) = (
                    num(sh, "x", 0.0),
                    num(sh, "y", 0.0),
                    num(sh, "w", 0.0),
                    num(sh, "h", 0.0),
                );
                let fill = if sh.get("fill").and_then(Value::as_bool).unwrap_or(false) {
                    col.clone()
                } else {
                    "none".into()
                };
                if kind == "rect" {
                    let r = num(sh, "radius", 0.0);
                    write!(
                        body,
                        r#"<rect x="{x:.1}" y="{y:.1}" width="{rw:.1}" height="{rh:.1}" rx="{r:.1}" fill="{fill}" stroke="{col}" stroke-width="{sw:.1}"/>"#
                    )?;
                } else {
                    write!(
                        body,
                        r#"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{:.1}" fill="{fill}" stroke="{col}" stroke-width="{sw:.1}"/>"#,
                        x + rw / 2.0,
                        y + rh / 2.0,
                        rw / 2.0,
                        rh / 2.0
                    )?;
                }
            }
            "text" => {
                let (x, y) = (num(sh, "x", 0.0), num(sh, "y", 0.0));
                let size = num(sh, "size", 14.0 * base);
                let t = esc(sh.get("text").and_then(Value::as_str).unwrap_or(""));
                // Outline keeps text legible on any background.
                write!(
                    body,
                    r##"<text x="{x:.1}" y="{y:.1}" font-family="Segoe UI, Arial, Helvetica, sans-serif" font-weight="700" font-size="{size:.1}" fill="{col}" stroke="#000000" stroke-opacity="0.55" stroke-width="{:.1}" paint-order="stroke">{t}</text>"##,
                    size / 8.0
                )?;
            }
            "callout" => {
                let (x, y) = (num(sh, "x", 0.0), num(sh, "y", 0.0));
                let r = num(sh, "size", 14.0 * base);
                let n = sh.get("n").and_then(Value::as_u64).unwrap_or(i as u64 + 1);
                write!(
                    body,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="{col}" stroke="#FFFFFF" stroke-width="{:.1}"/>"##,
                    r / 7.0
                )?;
                write!(
                    body,
                    r##"<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-family="Segoe UI, Arial, Helvetica, sans-serif" font-weight="700" font-size="{:.1}" fill="#FFFFFF">{n}</text>"##,
                    y + r * 0.36,
                    r * 1.05
                )?;
            }
            "magnifier" => {
                let (x, y) = (num(sh, "x", 0.0), num(sh, "y", 0.0));
                let r = num(sh, "r", 60.0 * base);
                let zoom = num(sh, "zoom", 2.0).clamp(1.0, 8.0);
                let (sx, sy) = (num(sh, "sx", x), num(sh, "sy", y));
                write!(
                    defs,
                    r#"<clipPath id="mag{i}"><circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}"/></clipPath>"#
                )?;
                let (tx, ty) = (x - sx * zoom, y - sy * zoom);
                write!(
                    body,
                    r#"<g clip-path="url(#mag{i})"><image href="morph:0" x="0" y="0" width="{w}" height="{h}" transform="translate({tx:.2} {ty:.2}) scale({zoom:.3})"/></g>"#
                )?;
                write!(
                    body,
                    r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="none" stroke="{col}" stroke-width="{sw:.1}"/>"#
                )?;
            }
            "pen" | "highlight" => {
                let pts = sh
                    .get("points")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut d = String::new();
                for (j, p) in pts.iter().enumerate() {
                    let (px, py) = (
                        p.get(0).and_then(Value::as_f64).unwrap_or(0.0),
                        p.get(1).and_then(Value::as_f64).unwrap_or(0.0),
                    );
                    write!(d, "{}{px:.1} {py:.1} ", if j == 0 { "M" } else { "L" })?;
                }
                if kind == "highlight" {
                    let hw = num(sh, "width", 18.0 * base);
                    let hc = color(sh.get("color"), "#FFE600");
                    write!(
                        body,
                        r#"<path d="{d}" fill="none" stroke="{hc}" stroke-opacity="0.4" stroke-width="{hw:.1}" stroke-linecap="square" stroke-linejoin="round"/>"#
                    )?;
                } else {
                    write!(
                        body,
                        r#"<path d="{d}" fill="none" stroke="{col}" stroke-width="{sw:.1}" stroke-linecap="round" stroke-linejoin="round"/>"#
                    )?;
                }
            }
            "blur" | "redact" => {
                // Used by the redact tool (S7): solid box or blurred region.
                let (x, y, rw, rh) = (
                    num(sh, "x", 0.0),
                    num(sh, "y", 0.0),
                    num(sh, "w", 0.0),
                    num(sh, "h", 0.0),
                );
                if kind == "redact" {
                    let fill = color(sh.get("color"), "#000000");
                    write!(
                        body,
                        r#"<rect x="{x:.1}" y="{y:.1}" width="{rw:.1}" height="{rh:.1}" fill="{fill}"/>"#
                    )?;
                } else {
                    let sd = num(sh, "strength", (rw.min(rh) / 6.0).max(4.0));
                    write!(
                        defs,
                        r#"<clipPath id="bl{i}"><rect x="{x:.1}" y="{y:.1}" width="{rw:.1}" height="{rh:.1}"/></clipPath><filter id="blf{i}" x="0" y="0" width="1" height="1" filterUnits="userSpaceOnUse"><feGaussianBlur stdDeviation="{sd:.1}" edgeMode="duplicate"/></filter>"#
                    )?;
                    write!(
                        body,
                        r#"<g clip-path="url(#bl{i})"><image href="morph:0" x="0" y="0" width="{w}" height="{h}" filter="url(#blf{i})"/></g>"#
                    )?;
                }
            }
            "" => {}
            other => bail!("unknown annotation `{other}`"),
        }
    }
    write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs>{defs}</defs>{body}</svg>"#
    )?;
    Ok(Scene {
        svg: s,
        width: w,
        height: h,
    })
}

// ---------------------------------------------------------------- background

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Background {
    /// `solid`, `gradient`, `image`, `blur`.
    pub kind: String,
    pub color: String,
    pub color2: String,
    /// Gradient angle in degrees.
    pub angle: f64,
    /// Padding as % of the image's longer side.
    pub padding: f64,
    /// Corner radius in px of the photo.
    pub radius: f64,
    /// Shadow strength 0–1.
    pub shadow: f64,
    /// `auto`, a ratio (`1:1`, `16:9`) or an exact size (`1080x1350`).
    pub size: String,
    /// Background picture for `image`.
    pub image: Option<PathBuf>,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            kind: "gradient".into(),
            color: "#4FE3FF".into(),
            color2: "#FFB347".into(),
            angle: 135.0,
            padding: 8.0,
            radius: 24.0,
            shadow: 0.5,
            size: "auto".into(),
            image: None,
        }
    }
}

fn parse_size(s: &str) -> Result<Option<SizeSpec>> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() || s == "auto" {
        return Ok(None);
    }
    if let Some((a, b)) = s.split_once('x') {
        if let (Ok(w), Ok(h)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) {
            return Ok(Some(SizeSpec::Exact(w.max(1), h.max(1))));
        }
    }
    match super::crop::parse_ratio(&s)? {
        Some((a, b)) => Ok(Some(SizeSpec::Ratio(a / b))),
        None => Ok(None),
    }
}

enum SizeSpec {
    Exact(u32, u32),
    Ratio(f64),
}

/// The photo on a background canvas. `extra` is the optional background image.
pub fn background(src: &Source, bg: &Background, extra: Option<&Source>) -> Result<Scene> {
    let (iw, ih) = src.size();
    let (iw, ih) = (iw as f64, ih as f64);
    let pad = iw.max(ih) * bg.padding.clamp(0.0, 100.0) / 100.0;
    // Canvas size and the photo's placed size.
    let (cw, ch, pw, ph) = match parse_size(&bg.size)? {
        None => (iw + 2.0 * pad, ih + 2.0 * pad, iw, ih),
        Some(SizeSpec::Ratio(r)) => {
            let (mut cw, mut ch) = (iw + 2.0 * pad, ih + 2.0 * pad);
            if cw / ch < r {
                cw = ch * r;
            } else {
                ch = cw / r;
            }
            (cw, ch, iw, ih)
        }
        Some(SizeSpec::Exact(w, h)) => {
            let (w, h) = (w as f64, h as f64);
            let pad = w.max(h) * bg.padding.clamp(0.0, 100.0) / 100.0;
            let s = ((w - 2.0 * pad) / iw).min((h - 2.0 * pad) / ih).max(0.01);
            (w, h, iw * s, ih * s)
        }
    };
    let (x, y) = ((cw - pw) / 2.0, (ch - ph) / 2.0);
    let (cw_i, ch_i) = (cw.round() as u32, ch.round() as u32);
    let r = bg.radius.max(0.0).min(pw.min(ph) / 2.0);
    let mut defs = String::new();
    let mut body = String::new();
    match bg.kind.as_str() {
        "solid" => write!(
            body,
            r#"<rect width="{cw:.1}" height="{ch:.1}" fill="{}"/>"#,
            color(Some(&Value::String(bg.color.clone())), "#FFFFFF")
        )?,
        "image" => {
            if extra.is_none() {
                bail!("choose a background picture");
            }
            write!(
                body,
                r#"<image href="morph:1" x="0" y="0" width="{cw:.1}" height="{ch:.1}" preserveAspectRatio="xMidYMid slice"/>"#
            )?;
        }
        "blur" => {
            let sd = cw.max(ch) / 40.0;
            write!(
                defs,
                r#"<filter id="bgblur" x="0" y="0" width="1" height="1"><feGaussianBlur stdDeviation="{sd:.1}" edgeMode="duplicate"/></filter>"#
            )?;
            write!(
                body,
                r#"<image href="morph:0" x="0" y="0" width="{cw:.1}" height="{ch:.1}" preserveAspectRatio="xMidYMid slice" filter="url(#bgblur)"/>"#
            )?;
            write!(
                body,
                r##"<rect width="{cw:.1}" height="{ch:.1}" fill="#000000" fill-opacity="0.15"/>"##
            )?;
        }
        _ => {
            let a = bg.angle.to_radians();
            let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
            write!(
                defs,
                r#"<linearGradient id="bg" x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient>"#,
                0.5 - dx,
                0.5 - dy,
                0.5 + dx,
                0.5 + dy,
                color(Some(&Value::String(bg.color.clone())), "#4FE3FF"),
                color(Some(&Value::String(bg.color2.clone())), "#FFB347")
            )?;
            write!(
                body,
                r#"<rect width="{cw:.1}" height="{ch:.1}" fill="url(#bg)"/>"#
            )?;
        }
    }
    if bg.shadow > 0.0 {
        let blur = pw.max(ph) / 40.0;
        write!(
            defs,
            r#"<filter id="sh" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="{blur:.1}"/></filter>"#
        )?;
        write!(
            body,
            r##"<rect x="{x:.1}" y="{:.1}" width="{pw:.1}" height="{ph:.1}" rx="{r:.1}" fill="#000000" fill-opacity="{:.3}" filter="url(#sh)"/>"##,
            y + blur * 0.6,
            bg.shadow.clamp(0.0, 1.0) * 0.55
        )?;
    }
    write!(
        defs,
        r#"<clipPath id="photo"><rect x="{x:.1}" y="{y:.1}" width="{pw:.1}" height="{ph:.1}" rx="{r:.1}"/></clipPath>"#
    )?;
    write!(
        body,
        r#"<image href="morph:0" x="{x:.1}" y="{y:.1}" width="{pw:.1}" height="{ph:.1}" preserveAspectRatio="none" clip-path="url(#photo)"/>"#
    )?;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{cw_i}" height="{ch_i}" viewBox="0 0 {cw:.1} {ch:.1}"><defs>{defs}</defs>{body}</svg>"#
    );
    Ok(Scene {
        svg,
        width: cw_i,
        height: ch_i,
    })
}

// ---------------------------------------------------------------- collage

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Collage {
    /// `grid`, `row`, `column`, `featured`.
    pub layout: String,
    /// Output width in px (height follows the layout).
    pub width: u32,
    pub spacing: f64,
    pub radius: f64,
    pub background: String,
}

impl Default for Collage {
    fn default() -> Self {
        Self {
            layout: "grid".into(),
            width: 2400,
            spacing: 16.0,
            radius: 12.0,
            background: "#FFFFFF".into(),
        }
    }
}

pub fn collage(srcs: &[Source], c: &Collage) -> Result<Scene> {
    if srcs.is_empty() {
        bail!("no images");
    }
    let n = srcs.len();
    let w = c.width.clamp(200, 20000) as f64;
    let s = c.spacing.max(0.0);
    let aspect: Vec<f64> = srcs
        .iter()
        .map(|x| {
            let (a, b) = x.size();
            a as f64 / b as f64
        })
        .collect();
    // Cells: (x, y, w, h) per image, and the canvas height.
    let mut cells = Vec::with_capacity(n);
    let height = match c.layout.as_str() {
        "row" => {
            // Justified row: equal heights, widths follow each aspect.
            let h = (w - s * (n as f64 + 1.0)) / aspect.iter().sum::<f64>();
            let mut x = s;
            for a in &aspect {
                cells.push((x, s, h * a, h));
                x += h * a + s;
            }
            h + 2.0 * s
        }
        "column" => {
            let cw = w - 2.0 * s;
            let mut y = s;
            for a in &aspect {
                let h = cw / a;
                cells.push((s, y, cw, h));
                y += h + s;
            }
            y
        }
        "featured" if n > 1 => {
            let big_w = (w - 3.0 * s) * 0.62;
            let side_w = w - 3.0 * s - big_w;
            let rest = n - 1;
            let total_h = big_w / aspect[0].max(0.2);
            let side_h = (total_h - s * (rest as f64 - 1.0)) / rest as f64;
            cells.push((s, s, big_w, total_h));
            for i in 0..rest {
                cells.push((2.0 * s + big_w, s + i as f64 * (side_h + s), side_w, side_h));
            }
            total_h + 2.0 * s
        }
        _ => {
            let cols = (n as f64).sqrt().ceil() as usize;
            let rows = n.div_ceil(cols);
            let cw = (w - s * (cols as f64 + 1.0)) / cols as f64;
            let mean = aspect.iter().sum::<f64>() / n as f64;
            let ch = cw / mean.clamp(0.5, 2.0);
            for i in 0..n {
                let (cx, cy) = (i % cols, i / cols);
                cells.push((s + cx as f64 * (cw + s), s + cy as f64 * (ch + s), cw, ch));
            }
            rows as f64 * (ch + s) + s
        }
    };
    let (wi, hi) = (w.round() as u32, height.round().max(1.0) as u32);
    let r = c.radius.max(0.0);
    let mut defs = String::new();
    let mut body = format!(
        r#"<rect width="{w:.1}" height="{height:.1}" fill="{}"/>"#,
        color(Some(&Value::String(c.background.clone())), "#FFFFFF")
    );
    for (i, (x, y, cw, ch)) in cells.iter().enumerate() {
        write!(
            defs,
            r#"<clipPath id="c{i}"><rect x="{x:.1}" y="{y:.1}" width="{cw:.1}" height="{ch:.1}" rx="{r:.1}"/></clipPath>"#
        )?;
        write!(
            body,
            r#"<image href="morph:{i}" x="{x:.1}" y="{y:.1}" width="{cw:.1}" height="{ch:.1}" preserveAspectRatio="xMidYMid slice" clip-path="url(#c{i})"/>"#
        )?;
    }
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{wi}" height="{hi}" viewBox="0 0 {w:.1} {height:.1}"><defs>{defs}</defs>{body}</svg>"#
    );
    Ok(Scene {
        svg,
        width: wi,
        height: hi,
    })
}

// ---------------------------------------------------------------- render

/// Render a scene. `images[i]` backs `morph:i`.
pub fn render(scene: &Scene, images: &[Arc<DynamicImage>]) -> Result<DynamicImage> {
    use resvg::usvg;
    // Encode each source once as a fast PNG for usvg's image loader.
    let encoded: Vec<Arc<Vec<u8>>> = images
        .iter()
        .map(|img| {
            let rgba = img.to_rgba8();
            let mut out = Vec::new();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut out,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::NoFilter,
            )
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map(|_| Arc::new(out))
            .map_err(anyhow::Error::from)
        })
        .collect::<Result<_>>()?;
    use image::ImageEncoder;
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let resolver_images = encoded.clone();
    opt.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
        resolve_string: Box::new(move |href: &str, _opts: &usvg::Options| {
            let i: usize = href.strip_prefix("morph:")?.parse().ok()?;
            resolver_images
                .get(i)
                .map(|b| usvg::ImageKind::PNG(b.clone()))
        }),
    };
    let tree = usvg::Tree::from_str(&scene.svg, &opt).context("building scene")?;
    let (w, h) = (scene.width.max(1), scene.height.max(1));
    if w as u64 * h as u64 > 400_000_000 {
        bail!("output would be too large ({w}×{h})");
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).context("allocating canvas")?;
    let sx = w as f32 / tree.size().width();
    let sy = h as f32 / tree.size().height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok(DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(w, h, rgba).context("canvas")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(w: u32, h: u32, c: [u8; 3]) -> Source {
        Source {
            path: PathBuf::from("x.png"),
            img: Arc::new(DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                w,
                h,
                image::Rgb(c),
            ))),
        }
    }

    #[test]
    fn annotate_renders_shapes_over_the_image() {
        let s = src(200, 100, [255, 255, 255]);
        let shapes = serde_json::json!([
            { "type": "rect", "x": 10, "y": 10, "w": 50, "h": 40, "color": "#FF0000", "width": 4, "fill": true },
            { "type": "arrow", "x1": 100, "y1": 80, "x2": 180, "y2": 20, "color": "#0000FF" },
            { "type": "callout", "x": 150, "y": 70, "n": 3 },
            { "type": "magnifier", "x": 60, "y": 70, "r": 20, "zoom": 2, "sx": 30, "sy": 30 }
        ]);
        let scene = annotate(&s, &shapes).unwrap();
        let out = render(&scene, std::slice::from_ref(&s.img))
            .unwrap()
            .to_rgba8();
        assert_eq!(out.dimensions(), (200, 100));
        assert_eq!(out.get_pixel(30, 30).0[..3], [255, 0, 0]);
        assert_eq!(out.get_pixel(5, 95).0[..3], [255, 255, 255]);
    }

    #[test]
    fn background_sizes() {
        let s = src(400, 300, [10, 20, 30]);
        let bg = Background {
            size: "1:1".into(),
            padding: 10.0,
            kind: "solid".into(),
            color: "#00FF00".into(),
            shadow: 0.0,
            ..Default::default()
        };
        let scene = background(&s, &bg, None).unwrap();
        assert_eq!(scene.width, scene.height);
        let out = render(&scene, std::slice::from_ref(&s.img))
            .unwrap()
            .to_rgba8();
        assert_eq!(out.get_pixel(1, 1).0[..3], [0, 255, 0]);
        let c = out.get_pixel(out.width() / 2, out.height() / 2).0;
        assert_eq!(c[..3], [10, 20, 30]);
        let exact = background(
            &s,
            &Background {
                size: "1080x1350".into(),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!((exact.width, exact.height), (1080, 1350));
    }

    #[test]
    fn collage_layouts() {
        let srcs = vec![
            src(400, 300, [255, 0, 0]),
            src(300, 400, [0, 255, 0]),
            src(400, 400, [0, 0, 255]),
        ];
        for layout in ["grid", "row", "column", "featured"] {
            let scene = collage(
                &srcs,
                &Collage {
                    layout: layout.into(),
                    width: 900,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(scene.width, 900, "{layout}");
            let imgs: Vec<_> = srcs.iter().map(|s| s.img.clone()).collect();
            let out = render(&scene, &imgs).unwrap();
            assert!(out.height() > 100, "{layout}");
        }
    }
}
