//! Conversion matrix: every image input × every image output, plus source-safety checks.

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use morph_core::{Job, Params, Ring, Runner};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const W: u32 = 64;
const H: u32 = 48;

fn fixture_image() -> DynamicImage {
    let mut img = RgbaImage::new(W, H);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let a = if x < 8 { 128 } else { 255 };
        *p = Rgba([(x * 4) as u8, (y * 5) as u8, 200, a]);
    }
    DynamicImage::ImageRgba8(img)
}

fn hash(p: &Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(p).unwrap()))
}

/// Write the fixture in every input format we can produce.
fn fixtures(dir: &Path) -> Vec<PathBuf> {
    let img = fixture_image();
    let opts = morph_engines::image::encode::EncodeOptions::default();
    let mut out = Vec::new();
    for ext in ["jpg", "png", "webp", "tiff", "bmp", "gif", "avif"] {
        if ext == "avif" && !morph_engines::image::heif::available() {
            continue; // AVIF input needs WIC (Windows) or heif-dec
        }
        let p = dir.join(format!("src-{ext}.{ext}"));
        morph_engines::image::encode::encode(&img, ext, &p, &opts).unwrap();
        out.push(p);
    }
    let svg = dir.join("src-svg.svg");
    std::fs::write(
        &svg,
        format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}"><rect width="{W}" height="{H}" fill="#4FE3FF"/><circle cx="32" cy="24" r="16" fill="#FFB347"/></svg>"##),
    )
    .unwrap();
    out.push(svg);
    out
}

#[test]
fn every_input_to_every_output() {
    let dir = tempfile::tempdir().unwrap();
    let reg = morph_engines::registry();
    let runner = Runner::new(&reg);
    let inputs = fixtures(dir.path());
    let outputs = ["jpg", "png", "webp", "tiff", "avif", "bmp", "gif", "pdf"];

    for input in &inputs {
        let before = hash(input);
        let in_ext = input.extension().unwrap().to_str().unwrap().to_string();
        for out in outputs {
            if out == in_ext {
                continue;
            }
            let job = Job::new(&format!("convert.{out}"), vec![input.clone()]);
            let report = runner.run(&job, |_| {}).unwrap();
            assert!(report.ok(), "{in_ext} → {out}: {:?}", report.errors);
            assert_eq!(report.outputs.len(), 1);
            let produced = &report.outputs[0];
            assert_eq!(produced.extension().unwrap(), out, "{in_ext} → {out}");
            assert_eq!(produced.parent(), input.parent(), "saved beside the original");
            if out == "pdf" {
                assert!(std::fs::read(produced).unwrap().starts_with(b"%PDF-"));
            } else if out != "avif" || morph_engines::image::heif::available() {
                let kind = morph_core::detect(produced).unwrap();
                let back = morph_engines::image::decode::decode(produced, &kind).unwrap();
                assert_eq!(back.dimensions(), (W, H), "{in_ext} → {out}");
            }
            assert_eq!(hash(input), before, "{in_ext} source changed by → {out}");
        }
    }
}

#[test]
fn second_run_numbers_the_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photo.jpg");
    morph_engines::image::encode::encode(&fixture_image(), "jpg", &src, &Default::default()).unwrap();
    let reg = morph_engines::registry();
    let runner = Runner::new(&reg);
    let job = Job::new("convert.png", vec![src.clone()]);
    let first = runner.run(&job, |_| {}).unwrap().outputs;
    let second = runner.run(&job, |_| {}).unwrap().outputs;
    assert_eq!(first[0].file_name().unwrap(), "photo.png");
    assert_eq!(second[0].file_name().unwrap(), "photo 2.png");
}

#[test]
fn many_images_make_one_pdf_and_batches_convert_each() {
    let dir = tempfile::tempdir().unwrap();
    let inputs = fixtures(dir.path());
    let reg = morph_engines::registry();
    let runner = Runner::new(&reg);

    let pdf = runner.run(&Job::new("convert.pdf", inputs.clone()), |_| {}).unwrap();
    assert_eq!(pdf.outputs.len(), 1);
    let doc = lopdf::Document::load(&pdf.outputs[0]).unwrap();
    assert_eq!(doc.get_pages().len(), inputs.len());

    let pngs: Vec<_> = inputs.iter().filter(|p| p.extension().unwrap() != "png").cloned().collect();
    let batch = runner.run(&Job::new("convert.webp", pngs.clone()), |_| {}).unwrap();
    assert!(batch.ok(), "{:?}", batch.errors);
    assert_eq!(batch.outputs.len(), pngs.len());
}

#[test]
fn every_registered_image_op_leaves_the_source_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("photo.png");
    morph_engines::image::encode::encode(&fixture_image(), "png", &src, &Default::default()).unwrap();
    let before = hash(&src);
    let kind = morph_core::detect(&src).unwrap();
    let reg = morph_engines::registry();
    let runner = Runner::new(&reg);
    for ring in [Ring::Convert, Ring::Tools] {
        for op in reg.for_selection(std::slice::from_ref(&kind), ring) {
            if op.spec().editor.is_some() || !op.available() {
                continue; // editors need user input; covered by their own tests
            }
            let job = Job::new(&op.spec().id, vec![src.clone()]).params(Params::new());
            let _ = runner.run(&job, |_| {});
            assert_eq!(hash(&src), before, "{} modified the source", op.spec().id);
        }
    }
}

#[test]
fn unsupported_input_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    let reg = morph_engines::registry();
    let err = Runner::new(&reg).run(&Job::new("convert.png", vec![txt]), |_| {}).unwrap_err();
    assert!(err.to_string().contains("can't take"), "{err}");
}
