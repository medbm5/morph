//! S7: local face/text detection, OCR and Redact. Needs the `ml` (and `ocr`)
//! components: `morph component install ml ocr`. Skipped with a notice otherwise.
//! CI runs this suite with the network disabled (`unshare -n`).

use image::GenericImageView;
use morph_core::{Job, Params, Runner};
use morph_engines::ml::{self, Region};
use std::path::{Path, PathBuf};

fn ready(component: &str) -> bool {
    let ok = ml::available() && morph_core::components::is_installed(component);
    if !ok {
        eprintln!("component `{component}` not installed — skipping (morph component install {component})");
    }
    ok
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/faces")
        .join(name)
}

#[test]
fn finds_at_least_90_percent_of_faces() {
    if !ready("ml") {
        return;
    }
    let labels = [
        ("expedition50.jpg", 6usize),
        ("expedition51.jpg", 6),
        ("expedition57.jpg", 3),
    ];
    let (mut found, mut total) = (0, 0);
    for (file, n) in labels {
        let img = image::open(fixture(file)).unwrap();
        let faces = ml::face::detect(&img).unwrap();
        eprintln!("{file}: {} of {n} faces", faces.len());
        assert!(
            faces.len() <= n + 1,
            "{file}: {} detections for {n} faces",
            faces.len()
        );
        // Faces in these portraits sit in the upper 60 % of the frame.
        let (_, h) = img.dimensions();
        assert!(
            faces.iter().all(|f| f.y + f.h / 2.0 < h as f32 * 0.6),
            "{file}: implausible box {faces:?}"
        );
        found += faces.len().min(n);
        total += n;
    }
    let recall = found as f64 / total as f64;
    assert!(recall >= 0.9, "face recall {recall:.2} ({found}/{total})");
}

/// A light page with words at known positions.
fn text_page(words: &[(&str, f32, f32, f32)]) -> (image::DynamicImage, Vec<Region>) {
    let (w, h) = (1200u32, 800u32);
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#F4F1E8"/>"##
    );
    let mut boxes = Vec::new();
    for (t, x, y, size) in words {
        svg.push_str(&format!(r##"<text x="{x}" y="{y}" font-family="Arial, Helvetica, DejaVu Sans, sans-serif" font-size="{size}" fill="#111111">{t}</text>"##));
        let width = t.chars().count() as f32 * size * 0.55;
        boxes.push(Region {
            x: *x,
            y: y - size * 0.8,
            w: width,
            h: *size,
            score: 1.0,
        });
    }
    svg.push_str("</svg>");
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).unwrap();
    let mut pix = resvg::tiny_skia::Pixmap::new(w, h).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pix.as_mut(),
    );
    let img = image::RgbaImage::from_raw(w, h, pix.take()).unwrap();
    (image::DynamicImage::ImageRgba8(img), boxes)
}

const WORDS: &[(&str, f32, f32, f32)] = &[
    ("Invoice", 80.0, 120.0, 48.0),
    ("Metamorphosis", 80.0, 220.0, 40.0),
    ("Originals", 620.0, 220.0, 36.0),
    ("never", 80.0, 320.0, 36.0),
    ("destroyed", 300.0, 320.0, 36.0),
    ("Offline", 700.0, 420.0, 32.0),
    ("Account 4417", 80.0, 520.0, 34.0),
    ("Total 1,280.00", 600.0, 620.0, 34.0),
    ("Signature", 80.0, 720.0, 30.0),
    ("Stand", 900.0, 720.0, 30.0),
];

#[test]
fn finds_at_least_90_percent_of_text_regions() {
    if !ready("ml") {
        return;
    }
    let (img, truth) = text_page(WORDS);
    let found = ml::text::detect(&img).unwrap();
    let covered = truth
        .iter()
        .filter(|t| {
            // Covered when detections overlap at least half of the word's box.
            let area: f32 = found
                .iter()
                .map(|f| {
                    let x1 = t.x.max(f.x);
                    let y1 = t.y.max(f.y);
                    let x2 = (t.x + t.w).min(f.x + f.w);
                    let y2 = (t.y + t.h).min(f.y + f.h);
                    (x2 - x1).max(0.0) * (y2 - y1).max(0.0)
                })
                .sum();
            area >= 0.5 * t.w * t.h
        })
        .count();
    let recall = covered as f64 / truth.len() as f64;
    eprintln!(
        "text regions: {} detections, {covered}/{} words covered",
        found.len(),
        truth.len()
    );
    assert!(recall >= 0.9, "text recall {recall:.2}");
}

#[test]
fn reads_text() {
    if !ready("ocr") {
        return;
    }
    let (img, _) = text_page(&[
        ("METAMORPHOSIS", 100.0, 200.0, 64.0),
        ("offline 2026", 100.0, 400.0, 56.0),
    ]);
    let text = ml::text::ocr_text(&img).unwrap().to_uppercase();
    eprintln!("ocr: {text:?}");
    assert!(text.contains("METAMORPHOSIS"), "{text}");
    assert!(text.contains("2026"), "{text}");
}

#[test]
fn redact_blacks_out_detected_faces() {
    if !ready("ml") {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("crew.jpg");
    std::fs::copy(fixture("expedition57.jpg"), &src).unwrap();
    let img = image::open(&src).unwrap();
    let faces = ml::face::detect(&img).unwrap();
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(
            &Job::new("image.redact", vec![src.clone()])
                .params(Params::new().with("auto", "faces")),
            |_| {},
        )
        .unwrap();
    assert!(r.ok(), "{:?}", r.errors);
    let out = image::open(&r.outputs[0]).unwrap().to_rgb8();
    for f in faces {
        let c = out.get_pixel((f.x + f.w / 2.0) as u32, (f.y + f.h / 2.0) as u32);
        assert!(
            c.0.iter().all(|&v| v < 16),
            "face centre not redacted: {c:?}"
        );
    }
    // Unredacted corner unchanged-ish (JPEG re-encode).
    let a = img.to_rgb8();
    let (p, q) = (a.get_pixel(5, 5), out.get_pixel(5, 5));
    assert!(p
        .0
        .iter()
        .zip(q.0)
        .all(|(x, y)| (*x as i32 - y as i32).abs() < 12));
}

#[test]
fn searchable_pdf_has_a_text_layer() {
    if !ready("ocr") {
        return;
    }
    let (img, _) = text_page(&[("Searchable", 100.0, 200.0, 64.0)]);
    let d = tempfile::tempdir().unwrap();
    let out = d.path().join("page.pdf");
    let words = ml::text::ocr(&img).unwrap();
    morph_engines::ml::ops::searchable_pdf(&img, &words, &out).unwrap();
    let doc = lopdf::Document::load(&out).unwrap();
    let text = doc.extract_text(&[1]).unwrap_or_default();
    assert!(text.to_lowercase().contains("searchable"), "{text}");
}

#[test]
fn removes_the_background() {
    if !ready("bgremove") {
        return;
    }
    let img = image::open(fixture("expedition57.jpg")).unwrap();
    let cut = ml::bg::remove(&img).unwrap().to_rgba8();
    // Starry sky corner becomes transparent; a crew member's chest stays opaque.
    assert!(
        cut.get_pixel(20, 20)[3] < 40,
        "corner alpha {}",
        cut.get_pixel(20, 20)[3]
    );
    assert!(
        cut.get_pixel(640, 500)[3] > 200,
        "subject alpha {}",
        cut.get_pixel(640, 500)[3]
    );
}
