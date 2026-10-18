//! PDF → EPUB 3: text reflowed with the PDF layout engine (headings, paragraphs,
//! lists), images kept, one chapter per page.

use crate::pdf::layout::{self, Block};
use anyhow::{Context, Result};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use std::io::Write;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

pub fn register(reg: &mut Registry) {
    let mut spec = OpSpec::convert("epub", "EPUB", &["application/pdf"]).order(7);
    spec.id = "pdf.to-epub".into();
    reg.register(PdfToEpub { spec });
}

struct PdfToEpub {
    spec: OpSpec,
}

impl Operation for PdfToEpub {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        crate::pdf::lib::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let pdfium = crate::pdf::lib::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(ctx.input(), None)
            .context("opening PDF")?;
        let blocks = layout::blocks(&doc, true);
        let title = morph_core::output::stem_of(ctx.input());
        let out = ctx.sink.file(ctx.input(), "epub")?;
        write_epub(&title, &blocks, out.path())?;
        Ok(vec![out.commit()?])
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Write an EPUB 3 book from layout blocks.
pub fn write_epub(title: &str, blocks: &[Block], path: &std::path::Path) -> Result<()> {
    // Split into chapters at page breaks.
    let mut chapters: Vec<Vec<&Block>> = vec![vec![]];
    for b in blocks {
        if matches!(b, Block::PageBreak) {
            chapters.push(vec![]);
        } else {
            chapters.last_mut().unwrap().push(b);
        }
    }
    chapters.retain(|c| !c.is_empty());
    if chapters.is_empty() {
        chapters.push(vec![]);
    }
    let f = std::fs::File::create(path)?;
    let mut z = zip::ZipWriter::new(f);
    // `mimetype` must come first and be stored uncompressed.
    z.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    z.write_all(b"application/epub+zip")?;
    let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("META-INF/container.xml", o)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)?;

    let mut manifest = String::new();
    let mut spine = String::new();
    let mut nav = String::new();
    let mut img_n = 0;
    for (ci, ch) in chapters.iter().enumerate() {
        let name = format!("chapter{:03}.xhtml", ci + 1);
        let mut body = String::new();
        let mut heading: Option<String> = None;
        let mut in_list: Option<bool> = None;
        for b in ch {
            let list = match b {
                Block::ListItem { ordered, .. } => Some(*ordered),
                _ => None,
            };
            if in_list != list {
                if let Some(o) = in_list {
                    body.push_str(if o { "</ol>" } else { "</ul>" });
                }
                if let Some(o) = list {
                    body.push_str(if o { "<ol>" } else { "<ul>" });
                }
                in_list = list;
            }
            match b {
                Block::Heading { level, text } => {
                    heading.get_or_insert_with(|| text.clone());
                    body.push_str(&format!("<h{level}>{}</h{level}>", esc(text)));
                }
                Block::Paragraph(t) => body.push_str(&format!("<p>{}</p>", esc(t))),
                Block::ListItem { text, .. } => body.push_str(&format!("<li>{}</li>", esc(text))),
                Block::Image { png, .. } => {
                    img_n += 1;
                    let file = format!("img{img_n:04}.png");
                    z.start_file(format!("OEBPS/{file}"), o)?;
                    z.write_all(png)?;
                    manifest.push_str(&format!(
                        r#"<item id="i{img_n}" href="{file}" media-type="image/png"/>"#
                    ));
                    body.push_str(&format!(r#"<p><img src="{file}" alt=""/></p>"#));
                }
                Block::PageBreak => {}
            }
        }
        if let Some(o) = in_list {
            body.push_str(if o { "</ol>" } else { "</ul>" });
        }
        let label = heading.unwrap_or_else(|| format!("Page {}", ci + 1));
        z.start_file(format!("OEBPS/{name}"), o)?;
        z.write_all(format!(r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>{}</title><style>img{{max-width:100%}}</style></head><body>{body}</body></html>"#, esc(&label)).as_bytes())?;
        manifest.push_str(&format!(
            r#"<item id="c{ci}" href="{name}" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{ci}"/>"#));
        nav.push_str(&format!(r#"<li><a href="{name}">{}</a></li>"#, esc(&label)));
    }
    z.start_file("OEBPS/nav.xhtml", o)?;
    z.write_all(format!(r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>{0}</title></head><body><nav epub:type="toc"><h1>{0}</h1><ol>{nav}</ol></nav></body></html>"#, esc(title)).as_bytes())?;
    let id = format!(
        "urn:morph:{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    z.start_file("OEBPS/content.opf", o)?;
    z.write_all(format!(r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">{id}</dc:identifier><dc:title>{}</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2000-01-01T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>{manifest}</manifest><spine>{spine}</spine></package>"#, esc(title)).as_bytes())?;
    z.finish()?;
    Ok(())
}
