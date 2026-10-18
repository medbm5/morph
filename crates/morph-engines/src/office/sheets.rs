//! Spreadsheets: read XLSX/XLS/ODS (calamine) and CSV/TSV; write XLSX, ODS, CSV, TSV.
//! PDF export goes through LibreOffice (`office.to-pdf`).

use anyhow::{bail, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SHEETS: &[&str] = &["ext:xlsx", "ext:xls", "ext:ods", "ext:csv", "ext:tsv"];

pub fn register(reg: &mut Registry) {
    for (ext, label, order) in [
        ("xlsx", "XLSX", 1),
        ("ods", "ODS", 2),
        ("csv", "CSV", 3),
        ("tsv", "TSV", 4),
    ] {
        let mut spec = OpSpec::convert(ext, label, SHEETS).order(order);
        spec.id = format!("sheet.to-{ext}");
        reg.register(SheetConvert { spec, ext });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64),
    Bool(bool),
}

impl Cell {
    fn as_text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(s) => s.clone(),
            Cell::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Cell::Number(n) => n.to_string(),
            Cell::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

pub fn read(path: &Path, ext: &str) -> Result<Vec<Sheet>> {
    match ext {
        "csv" | "tsv" => {
            let mut r = csv::ReaderBuilder::new()
                .delimiter(if ext == "tsv" {
                    b'\t'
                } else {
                    sniff_delimiter(path)
                })
                .has_headers(false)
                .flexible(true)
                .from_path(path)?;
            let rows = r
                .records()
                .map(|rec| rec.map(|rec| rec.iter().map(parse_cell).collect()))
                .collect::<Result<Vec<Vec<Cell>>, _>>()?;
            Ok(vec![Sheet {
                name: morph_core::output::stem_of(path),
                rows,
            }])
        }
        _ => {
            let mut wb =
                open_workbook_auto(path).with_context(|| format!("opening {}", path.display()))?;
            let names = wb.sheet_names().to_vec();
            let mut out = Vec::new();
            for name in names {
                let range = wb.worksheet_range(&name)?;
                let rows = range
                    .rows()
                    .map(|r| {
                        r.iter()
                            .map(|c| match c {
                                Data::Empty => Cell::Empty,
                                Data::String(s) => Cell::Text(s.clone()),
                                Data::Float(f) => Cell::Number(*f),
                                Data::Int(i) => Cell::Number(*i as f64),
                                Data::Bool(b) => Cell::Bool(*b),
                                Data::DateTime(d) => Cell::Number(d.as_f64()),
                                Data::DateTimeIso(s) | Data::DurationIso(s) => {
                                    Cell::Text(s.clone())
                                }
                                Data::Error(e) => Cell::Text(format!("#{e:?}")),
                            })
                            .collect()
                    })
                    .collect();
                out.push(Sheet { name, rows });
            }
            Ok(out)
        }
    }
}

/// `;` is common in European CSV exports.
fn sniff_delimiter(path: &Path) -> u8 {
    let head = std::fs::read(path)
        .map(|b| b.into_iter().take(4096).collect::<Vec<_>>())
        .unwrap_or_default();
    let line = head.split(|&c| c == b'\n').next().unwrap_or(&[]);
    let count = |d: u8| line.iter().filter(|&&c| c == d).count();
    if count(b';') > count(b',') {
        b';'
    } else {
        b','
    }
}

fn parse_cell(s: &str) -> Cell {
    let t = s.trim();
    if t.is_empty() {
        Cell::Empty
    } else if let Ok(n) = t.parse::<f64>() {
        if t.starts_with('0') && t.len() > 1 && !t.starts_with("0.") {
            Cell::Text(s.into()) // keep leading zeros (IDs, ZIP codes)
        } else {
            Cell::Number(n)
        }
    } else {
        match t.to_ascii_uppercase().as_str() {
            "TRUE" => Cell::Bool(true),
            "FALSE" => Cell::Bool(false),
            _ => Cell::Text(s.into()),
        }
    }
}

pub fn write_xlsx(sheets: &[Sheet], path: &Path) -> Result<()> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    for s in sheets {
        let ws = wb.add_worksheet();
        let name: String = s
            .name
            .chars()
            .filter(|c| !"[]:*?/\\".contains(*c))
            .take(31)
            .collect();
        if !name.is_empty() {
            let _ = ws.set_name(&name);
        }
        for (r, row) in s.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let (r, c) = (r as u32, c as u16);
                match cell {
                    Cell::Empty => {}
                    Cell::Text(t) => {
                        ws.write_string(r, c, t)?;
                    }
                    Cell::Number(n) => {
                        ws.write_number(r, c, *n)?;
                    }
                    Cell::Bool(b) => {
                        ws.write_boolean(r, c, *b)?;
                    }
                }
            }
        }
    }
    wb.save(path)?;
    Ok(())
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Minimal OpenDocument spreadsheet (values only).
pub fn write_ods(sheets: &[Sheet], path: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;
    let f = std::fs::File::create(path)?;
    let mut z = zip::ZipWriter::new(f);
    z.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    z.write_all(b"application/vnd.oasis.opendocument.spreadsheet")?;
    let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("META-INF/manifest.xml", o)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#)?;
    let mut c = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2"><office:body><office:spreadsheet>"#,
    );
    for s in sheets {
        c.push_str(&format!(r#"<table:table table:name="{}">"#, xml(&s.name)));
        for row in &s.rows {
            c.push_str("<table:table-row>");
            for cell in row {
                match cell {
                    Cell::Empty => c.push_str("<table:table-cell/>"),
                    Cell::Number(n) => c.push_str(&format!(r#"<table:table-cell office:value-type="float" office:value="{n}"><text:p>{n}</text:p></table:table-cell>"#)),
                    Cell::Bool(b) => c.push_str(&format!(r#"<table:table-cell office:value-type="boolean" office:boolean-value="{b}"><text:p>{}</text:p></table:table-cell>"#, cell.as_text())),
                    Cell::Text(t) => c.push_str(&format!(r#"<table:table-cell office:value-type="string"><text:p>{}</text:p></table:table-cell>"#, xml(t))),
                }
            }
            c.push_str("</table:table-row>");
        }
        c.push_str("</table:table>");
    }
    c.push_str("</office:spreadsheet></office:body></office:document-content>");
    z.start_file("content.xml", o)?;
    z.write_all(c.as_bytes())?;
    z.finish()?;
    Ok(())
}

pub fn write_delimited(sheet: &Sheet, path: &Path, tab: bool) -> Result<()> {
    let mut w = csv::WriterBuilder::new()
        .delimiter(if tab { b'\t' } else { b',' })
        .flexible(true)
        .from_path(path)?;
    for row in &sheet.rows {
        w.write_record(row.iter().map(Cell::as_text))?;
    }
    w.flush()?;
    Ok(())
}

struct SheetConvert {
    spec: OpSpec,
    ext: &'static str,
}

impl Operation for SheetConvert {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let sheets = read(input, &ctx.kind().ext)?;
        if sheets.is_empty() {
            bail!("no sheets");
        }
        match self.ext {
            "xlsx" | "ods" => {
                let out = ctx.sink.file(input, self.ext)?;
                if self.ext == "xlsx" {
                    write_xlsx(&sheets, out.path())?;
                } else {
                    write_ods(&sheets, out.path())?;
                }
                Ok(vec![out.commit()?])
            }
            ext => {
                let tab = ext == "tsv";
                let used: Vec<&Sheet> = sheets
                    .iter()
                    .filter(|s| s.rows.iter().any(|r| r.iter().any(|c| *c != Cell::Empty)))
                    .collect();
                if used.len() <= 1 {
                    let out = ctx.sink.file(input, ext)?;
                    write_delimited(used.first().copied().unwrap_or(&sheets[0]), out.path(), tab)?;
                    return Ok(vec![out.commit()?]);
                }
                // One file per sheet, in a folder.
                let dir = ctx.sink.folder(input)?;
                for s in used {
                    let name = morph_core::naming::sanitize(&s.name);
                    write_delimited(s, &dir.path().join(format!("{name}.{ext}")), tab)?;
                }
                Ok(vec![dir.commit()?])
            }
        }
    }
}
