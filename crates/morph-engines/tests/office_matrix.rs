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
                let from_md = run("doc.to-pdf", &md);
                assert!(morph_engines::pdf::page_count(&from_md).unwrap() >= 1);
                let docx = run("doc.to-docx", &md);
                let pdf = run("office.to-pdf", &docx);
                assert!(morph_engines::pdf::page_count(&pdf).unwrap() >= 1);
                // Slides → images (pandoc writes the deck).
                let pptx = d.path().join("deck.pptx");
                let ok = std::process::Command::new(morph_engines::office::pandoc().unwrap())
                    .arg(&md)
                    .arg("-o")
                    .arg(&pptx)
                    .status()
                    .unwrap()
                    .success();
                assert!(ok, "pandoc wrote the deck");
                let png = run("slides.to-png", &pptx);
                assert!(png.exists());
            }
        }
    }
}

#[test]
fn one_pdf_segment_per_document_kind() {
    let d = tempfile::tempdir().unwrap();
    let reg = morph_engines::registry();
    for (name, want) in [
        ("a.md", "doc.to-pdf"),
        ("a.txt", "doc.to-pdf"),
        ("a.html", "doc.to-pdf"),
        ("a.docx", "office.to-pdf"),
    ] {
        let f = d.path().join(name);
        std::fs::write(&f, b"x").unwrap();
        let kind = detect(&f).unwrap();
        let pdfs: Vec<String> = reg
            .for_selection(&[kind], Ring::Convert)
            .into_iter()
            .filter(|o| o.spec().label == "PDF")
            .map(|o| o.spec().id.clone())
            .collect();
        assert_eq!(pdfs, vec![want.to_string()], "{name}");
    }
}
