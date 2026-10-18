//! S13 format matrix. pandoc tests need the `pandoc` component; LibreOffice tests run
//! when LibreOffice is installed, and otherwise check that its segments report
//! INSTALL COMPONENT instead of failing.

use morph_core::{detect, Job, Params, Ring, Runner};
use std::io::Read;
use std::path::{Path, PathBuf};

fn run(op: &str, input: &Path) -> PathBuf {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(
            &Job::new(op, vec![input.to_path_buf()]).params(Params::new()),
            |_| {},
        )
        .unwrap();
    assert!(r.ok(), "{op} on {}: {:?}", input.display(), r.errors);
    r.outputs[0].clone()
}

fn zip_names(p: &Path) -> Vec<String> {
    let z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    z.file_names().map(str::to_string).collect()
}

fn zip_text(p: &Path, name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::fs::File::open(p).unwrap()).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

const MD: &str = "# Metamorphosis\n\nOriginals are **never** destroyed.\n\n- Offline\n- Non-destructive\n\n| Format | Ring |\n|---|---|\n| PNG | Convert |\n";

#[test]
fn documents_through_pandoc() {
    if morph_engines::office::pandoc().is_none() {
        eprintln!("pandoc not installed — skipping (morph component install pandoc)");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let md = d.path().join("notes.md");
    std::fs::write(&md, MD).unwrap();
    let docx = run("doc.to-docx", &md);
    assert!(zip_text(&docx, "word/document.xml").contains("never"));
    let html = run("doc.to-html", &docx);
    let h = std::fs::read_to_string(&html).unwrap();
    assert!(
        h.contains("<h1") && h.contains("<strong>never</strong>") && h.contains("<table"),
        "{h}"
    );
    let epub = run("doc.to-epub", &md);
    assert!(zip_names(&epub).iter().any(|n| n.ends_with(".xhtml")));
    let txt = run("doc.to-txt", &epub);
    assert!(std::fs::read_to_string(&txt)
        .unwrap()
        .contains("Originals are never destroyed"));
    let back = run("doc.to-md", &docx);
    assert!(std::fs::read_to_string(&back)
        .unwrap()
        .contains("# Metamorphosis"));
    let rtf = d.path().join("letter.rtf");
    std::fs::write(
        &rtf,
        r"{\rtf1\ansi{\fonttbl\f0\fswiss Helvetica;}\f0\pard Hello {\b bold} world.\par}",
    )
    .unwrap();
    let from_rtf = run("doc.to-md", &rtf);
    assert!(std::fs::read_to_string(&from_rtf)
        .unwrap()
        .contains("**bold**"));
}

#[test]
fn spreadsheets_round_trip() {
    let d = tempfile::tempdir().unwrap();
    let csv = d.path().join("data.csv");
    std::fs::write(
        &csv,
        "Name,Qty,Price,ZIP\nApple,3,1.25,02134\nPear,10,0.5,90210\n",
    )
    .unwrap();
    let xlsx = run("sheet.to-xlsx", &csv);
    let ods = run("sheet.to-ods", &xlsx);
    let tsv = run("sheet.to-tsv", &ods);
    let t = std::fs::read_to_string(&tsv).unwrap();
    assert_eq!(
        t.replace("\r\n", "\n"),
        "Name\tQty\tPrice\tZIP\nApple\t3\t1.25\t02134\nPear\t10\t0.5\t90210\n"
    );
    // XLS input path (calamine) is covered by reading the XLSX back:
    let sheets = morph_engines::office::sheets::read(&xlsx, "xlsx").unwrap();
    assert_eq!(sheets[0].rows.len(), 3);
}

#[test]
fn subtitles() {
    let d = tempfile::tempdir().unwrap();
    let srt = d.path().join("film.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHi\n").unwrap();
    let vtt = run("subs.to-vtt", &srt);
    assert!(std::fs::read_to_string(&vtt).unwrap().starts_with("WEBVTT"));
    let back = run("subs.to-srt", &vtt);
    assert!(std::fs::read_to_string(&back)
        .unwrap()
        .contains("00:00:01,000 --> 00:00:02,000"));
    assert_eq!(
        std::fs::read_to_string(run("subs.to-txt", &vtt))
            .unwrap()
            .trim(),
        "Hi"
    );
}

/// A 2-page PDF with a heading, a paragraph and a small table.
fn sample_pdf(dir: &Path) -> PathBuf {
    use lopdf::content::{Content, Operation as Op};
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let mut kids = vec![];
    for p in 1..=2 {
        let mut ops = vec![];
        let mut t = |size: i64, x: i64, y: i64, s: &str| {
            ops.extend([
                Op::new("BT", vec![]),
                Op::new("Tf", vec!["F1".into(), size.into()]),
                Op::new("Td", vec![x.into(), y.into()]),
                Op::new("Tj", vec![Object::string_literal(s)]),
                Op::new("ET", vec![]),
            ]);
        };
        t(24, 72, 760, &format!("Report {p}"));
        t(11, 72, 720, "Quarterly figures below.");
        for (r, (a, b, c)) in [
            ("Item", "Qty", "Total"),
            ("Apples", "3", "12"),
            ("Pears", "10", "40"),
        ]
        .iter()
        .enumerate()
        {
            let y = 660 - r as i64 * 20;
            t(11, 72, y, a);
            t(11, 250, y, b);
            t(11, 400, y, c);
        }
        let c = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().unwrap(),
        ));
        kids.push(Object::Reference(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => c,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        })));
    }
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", cat);
    let p = dir.join("report.pdf");
    doc.save(&p).unwrap();
    p
}

#[test]
fn pdf_to_pptx_epub_xlsx() {
    if !morph_engines::pdf::lib::available() {
        eprintln!("PDFium not found — skipping");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let pdf = sample_pdf(d.path());
    let pptx = run("pdf.to-pptx", &pdf);
    let names = zip_names(&pptx);
    assert!(
        names.contains(&"ppt/slides/slide2.xml".to_string())
            && !names.contains(&"ppt/slides/slide3.xml".to_string())
    );
    let epub = run("pdf.to-epub", &pdf);
    let names = zip_names(&epub);
    assert_eq!(names[0], "mimetype");
    assert!(zip_text(&epub, "OEBPS/chapter001.xhtml").contains("<h1>Report 1</h1>"));
    let xlsx = run("pdf.to-xlsx", &pdf);
    let sheets = morph_engines::office::sheets::read(&xlsx, "xlsx").unwrap();
    assert_eq!(sheets.len(), 2);
    let rows: Vec<String> = sheets[0].rows.iter().map(|r| format!("{r:?}")).collect();
    let apples = sheets[0]
        .rows
        .iter()
        .find(|r| {
            r.iter()
                .any(|c| *c == morph_engines::office::sheets::Cell::Text("Apples".into()))
        })
        .expect("Apples row");
    assert!(
        apples.contains(&morph_engines::office::sheets::Cell::Number(3.0)),
        "{rows:?}"
    );
    assert!(
        apples.contains(&morph_engines::office::sheets::Cell::Number(12.0)),
        "{rows:?}"
    );

    if morph_engines::office::soffice().is_some() {
        // The PPTX we write opens in LibreOffice.
        let back = run("office.to-pdf", &pptx);
        assert_eq!(morph_engines::pdf::page_count(&back).unwrap(), 2);
    }
}

#[test]
fn libreoffice_segments() {
    let d = tempfile::tempdir().unwrap();
    let reg = morph_engines::registry();
    let csv = d.path().join("t.csv");
    std::fs::write(&csv, "a,b\n1,2\n").unwrap();
    let kind = detect(&csv).unwrap();
    let pdf_op = reg
        .for_selection(&[kind], Ring::Convert)
        .into_iter()
        .find(|o| o.spec().id == "office.to-pdf")
        .expect("PDF segment for CSV");
    match morph_engines::office::soffice() {
        None => {
            assert!(!pdf_op.available(), "shows INSTALL COMPONENT");
            let err = Runner::new(&reg)
                .run(&Job::new("office.to-pdf", vec![csv]), |_| {})
                .unwrap_err();
            assert!(err.to_string().contains("libreoffice"), "{err}");
        }
        Some(_) => {
            let pdf = run("office.to-pdf", &csv);
            assert_eq!(morph_engines::pdf::page_count(&pdf).unwrap(), 1);
            let md = d.path().join("n.md");
            std::fs::write(&md, MD).unwrap();
            if morph_engines::office::pandoc().is_some() {
                let docx = run("doc.to-docx", &md);
                let pdf = run("office.to-pdf", &docx);
                assert!(morph_engines::pdf::page_count(&pdf).unwrap() >= 1);
                // Slides → images.
                let pptx = run("pdf.to-pptx", &pdf);
                let png = run("slides.to-png", &pptx);
                assert!(png.exists());
            }
        }
    }
}
