//! PDF tables → XLSX (experimental): words are placed into rows by their baseline and
//! into columns by clustering their left edges across the page. One sheet per page.

use super::sheets::{write_xlsx, Cell, Sheet};
use anyhow::{Context, Result};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use pdfium_render::prelude::*;
use std::path::PathBuf;

pub fn register(reg: &mut Registry) {
    let mut spec = OpSpec::convert("xlsx", "XLSX (beta)", &["application/pdf"]).order(9);
    spec.id = "pdf.to-xlsx".into();
    reg.register(PdfToXlsx { spec });
}

#[derive(Debug, Clone)]
struct Word {
    x: f32,
    y: f32,
    h: f32,
    text: String,
}

fn words(page: &PdfPage) -> Vec<Word> {
    let Ok(text) = page.text() else { return vec![] };
    let mut out: Vec<Word> = Vec::new();
    let mut cur: Option<(Word, f32)> = None; // word, right edge
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        let Ok(b) = ch.loose_bounds() else { continue };
        let (l, r, top, bottom) = (
            b.left().value,
            b.right().value,
            b.top().value,
            b.bottom().value,
        );
        let size = ch.scaled_font_size().value.max(1.0);
        let gap =
            |w: &Word, right: f32| (bottom - w.y).abs() > size * 0.5 || l - right > size * 0.6;
        if c.is_whitespace() || cur.as_ref().is_some_and(|(w, right)| gap(w, *right)) {
            if let Some((w, _)) = cur.take() {
                if !w.text.trim().is_empty() {
                    out.push(w);
                }
            }
            if c.is_whitespace() {
                continue;
            }
        }
        match cur.as_mut() {
            Some((w, right)) => {
                w.text.push(c);
                *right = r;
            }
            None => {
                cur = Some((
                    Word {
                        x: l,
                        y: bottom,
                        h: top - bottom,
                        text: c.to_string(),
                    },
                    r,
                ))
            }
        }
    }
    if let Some((w, _)) = cur {
        out.push(w);
    }
    out
}

fn page_table(page: &PdfPage) -> Vec<Vec<Cell>> {
    let mut ws = words(page);
    if ws.is_empty() {
        return vec![];
    }
    // Rows: top to bottom, grouping words whose baselines are within half a line.
    ws.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap());
    let mut rows: Vec<Vec<Word>> = Vec::new();
    for w in ws {
        match rows.last_mut() {
            Some(r) if (r[0].y - w.y).abs() < r[0].h.max(w.h) * 0.5 => r.push(w),
            _ => rows.push(vec![w]),
        }
    }
    // Columns: cluster left edges (words starting within ~1.5 em share a column).
    let mut starts: Vec<f32> = rows.iter().flatten().map(|w| w.x).collect();
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let em = rows
        .iter()
        .flatten()
        .map(|w| w.h)
        .fold(0.0, f32::max)
        .max(8.0);
    let mut cols: Vec<f32> = Vec::new();
    for s in starts {
        if cols.last().is_none_or(|c| s - c > em * 1.5) {
            cols.push(s);
        }
    }
    rows.into_iter()
        .map(|mut r| {
            r.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
            let mut cells = vec![String::new(); cols.len()];
            for w in r {
                let i = cols
                    .iter()
                    .rposition(|c| *c <= w.x + em * 0.75)
                    .unwrap_or(0);
                if !cells[i].is_empty() {
                    cells[i].push(' ');
                }
                cells[i].push_str(&w.text);
            }
            cells
                .into_iter()
                .map(|t| {
                    let n = t.replace([',', ' '], "");
                    match n.parse::<f64>() {
                        Ok(v) if !t.is_empty() => Cell::Number(v),
                        _ if t.is_empty() => Cell::Empty,
                        _ => Cell::Text(t),
                    }
                })
                .collect()
        })
        .collect()
}

struct PdfToXlsx {
    spec: OpSpec,
}

impl Operation for PdfToXlsx {
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
        let sheets: Vec<Sheet> = doc
            .pages()
            .iter()
            .enumerate()
            .map(|(i, p)| Sheet {
                name: format!("Page {}", i + 1),
                rows: page_table(&p),
            })
            .collect();
        let out = ctx.sink.file(ctx.input(), "xlsx")?;
        write_xlsx(&sheets, out.path())?;
        Ok(vec![out.commit()?])
    }
}
