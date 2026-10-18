//! S4: PDF → images, TXT, Markdown, DOCX. Needs the PDFium library
//! (`python3 scripts/fetch_pdfium.py`); tests are skipped with a notice otherwise.

use lopdf::content::{Content, Operation as Op};
use lopdf::{dictionary, Document, Object, Stream};
use morph_core::{Job, Params, Runner};
use std::io::Read;
use std::path::{Path, PathBuf};

/// A4 pages with a 24 pt heading, body paragraphs, a bullet list and an image.
fn fixture_pdf(path: &Path, pages: usize) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let bold = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold", "Encoding" => "WinAnsiEncoding" });
    // 64×48 RGB gradient image.
    let mut px = Vec::new();
    for y in 0..48u32 {
        for x in 0..64u32 {
            px.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, 200]);
        }
    }
    let img = doc.add_object(Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 64, "Height" => 48, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        px,
    ));
    let mut kids = vec![];
    for p in 1..=pages {
        let text = |ops: &mut Vec<Op>, f: &str, size: i64, x: i64, y: i64, s: &str| {
            ops.push(Op::new("BT", vec![]));
            ops.push(Op::new("Tf", vec![f.into(), size.into()]));
            ops.push(Op::new("Td", vec![x.into(), y.into()]));
            ops.push(Op::new("Tj", vec![Object::string_literal(s)]));
            ops.push(Op::new("ET", vec![]));
        };
        let mut ops = vec![];
        text(&mut ops, "F2", 24, 72, 770, &format!("Chapter {p}"));
        text(
            &mut ops,
            "F1",
            11,
            72,
            730,
            "Metamorphosis converts files without touching the original.",
        );
        text(
            &mut ops,
            "F1",
            11,
            72,
            716,
            "Every result is saved as a new file next to the source.",
        );
        text(
            &mut ops,
            "F1",
            11,
            72,
            680,
            &format!("This is the second paragraph of page {p}."),
        );
        text(&mut ops, "F1", 11, 72, 640, "\u{95} First point");
        text(&mut ops, "F1", 11, 72, 624, "\u{95} Second point");
        ops.push(Op::new("q", vec![]));
        ops.push(Op::new(
            "cm",
            vec![
                160.into(),
                0.into(),
                0.into(),
                120.into(),
                72.into(),
                450.into(),
            ],
        ));
        ops.push(Op::new("Do", vec!["Im1".into()]));
        ops.push(Op::new("Q", vec![]));
        text(&mut ops, "F1", 11, 72, 420, "Text after the image.");
        // Latin-1 bytes for the bullet (0x95 in WinAnsi) — lopdf writes the string as-is.
        let content = Content { operations: ops }.encode().unwrap();
        let cid = doc.add_object(Stream::new(dictionary! {}, fix_bullets(content)));
        let pid = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => cid,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font, "F2" => bold }, "XObject" => dictionary! { "Im1" => img } },
        });
        kids.push(Object::Reference(pid));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => pages as i64 },
        ),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    doc.save(path).unwrap();
}

/// `string_literal` UTF-8-encodes U+0095; turn it back into the single WinAnsi byte.
fn fix_bullets(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0xC2 && bytes.get(i + 1) == Some(&0x95) {
            out.push(0x95);
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn ready() -> bool {
    let ok = morph_engines::pdf::lib::available();
    if !ok {
        eprintln!("PDFium not found — skipping (run scripts/fetch_pdfium.py)");
    }
    ok
}

fn run(op: &str, input: &Path, params: Params) -> Vec<PathBuf> {
    let reg = morph_engines::registry();
    let report = Runner::new(&reg)
        .run(
            &Job::new(op, vec![input.to_path_buf()]).params(params),
            |_| {},
        )
        .unwrap();
    assert!(report.ok(), "{op}: {:?}", report.errors);
    report.outputs
}

#[test]
fn ten_pages_to_ten_pngs_at_300_dpi() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let pdf = d.path().join("report.pdf");
    fixture_pdf(&pdf, 10);
    let out = run("pdf.to-png", &pdf, Params::new());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file_name().unwrap(), "report");
    let mut files: Vec<_> = std::fs::read_dir(&out[0])
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files.len(), 10);
    assert_eq!(files[0], "Page 001.png");
    assert_eq!(files[9], "Page 010.png");
    let (w, h) = image::image_dimensions(out[0].join("Page 001.png")).unwrap();
    // A4 = 595×842 pt → 300 DPI = 2479×3508 px (±1 for rounding).
    assert!(
        (w as i32 - 2479).abs() <= 1 && (h as i32 - 3508).abs() <= 1,
        "{w}×{h}"
    );
}

#[test]
fn single_page_and_page_ranges() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let pdf = d.path().join("one.pdf");
    fixture_pdf(&pdf, 1);
    let out = run("pdf.to-jpg", &pdf, Params::new().with("dpi", 72));
    assert_eq!(out[0].file_name().unwrap(), "one.jpg");
    let pdf3 = d.path().join("three.pdf");
    fixture_pdf(&pdf3, 3);
    let out = run(
        "pdf.to-png",
        &pdf3,
        Params::new().with("pages", "2").with("dpi", 72),
    );
    assert_eq!(out[0].file_name().unwrap(), "three.png");
}

#[test]
fn text_markdown_and_docx() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let pdf = d.path().join("report.pdf");
    fixture_pdf(&pdf, 2);

    let txt = std::fs::read_to_string(&run("pdf.to-txt", &pdf, Params::new())[0]).unwrap();
    assert!(
        txt.contains("Chapter 1") && txt.contains("Chapter 2"),
        "{txt}"
    );
    assert!(txt.contains("without touching the original"));

    let md = std::fs::read_to_string(&run("pdf.to-md", &pdf, Params::new())[0]).unwrap();
    assert!(md.contains("# Chapter 1"), "{md}");
    assert!(md.contains("- First point"), "{md}");
    // Two lines of the first paragraph are joined into one paragraph.
    assert!(md.contains("original. Every result"), "{md}");

    let docx = run("pdf.to-docx", &pdf, Params::new());
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&docx[0]).unwrap()).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("Chapter 1") && xml.contains("Heading1"));
    assert!(xml.contains("Text after the image"));
    let media = (0..zip.len())
        .filter(|&i| zip.by_index(i).unwrap().name().starts_with("word/media/"))
        .count();
    assert!(media >= 2, "images embedded: {media}");
}

#[test]
fn images_to_pdf_round_trip() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let mut inputs = vec![];
    for i in 0..3 {
        let p = d.path().join(format!("img{i}.png"));
        image::RgbImage::from_pixel(40, 30, image::Rgb([i * 80, 100, 200]))
            .save(&p)
            .unwrap();
        inputs.push(p);
    }
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(&Job::new("convert.pdf", inputs), |_| {})
        .unwrap();
    assert_eq!(morph_engines::pdf::page_count(&r.outputs[0]).unwrap(), 3);
}
