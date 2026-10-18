//! PDF engine (PDFium): render pages to images, extract text, Markdown and DOCX.

pub mod layout;
pub mod lib;

use crate::image::encode::{self, EncodeOptions};
use anyhow::{bail, Context, Result};
use layout::Block;
use morph_core::{OpContext, OpSpec, Operation, ParamKind, ParamSpec, Registry};
use pdfium_render::prelude::*;
use serde_json::json;
use std::path::PathBuf;

const PDF: &[&str] = &["application/pdf"];

pub fn register(reg: &mut Registry) {
    for (ext, label, order) in [("jpg", "JPG", 1), ("png", "PNG", 2)] {
        let mut spec = OpSpec::convert(ext, label, PDF)
            .order(order)
            .param(ParamSpec::new(
                "dpi",
                "DPI",
                ParamKind::Int { min: 36, max: 1200 },
                json!(300),
            ))
            .param(
                ParamSpec::new("pages", "Pages", ParamKind::Text, json!("all"))
                    .help("all, or ranges like 1-3,5"),
            );
        spec.id = format!("pdf.to-{ext}");
        if ext == "jpg" {
            spec = spec.param(ParamSpec::new(
                "quality",
                "Quality",
                ParamKind::Int { min: 1, max: 100 },
                json!(92),
            ));
        }
        reg.register(Render { spec, ext });
    }
    let mut txt = OpSpec::convert("txt", "TXT", PDF).order(3);
    txt.id = "pdf.to-txt".into();
    reg.register(Text {
        spec: txt,
        format: TextFormat::Plain,
    });
    let mut md = OpSpec::convert("md", "Markdown", PDF).order(4);
    md.id = "pdf.to-md".into();
    reg.register(Text {
        spec: md,
        format: TextFormat::Markdown,
    });
    let mut docx = OpSpec::convert("docx", "DOCX", PDF).order(5);
    docx.id = "pdf.to-docx".into();
    reg.register(Text {
        spec: docx,
        format: TextFormat::Docx,
    });
}

/// Number of pages in a PDF.
pub fn page_count(path: &std::path::Path) -> Result<usize> {
    let pdfium = lib::pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .context("opening PDF")?;
    Ok(doc.pages().len() as usize)
}

/// Parse `all` or `1-3,5` into 0-based page indices.
pub fn parse_pages(spec: &str, count: usize) -> Result<Vec<usize>> {
    let s = spec.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("all") {
        return Ok((0..count).collect());
    }
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: usize = a.parse().with_context(|| format!("bad page `{part}`"))?;
        let b: usize = if b.is_empty() {
            count
        } else {
            b.parse().with_context(|| format!("bad page `{part}`"))?
        };
        if a == 0 || b < a || a > count {
            bail!("page range `{part}` is outside 1–{count}");
        }
        out.extend((a - 1)..b.min(count));
    }
    out.dedup();
    Ok(out)
}

struct Render {
    spec: OpSpec,
    ext: &'static str,
}

impl Operation for Render {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn available(&self) -> bool {
        lib::available()
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let dpi = ctx.params.f64("dpi").unwrap_or(300.0) as f32;
        let opts = EncodeOptions {
            quality: ctx.params.u32("quality").unwrap_or(92) as u8,
            ..Default::default()
        };
        let pdfium = lib::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(input, None)
            .context("opening PDF")?;
        let count = doc.pages().len() as usize;
        let pages = parse_pages(ctx.params.str("pages").unwrap_or("all"), count)?;
        let cfg = PdfRenderConfig::new()
            .scale_page_by_factor(dpi / 72.0)
            .render_form_data(true);

        let render = |i: usize| -> Result<image::DynamicImage> {
            let page = doc.pages().get(i as _)?;
            let img = page.render_with_config(&cfg)?.as_image()?;
            // JPG has no alpha; PNG keeps the white page background PDFium paints.
            Ok(image::DynamicImage::ImageRgb8(img.to_rgb8()))
        };

        if pages.len() == 1 {
            let img = render(pages[0])?;
            let out = ctx.sink.file(input, self.ext)?;
            encode::encode(&img, self.ext, out.path(), &opts)?;
            ctx.progress.set(1.0);
            return Ok(vec![out.commit()?]);
        }

        // Several pages: `report/Page 001.png`, … in one new folder beside the PDF.
        let folder = ctx.sink.folder(input)?;
        let width = count.to_string().len().max(3);
        for (n, &i) in pages.iter().enumerate() {
            ctx.progress.check()?;
            let img = render(i)?;
            let name = format!("Page {:0width$}.{}", i + 1, self.ext);
            encode::encode(&img, self.ext, &folder.path().join(name), &opts)?;
            ctx.progress.message(
                (n + 1) as f32 / pages.len() as f32,
                &format!("Page {}", i + 1),
            );
        }
        Ok(vec![folder.commit()?])
    }
}

#[derive(Clone, Copy)]
enum TextFormat {
    Plain,
    Markdown,
    Docx,
}

struct Text {
    spec: OpSpec,
    format: TextFormat,
}

impl Operation for Text {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn available(&self) -> bool {
        lib::available()
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let pdfium = lib::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(input, None)
            .context("opening PDF")?;
        let (ext, bytes) = match self.format {
            TextFormat::Plain => {
                let mut s = String::new();
                let n = doc.pages().len();
                for (i, page) in doc.pages().iter().enumerate() {
                    ctx.progress.check()?;
                    if i > 0 {
                        s.push_str("\n\u{c}\n"); // form feed between pages
                    }
                    s.push_str(page.text()?.all().trim_end());
                    s.push('\n');
                    ctx.progress.set((i + 1) as f32 / n as f32);
                }
                ("txt", s.into_bytes())
            }
            TextFormat::Markdown => {
                let blocks = layout::blocks(&doc, false);
                ("md", layout::to_markdown(&blocks).into_bytes())
            }
            TextFormat::Docx => {
                let blocks = layout::blocks(&doc, true);
                ctx.progress.set(0.6);
                ("docx", docx(&blocks)?)
            }
        };
        let out = ctx.sink.file(input, ext)?;
        std::fs::write(out.path(), bytes)?;
        ctx.progress.set(1.0);
        Ok(vec![out.commit()?])
    }
}

/// Build a DOCX with headings, paragraphs, list items, images and page breaks.
fn docx(blocks: &[Block]) -> Result<Vec<u8>> {
    use docx_rs::*;
    let mut d = Docx::new()
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("Heading 1")
                .size(36)
                .bold(),
        )
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("Heading 2")
                .size(30)
                .bold(),
        )
        .add_style(
            Style::new("Heading3", StyleType::Paragraph)
                .name("Heading 3")
                .size(26)
                .bold(),
        );
    for b in blocks {
        d = match b {
            Block::Heading { level, text } => d.add_paragraph(
                Paragraph::new()
                    .style(&format!("Heading{level}"))
                    .add_run(Run::new().add_text(text)),
            ),
            Block::Paragraph(t) => {
                d.add_paragraph(Paragraph::new().add_run(Run::new().add_text(t)))
            }
            Block::ListItem { ordered: _, text } => d.add_paragraph(
                Paragraph::new()
                    .indent(Some(720), Some(SpecialIndentType::Hanging(360)), None, None)
                    .add_run(Run::new().add_text(format!("•\t{text}"))),
            ),
            Block::Image {
                png,
                width_pt,
                height_pt,
            } => {
                // Fit within a 6.5 in text column, 1 pt = 12700 EMU.
                let max_w = 6.5 * 72.0;
                let s = (max_w / width_pt).min(1.0);
                let pic = Pic::new(png).size(
                    (width_pt * s * 12700.0) as u32,
                    (height_pt * s * 12700.0) as u32,
                );
                d.add_paragraph(Paragraph::new().add_run(Run::new().add_image(pic)))
            }
            Block::PageBreak => {
                d.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)))
            }
        };
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    d.build().pack(&mut buf).context("writing DOCX")?;
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        assert_eq!(parse_pages("all", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_pages("1-2,3", 5).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_pages("4-", 5).unwrap(), vec![3, 4]);
        assert!(parse_pages("0", 5).is_err());
        assert!(parse_pages("7", 5).is_err());
    }
}
