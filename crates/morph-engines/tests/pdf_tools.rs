//! S12: merge keeps page count and links; organize round-trips with selectable text;
//! compress shrinks image-heavy PDFs.

use lopdf::content::{Content, Operation as Op};
use lopdf::{dictionary, Document, Object, Stream};
use morph_core::{Job, Params, Runner};
use std::path::{Path, PathBuf};

/// A PDF whose pages say "<tag> page N", with a URI link and a link to page 2.
/// `inherit` puts Resources and MediaBox on the Pages node (inherited by pages).
fn make(path: &Path, tag: &str, pages: usize, inherit: bool) {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let resources = dictionary! { "Font" => dictionary! { "F1" => font } };
    let mut page_ids = Vec::new();
    for n in 1..=pages {
        let ops = vec![
            Op::new("BT", vec![]),
            Op::new("Tf", vec!["F1".into(), 24.into()]),
            Op::new("Td", vec![72.into(), 700.into()]),
            Op::new(
                "Tj",
                vec![Object::string_literal(format!("{tag} page {n}"))],
            ),
            Op::new("ET", vec![]),
        ];
        let c = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().unwrap(),
        ));
        let mut page = dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => c };
        if !inherit {
            page.set("Resources", resources.clone());
            page.set("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
        }
        page_ids.push(doc.add_object(page));
    }
    // Links on page 1: external URI and an internal jump to the last page.
    let uri = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![72.into(), 650.into(), 300.into(), 680.into()],
        "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal("https://example.org/") },
    });
    let goto = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![72.into(), 600.into(), 300.into(), 630.into()],
        "Dest" => vec![Object::Reference(*page_ids.last().unwrap()), "Fit".into()],
    });
    doc.get_dictionary_mut(page_ids[0])
        .unwrap()
        .set("Annots", vec![uri.into(), goto.into()]);
    let mut root = dictionary! { "Type" => "Pages", "Kids" => page_ids.iter().map(|&i| i.into()).collect::<Vec<Object>>(), "Count" => pages as i64 };
    if inherit {
        root.set("Resources", resources);
        root.set("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
    }
    doc.objects.insert(pages_id, Object::Dictionary(root));
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    doc.save(path).unwrap();
}

fn run(op: &str, inputs: Vec<PathBuf>, params: Params) -> PathBuf {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(&Job::new(op, inputs).params(params), |_| {})
        .unwrap();
    assert!(r.ok(), "{op}: {:?}", r.errors);
    r.outputs[0].clone()
}

/// Text of each page as PDFium sees it (i.e. selectable text).
fn page_texts(p: &Path) -> Vec<String> {
    let pdfium = morph_engines::pdf::lib::pdfium().unwrap();
    let doc = pdfium.load_pdf_from_file(p, None).unwrap();
    // Character by character (what text selection uses): `all()` clips to the page box,
    // which is in rotated coordinates on rotated pages.
    doc.pages()
        .iter()
        .map(|pg| {
            let t = pg.text().unwrap();
            t.chars()
                .iter()
                .filter_map(|c| c.unicode_char())
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect()
}

fn pdfium_ready() -> bool {
    let ok = morph_engines::pdf::lib::available();
    if !ok {
        eprintln!("PDFium not found — skipping");
    }
    ok
}

#[test]
fn merge_preserves_pages_and_links() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (a, b, c) = (
        d.path().join("a.pdf"),
        d.path().join("b.pdf"),
        d.path().join("c.pdf"),
    );
    make(&a, "A", 2, false);
    make(&b, "B", 3, true);
    make(&c, "C", 1, false);
    let out = run("pdf.merge", vec![a, b, c], Params::new());
    assert_eq!(out.file_name().unwrap(), "a Merged.pdf");
    let texts = page_texts(&out);
    assert_eq!(
        texts,
        vec!["A page 1", "A page 2", "B page 1", "B page 2", "B page 3", "C page 1"]
    );

    let doc = Document::load(&out).unwrap();
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    let mut uris = 0;
    let mut gotos = Vec::new();
    for pid in &pages {
        let Ok(annots) = doc
            .get_dictionary(*pid)
            .unwrap()
            .get(b"Annots")
            .and_then(Object::as_array)
        else {
            continue;
        };
        for a in annots {
            let ad = doc.get_dictionary(a.as_reference().unwrap()).unwrap();
            if ad.has(b"A") {
                uris += 1;
            }
            if let Ok(dest) = ad.get(b"Dest").and_then(Object::as_array) {
                gotos.push(dest[0].as_reference().unwrap());
            }
        }
    }
    assert_eq!(uris, 3, "URI links kept");
    assert_eq!(gotos.len(), 3, "internal links kept");
    // Each internal link still points at a page of the merged document: A→2, B→5, C→6.
    let idx: Vec<usize> = gotos
        .iter()
        .map(|g| pages.iter().position(|p| p == g).expect("dest is a page") + 1)
        .collect();
    assert_eq!(idx, vec![2, 5, 6]);
}

#[test]
fn organize_round_trips_with_selectable_text() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "D", 3, true);
    let out = run(
        "pdf.organize",
        vec![src.clone()],
        Params::new().with("pages", "3,1,2:90,2"),
    );
    assert_eq!(
        page_texts(&out),
        vec!["D page 3", "D page 1", "D page 2", "D page 2"]
    );
    let doc = Document::load(&out).unwrap();
    let rot: Vec<i64> = doc
        .get_pages()
        .into_values()
        .map(|p| {
            doc.get_dictionary(p)
                .unwrap()
                .get(b"Rotate")
                .and_then(Object::as_i64)
                .unwrap_or(0)
        })
        .collect();
    assert_eq!(rot, vec![0, 0, 90, 0]);
    // Back to the original order: identical text again.
    let back = run(
        "pdf.organize",
        vec![out],
        Params::new().with("pages", "2,3:-90"),
    );
    assert_eq!(page_texts(&back), vec!["D page 1", "D page 2"]);
}

#[test]
fn compress_shrinks_image_heavy_pdfs() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let png = d.path().join("photo.jpg");
    let mut s = 1u32;
    let img = image::RgbImage::from_fn(3000, 2000, |x, y| {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        image::Rgb([(x / 12 % 256) as u8, (y / 9 % 256) as u8, (s >> 26) as u8])
    });
    let mut f = std::fs::File::create(&png).unwrap();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 97)
        .encode_image(&img)
        .unwrap();
    drop(f);
    let pdf = run("convert.pdf", vec![png.clone(), png], Params::new());
    let before = std::fs::metadata(&pdf).unwrap().len();
    let out = run(
        "pdf.compress",
        vec![pdf],
        Params::new().with("level", "medium"),
    );
    let after = std::fs::metadata(&out).unwrap().len();
    eprintln!("{before} → {after} bytes");
    assert!(
        after * 3 < before,
        "expected a much smaller PDF: {before} → {after}"
    );
    assert_eq!(morph_engines::pdf::page_count(&out).unwrap(), 2);
}
