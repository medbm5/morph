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

fn run_err(op: &str, inputs: Vec<PathBuf>, params: Params) -> String {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg).run(&Job::new(op, inputs).params(params), |_| {});
    match r {
        Ok(r) => {
            assert!(!r.ok(), "{op} should fail");
            format!("{:?}", r.errors)
        }
        Err(e) => e.to_string(),
    }
}

#[test]
fn split_into_single_pages_and_chunks() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "S", 5, true);
    let dir = run("pdf.split", vec![src.clone()], Params::new());
    assert!(dir.is_dir());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Page 001.pdf",
            "Page 002.pdf",
            "Page 003.pdf",
            "Page 004.pdf",
            "Page 005.pdf"
        ]
    );
    assert_eq!(page_texts(&dir.join("Page 004.pdf")), vec!["S page 4"]);

    let dir = run(
        "pdf.split",
        vec![src.clone()],
        Params::new().with("every", 2),
    );
    assert_eq!(
        page_texts(&dir.join("Pages 001-002.pdf")),
        vec!["S page 1", "S page 2"]
    );
    assert_eq!(page_texts(&dir.join("Page 005.pdf")), vec!["S page 5"]);

    let err = run_err("pdf.split", vec![src], Params::new().with("every", 5));
    assert!(err.contains("nothing to split"), "{err}");
}

#[test]
fn protect_then_unlock() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "P", 2, false);

    let err = run_err(
        "pdf.protect",
        vec![src.clone()],
        Params::new()
            .with("password", "s3cret")
            .with("confirm", "typo"),
    );
    assert!(err.contains("don't match"), "{err}");

    let locked = run(
        "pdf.protect",
        vec![src.clone()],
        Params::new()
            .with("password", "s3cret")
            .with("confirm", "s3cret"),
    );
    assert_eq!(locked.file_name().unwrap(), "doc Protected.pdf");
    {
        // A real reader refuses it without the password and opens it with it.
        let pdfium = morph_engines::pdf::lib::pdfium().unwrap();
        assert!(pdfium.load_pdf_from_file(&locked, None).is_err());
        let doc = pdfium.load_pdf_from_file(&locked, Some("s3cret")).unwrap();
        assert_eq!(doc.pages().len(), 2);
    }

    let err = run_err(
        "pdf.unlock",
        vec![locked.clone()],
        Params::new().with("password", "nope"),
    );
    assert!(err.contains("wrong password"), "{err}");
    let open = run(
        "pdf.unlock",
        vec![locked],
        Params::new().with("password", "s3cret"),
    );
    assert_eq!(page_texts(&open), vec!["P page 1", "P page 2"]);
    assert!(!Document::load(&open).unwrap().is_encrypted());

    let err = run_err("pdf.unlock", vec![src], Params::new());
    assert!(err.contains("isn't password-protected"), "{err}");
}

#[test]
fn unlock_removes_owner_only_restrictions() {
    use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
    use lopdf::{EncryptionState, EncryptionVersion, Permissions};
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "R", 2, false);
    // Opens without a password, but printing and copying are locked.
    let mut doc = Document::load(&src).unwrap();
    let filter: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: std::collections::BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &[7u8; 32],
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: "owner",
        user_password: "",
        permissions: Permissions::empty(),
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    let restricted = d.path().join("restricted.pdf");
    doc.save(&restricted).unwrap();

    let open = run("pdf.unlock", vec![restricted], Params::new());
    assert!(!Document::load(&open).unwrap().is_encrypted());
    assert_eq!(page_texts(&open), vec!["R page 1", "R page 2"]);
}

/// Dark pixels in a box of the rendered page, as fractions of the displayed page
/// (x0, y0 from the top-left), so rotated pages are checked the way people see them.
fn ink(p: &Path, page: i32, (x0, y0, x1, y1): (f32, f32, f32, f32)) -> usize {
    use pdfium_render::prelude::*;
    let pdfium = morph_engines::pdf::lib::pdfium().unwrap();
    let doc = pdfium.load_pdf_from_file(p, None).unwrap();
    let pg = doc.pages().get(page).unwrap();
    let img = pg
        .render_with_config(&PdfRenderConfig::new().set_target_width(600))
        .unwrap()
        .as_image()
        .unwrap()
        .to_luma8();
    let (w, h) = (img.width() as f32, img.height() as f32);
    let mut n = 0;
    for y in (y0 * h) as u32..(y1 * h) as u32 {
        for x in (x0 * w) as u32..(x1 * w) as u32 {
            if img.get_pixel(x, y)[0] < 240 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn page_numbers_sit_upright_at_the_bottom() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "N", 3, true);
    // Page 2 turned sideways: its number must still read along the displayed bottom.
    let turned = run(
        "pdf.organize",
        vec![src],
        Params::new().with("pages", "1,2:90,3"),
    );
    let out = run(
        "pdf.page-numbers",
        vec![turned],
        Params::new().with("style", "Page 1 of 9"),
    );
    let texts = page_texts(&out);
    for (i, t) in texts.iter().enumerate() {
        assert!(t.contains(&format!("N page {}", i + 1)), "{t}");
        assert!(t.contains(&format!("Page {} of 3", i + 1)), "{t}");
    }
    for page in 0..3 {
        let bottom = ink(&out, page, (0.3, 0.9, 0.7, 1.0));
        let top = ink(&out, page, (0.3, 0.0, 0.7, 0.05));
        assert!(bottom > 20, "page {page}: number at the bottom ({bottom})");
        assert_eq!(top, 0, "page {page}: nothing at the top");
    }
}

#[test]
fn watermark_crosses_the_page() {
    if !pdfium_ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("doc.pdf");
    make(&src, "W", 2, false);
    let blank = ink(&src, 0, (0.3, 0.4, 0.7, 0.6));
    let out = run(
        "pdf.watermark",
        vec![src],
        Params::new().with("text", "DRAFT").with("opacity", 40),
    );
    assert_eq!(out.file_name().unwrap(), "doc Watermarked.pdf");
    for (i, t) in page_texts(&out).iter().enumerate() {
        assert!(
            t.contains(&format!("W page {}", i + 1)) && t.contains("DRAFT"),
            "{t}"
        );
    }
    let centre = ink(&out, 0, (0.3, 0.4, 0.7, 0.6));
    assert!(
        centre > blank + 200,
        "watermark across the middle ({blank} → {centre})"
    );

    let err = run_err(
        "pdf.watermark",
        vec![d.path().join("doc.pdf")],
        Params::new().with("text", "  "),
    );
    assert!(err.contains("watermark text"), "{err}");
}
