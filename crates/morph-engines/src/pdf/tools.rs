//! PDF tools (S12): Merge, Organize pages, Compress — structure edits with lopdf,
//! so text stays selectable and links keep pointing at their pages.

use anyhow::{bail, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use morph_core::{Arity, OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Registry};
use serde_json::{json, Value};
use std::path::PathBuf;

const PDF: &[&str] = &["application/pdf"];
const INHERITABLE: &[&str] = &["Resources", "MediaBox", "CropBox", "Rotate"];

pub fn register(reg: &mut Registry) {
    reg.register(Merge {
        spec: OpSpec::tool(
            "pdf.merge",
            "Merge",
            "Merged",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .arity(Arity::All)
        .order(1)
        .editor("pdf-merge"),
    });
    reg.register(Organize {
        spec: OpSpec::tool("pdf.organize", "Organize", "Organized", PDF, OutputKind::Ext("pdf".into()))
            .order(2)
            .editor("pages")
            .param(
                ParamSpec::new("pages", "Pages", ParamKind::Text, json!(""))
                    .help("Order of pages, e.g. 3,1,2:90,2 (page:rotation; repeat to duplicate, omit to delete)"),
            ),
    });
    reg.register(Compress {
        spec: OpSpec::tool(
            "pdf.compress",
            "Compress",
            "Compressed",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .order(3)
        .param(
            ParamSpec::new(
                "level",
                "Level",
                ParamKind::Choice {
                    options: vec!["high".into(), "medium".into(), "low".into()],
                },
                json!("medium"),
            )
            .help("high = best quality, low = smallest file"),
        ),
    });
}

fn load(p: &std::path::Path) -> Result<Document> {
    let doc = Document::load(p).with_context(|| format!("opening {}", p.display()))?;
    if doc.is_encrypted() {
        bail!("{} is password-protected", p.display());
    }
    Ok(doc)
}

/// Copy attributes a page inherits from its parent `Pages` nodes onto the page itself,
/// so pages can be moved to another tree without losing resources or size.
fn flatten_inheritance(doc: &mut Document) -> Result<()> {
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for id in pages {
        let mut missing: Vec<(&str, Object)> = Vec::new();
        let page = doc.get_dictionary(id)?;
        let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
        let mut need: Vec<&str> = INHERITABLE
            .iter()
            .copied()
            .filter(|k| !page.has(k.as_bytes()))
            .collect();
        while let (Some(pid), false) = (parent, need.is_empty()) {
            let Ok(pd) = doc.get_dictionary(pid) else {
                break;
            };
            need.retain(|k| match pd.get(k.as_bytes()) {
                Ok(v) => {
                    missing.push((k, v.clone()));
                    false
                }
                Err(_) => true,
            });
            parent = pd.get(b"Parent").and_then(Object::as_reference).ok();
        }
        if !missing.is_empty() {
            let page = doc.get_dictionary_mut(id)?;
            for (k, v) in missing {
                page.set(k, v);
            }
        }
    }
    Ok(())
}

/// Replace the page tree with a single `Pages` node holding `kids`, and drop what's unused.
fn rebuild_tree(doc: &mut Document, kids: Vec<ObjectId>) -> Result<()> {
    let pages_id = doc
        .catalog()?
        .get(b"Pages")
        .and_then(Object::as_reference)?;
    for k in &kids {
        doc.get_dictionary_mut(*k)?.set("Parent", pages_id);
    }
    let count = kids.len() as i64;
    let mut pages = Dictionary::new();
    pages.set("Type", "Pages");
    pages.set(
        "Kids",
        kids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
    );
    pages.set("Count", count);
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    doc.prune_objects();
    Ok(())
}

fn save(doc: &mut Document, out: &std::path::Path) -> Result<()> {
    doc.compress();
    let mut f = std::io::BufWriter::new(std::fs::File::create(out)?);
    doc.save_with_options(
        &mut f,
        lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build(),
    )
    .context("writing PDF")?;
    Ok(())
}

/// Merge documents in order into one.
pub fn merge(paths: &[PathBuf]) -> Result<Document> {
    let mut out = Document::with_version("1.7");
    let mut max_id = 1;
    let mut kids = Vec::new();
    for p in paths {
        let mut d = load(p)?;
        flatten_inheritance(&mut d)?;
        d.renumber_objects_with(max_id);
        max_id = d.max_id + 1;
        let pages: Vec<ObjectId> = d.get_pages().into_values().collect();
        let skip: Vec<ObjectId> = d
            .objects
            .iter()
            .filter(|(_, o)| {
                o.as_dict().is_ok_and(|dict| {
                    dict.get(b"Type")
                        .and_then(Object::as_name)
                        .is_ok_and(|t| t == b"Catalog" || t == b"Pages")
                })
            })
            .map(|(id, _)| *id)
            .collect();
        for (id, obj) in d.objects {
            if !skip.contains(&id) {
                out.objects.insert(id, obj);
            }
        }
        kids.extend(pages);
    }
    out.max_id = max_id;
    let pages_id = out.new_object_id();
    for k in &kids {
        if let Ok(Object::Dictionary(d)) = out.get_object_mut(*k) {
            d.set("Parent", pages_id);
        }
    }
    let count = kids.len() as i64;
    let mut pages = Dictionary::new();
    pages.set("Type", "Pages");
    pages.set(
        "Kids",
        kids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
    );
    pages.set("Count", count);
    out.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", pages_id);
    let catalog_id = out.add_object(catalog);
    out.trailer.set("Root", catalog_id);
    out.prune_objects();
    Ok(out)
}

/// One output page: source page (0-based) and extra rotation in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRef {
    pub page: usize,
    pub rotate: i64,
}

/// `3,1,2:90,2` or a JSON list of `{page, rotate}` (1-based pages).
pub fn parse_order(v: Option<&Value>, count: usize) -> Result<Vec<PageRef>> {
    let mut out = Vec::new();
    match v {
        Some(Value::Array(a)) => {
            for e in a {
                let page = e
                    .get("page")
                    .and_then(Value::as_u64)
                    .context("page entry needs `page`")? as usize;
                let rotate = e.get("rotate").and_then(Value::as_i64).unwrap_or(0);
                out.push(PageRef { page, rotate });
            }
        }
        Some(Value::String(s)) if !s.trim().is_empty() => {
            for part in s.split(',') {
                let (p, r) = part.trim().split_once(':').unwrap_or((part.trim(), "0"));
                out.push(PageRef {
                    page: p
                        .trim()
                        .parse()
                        .with_context(|| format!("bad page `{part}`"))?,
                    rotate: r
                        .trim()
                        .parse()
                        .with_context(|| format!("bad rotation `{part}`"))?,
                });
            }
        }
        _ => out.extend((1..=count).map(|page| PageRef { page, rotate: 0 })),
    }
    for r in &mut out {
        if r.page == 0 || r.page > count {
            bail!("page {} is outside 1–{count}", r.page);
        }
        if r.rotate % 90 != 0 {
            bail!("rotation must be a multiple of 90°");
        }
        r.page -= 1;
    }
    if out.is_empty() {
        bail!("no pages left");
    }
    Ok(out)
}

pub fn organize(doc: &mut Document, order: &[PageRef]) -> Result<()> {
    flatten_inheritance(doc)?;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    // Snapshot the pages first so duplicates copy the original, not an already-rotated page.
    let originals: Vec<Dictionary> = pages
        .iter()
        .map(|p| doc.get_dictionary(*p).cloned())
        .collect::<lopdf::Result<_>>()?;
    let mut used = std::collections::HashSet::new();
    let mut kids = Vec::new();
    for r in order {
        let src = pages[r.page];
        let id = if used.insert(src) {
            src
        } else {
            // Duplicate: a new page dictionary sharing the same contents and resources.
            doc.add_object(originals[r.page].clone())
        };
        if r.rotate != 0 {
            let d = doc.get_dictionary_mut(id)?;
            let cur = d.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
            d.set("Rotate", (cur + r.rotate).rem_euclid(360));
        }
        kids.push(id);
    }
    rebuild_tree(doc, kids)
}

/// Downsample and re-encode embedded images. Returns how many were replaced.
pub fn compress_images(doc: &mut Document, max_side: u32, quality: u8) -> Result<usize> {
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| {
            o.as_stream().is_ok_and(|s| {
                s.dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|n| n == b"Image")
            })
        })
        .map(|(id, _)| *id)
        .collect();
    let mut replaced = 0;
    for id in ids {
        let Ok(Object::Stream(stream)) = doc.get_object(id) else {
            continue;
        };
        let Some(img) = decode_image(stream) else {
            continue;
        };
        let (w, h) = (img.width(), img.height());
        let scale = (max_side as f64 / w.max(h) as f64).min(1.0);
        let img = if scale < 1.0 {
            img.resize_exact(
                ((w as f64 * scale).round() as u32).max(1),
                ((h as f64 * scale).round() as u32).max(1),
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };
        let gray = matches!(img, image::DynamicImage::ImageLuma8(_));
        let mut jpeg = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality);
        if gray {
            img.to_luma8().write_with_encoder(enc)?;
        } else {
            img.to_rgb8().write_with_encoder(enc)?;
        }
        if jpeg.len() >= stream.content.len() {
            continue; // never make it bigger
        }
        let mut dict = stream.dict.clone();
        for k in ["Filter", "DecodeParms", "Decode", "Length"] {
            dict.remove(k.as_bytes());
        }
        dict.set("Filter", "DCTDecode");
        dict.set("Width", img.width() as i64);
        dict.set("Height", img.height() as i64);
        dict.set("BitsPerComponent", 8);
        dict.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
        let mut s = Stream::new(dict, jpeg);
        s.allows_compression = false;
        doc.objects.insert(id, Object::Stream(s));
        replaced += 1;
    }
    Ok(replaced)
}

/// Decode an image XObject we know how to re-encode (JPEG, or 8-bit Flate RGB/Gray).
fn decode_image(s: &Stream) -> Option<image::DynamicImage> {
    let d = &s.dict;
    if d.has(b"SMask")
        || d.has(b"Mask")
        || d.get(b"ImageMask")
            .and_then(Object::as_bool)
            .unwrap_or(false)
    {
        return None; // keep transparency exactly as it is
    }
    let filter = match d.get(b"Filter").ok()? {
        Object::Name(n) => n.clone(),
        Object::Array(a) if a.len() == 1 => a[0].as_name().ok()?.to_vec(),
        _ => return None,
    };
    let (w, h) = (
        d.get(b"Width").ok()?.as_i64().ok()? as u32,
        d.get(b"Height").ok()?.as_i64().ok()? as u32,
    );
    if filter == b"DCTDecode" {
        let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
        // CMYK JPEGs decode inverted with some producers: leave them alone.
        return matches!(
            img,
            image::DynamicImage::ImageRgb8(_) | image::DynamicImage::ImageLuma8(_)
        )
        .then_some(img);
    }
    if filter == b"FlateDecode"
        && d.get(b"BitsPerComponent").ok()?.as_i64().ok()? == 8
        && !d.has(b"DecodeParms")
    {
        let cs = d.get(b"ColorSpace").ok()?.as_name().ok()?;
        let raw = s.decompressed_content().ok()?;
        return match cs {
            b"DeviceRGB" if raw.len() >= (w * h * 3) as usize => {
                image::RgbImage::from_raw(w, h, raw[..(w * h * 3) as usize].to_vec())
                    .map(image::DynamicImage::ImageRgb8)
            }
            b"DeviceGray" if raw.len() >= (w * h) as usize => {
                image::GrayImage::from_raw(w, h, raw[..(w * h) as usize].to_vec())
                    .map(image::DynamicImage::ImageLuma8)
            }
            _ => None,
        };
    }
    None
}

struct Merge {
    spec: OpSpec,
}

impl Operation for Merge {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        if ctx.inputs.len() < 2 {
            bail!("select two or more PDFs to merge");
        }
        let mut doc = merge(&ctx.inputs)?;
        ctx.progress.set(0.7);
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        save(&mut doc, out.path())?;
        Ok(vec![out.commit()?])
    }
}

struct Organize {
    spec: OpSpec,
}

impl Operation for Organize {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let mut doc = load(ctx.input())?;
        let count = doc.get_pages().len();
        let order = parse_order(ctx.params.get("pages"), count)?;
        organize(&mut doc, &order)?;
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        save(&mut doc, out.path())?;
        Ok(vec![out.commit()?])
    }
}

struct Compress {
    spec: OpSpec,
}

impl Operation for Compress {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let mut doc = load(ctx.input())?;
        let (side, q) = match ctx.params.str("level").unwrap_or("medium") {
            "high" => (2400, 82),
            "low" => (1100, 55),
            _ => (1600, 70),
        };
        compress_images(&mut doc, side, q)?;
        ctx.progress.set(0.7);
        doc.prune_objects();
        doc.delete_zero_length_streams();
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        save(&mut doc, out.path())?;
        // Already optimal: keep a byte-identical copy rather than a bigger file.
        if std::fs::metadata(out.path())?.len() > std::fs::metadata(ctx.input())?.len() {
            std::fs::copy(ctx.input(), out.path())?;
        }
        Ok(vec![out.commit()?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_syntax() {
        let o = parse_order(Some(&json!("3,1,2:90,2")), 3).unwrap();
        assert_eq!(
            o,
            vec![
                PageRef { page: 2, rotate: 0 },
                PageRef { page: 0, rotate: 0 },
                PageRef {
                    page: 1,
                    rotate: 90
                },
                PageRef { page: 1, rotate: 0 },
            ]
        );
        assert_eq!(parse_order(None, 2).unwrap().len(), 2);
        assert!(parse_order(Some(&json!("4")), 3).is_err());
        assert!(parse_order(Some(&json!("1:45")), 3).is_err());
        let j = parse_order(Some(&json!([{ "page": 2, "rotate": 270 }])), 2).unwrap();
        assert_eq!(
            j,
            vec![PageRef {
                page: 1,
                rotate: 270
            }]
        );
    }
}
