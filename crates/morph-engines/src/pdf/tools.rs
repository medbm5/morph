//! PDF tools (S12): Compress, Merge, Split, Organize pages, Page numbers, Watermark,
//! Protect, Unlock — structure edits with lopdf, so text stays selectable and links keep
//! pointing at their pages.

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
        .order(2)
        .editor("pdf-merge"),
    });
    reg.register(Split {
        spec: OpSpec::tool("pdf.split", "Split", "Pages", PDF, OutputKind::Folder)
            .order(3)
            .editor("form")
            .quick()
            .param(ParamSpec::new(
                "every",
                "Pages per file",
                ParamKind::Int { min: 1, max: 1000 },
                json!(1),
            )),
    });
    reg.register(Organize {
        spec: OpSpec::tool("pdf.organize", "Organize", "Organized", PDF, OutputKind::Ext("pdf".into()))
            .order(4)
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
        .order(1)
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
    reg.register(Stamp {
        kind: StampKind::Numbers,
        spec: OpSpec::tool(
            "pdf.page-numbers",
            "Page numbers",
            "Numbered",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .order(5)
        .editor("form")
        .quick()
        .param(ParamSpec::new(
            "style",
            "Style",
            ParamKind::Choice {
                options: vec!["1".into(), "1 / 9".into(), "Page 1 of 9".into()],
            },
            json!("1"),
        ))
        .param(ParamSpec::new(
            "position",
            "Position",
            ParamKind::Choice {
                options: vec![
                    "bottom-center".into(),
                    "bottom-right".into(),
                    "top-right".into(),
                ],
            },
            json!("bottom-center"),
        ))
        .param(ParamSpec::new(
            "start",
            "First number",
            ParamKind::Int {
                min: 0,
                max: 100_000,
            },
            json!(1),
        )),
    });
    reg.register(Stamp {
        kind: StampKind::Watermark,
        spec: OpSpec::tool(
            "pdf.watermark",
            "Watermark",
            "Watermarked",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .order(6)
        .editor("form")
        .param(ParamSpec::new(
            "text",
            "Text",
            ParamKind::Text,
            json!("CONFIDENTIAL"),
        ))
        .param(ParamSpec::new(
            "opacity",
            "Opacity %",
            ParamKind::Int { min: 5, max: 100 },
            json!(20),
        )),
    });
    reg.register(Protect {
        spec: OpSpec::tool(
            "pdf.protect",
            "Protect",
            "Protected",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .order(7)
        .editor("form")
        .param(ParamSpec::new(
            "password",
            "Password",
            ParamKind::Password,
            json!(""),
        ))
        .param(ParamSpec::new(
            "confirm",
            "Confirm password",
            ParamKind::Password,
            json!(""),
        )),
    });
    reg.register(Unlock {
        spec: OpSpec::tool(
            "pdf.unlock",
            "Unlock",
            "Unlocked",
            PDF,
            OutputKind::Ext("pdf".into()),
        )
        .order(8)
        .editor("form")
        .param(
            ParamSpec::new("password", "Password", ParamKind::Password, json!(""))
                .help("Leave empty if the PDF opens without one (printing or copying locked)"),
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

struct Split {
    spec: OpSpec,
}

impl Operation for Split {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let doc = load(ctx.input())?;
        let count = doc.get_pages().len();
        let every = ctx.params.u32("every").unwrap_or(1).max(1) as usize;
        if count <= every {
            bail!("this PDF has {count} page(s): nothing to split");
        }
        let dir = ctx.sink.folder(ctx.input())?;
        let width = count.to_string().len().max(3);
        let chunks = count.div_ceil(every);
        for (n, first) in (0..count).step_by(every).enumerate() {
            ctx.progress.check()?;
            let last = (first + every).min(count);
            let order: Vec<PageRef> = (first..last)
                .map(|page| PageRef { page, rotate: 0 })
                .collect();
            let mut part = doc.clone();
            organize(&mut part, &order)?;
            part.prune_objects();
            let name = if last - first == 1 {
                format!("Page {:0width$}.pdf", first + 1)
            } else {
                format!("Pages {:0width$}-{:0width$}.pdf", first + 1, last)
            };
            save(&mut part, &dir.path().join(name))?;
            ctx.progress.set((n + 1) as f32 / chunks as f32);
        }
        Ok(vec![dir.commit()?])
    }
}

#[derive(Clone, Copy)]
enum StampKind {
    Numbers,
    Watermark,
}

/// Page numbers and watermarks: text drawn over each page in a new content stream.
struct Stamp {
    kind: StampKind,
    spec: OpSpec,
}

impl Operation for Stamp {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let mut doc = load(ctx.input())?;
        flatten_inheritance(&mut doc)?;
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let total = pages.len();
        let p = &ctx.params;
        let text = p.str("text").unwrap_or("CONFIDENTIAL").to_string();
        if matches!(self.kind, StampKind::Watermark) && text.trim().is_empty() {
            bail!("type the watermark text");
        }
        let name = |n: &str| Object::Name(n.as_bytes().to_vec());
        let mut f = Dictionary::new();
        f.set("Type", name("Font"));
        f.set("Subtype", name("Type1"));
        f.set("BaseFont", name("Helvetica"));
        f.set("Encoding", name("WinAnsiEncoding"));
        let font = doc.add_object(f);
        let alpha = p.u32("opacity").unwrap_or(20).clamp(5, 100) as f32 / 100.0;
        let mut g = Dictionary::new();
        g.set("Type", name("ExtGState"));
        g.set("ca", alpha);
        g.set("CA", alpha);
        let gs = doc.add_object(g);
        let start = p.u32("start").unwrap_or(1) as usize;
        for (i, &page) in pages.iter().enumerate() {
            ctx.progress.check()?;
            let geo = PageGeometry::of(&doc, page)?;
            let ops = match self.kind {
                StampKind::Numbers => {
                    let n = start + i;
                    let last = start + total - 1;
                    let label = match p.str("style").unwrap_or("1") {
                        "1 / 9" => format!("{n} / {last}"),
                        "Page 1 of 9" => format!("Page {n} of {last}"),
                        _ => n.to_string(),
                    };
                    let size = 10.0;
                    let width = text_width(&label, size);
                    let margin = 28.0;
                    let (x, y) = match p.str("position").unwrap_or("bottom-center") {
                        "bottom-right" => (geo.w - margin - width, margin),
                        "top-right" => (geo.w - margin - width, geo.h - margin - size),
                        _ => ((geo.w - width) / 2.0, margin),
                    };
                    format!(
                        "q {} cm 0 g BT /MorphF {size} Tf {x:.2} {y:.2} Td {} Tj ET Q",
                        geo.cm(),
                        pdf_text(&label)
                    )
                }
                StampKind::Watermark => {
                    let diag = (geo.w * geo.w + geo.h * geo.h).sqrt();
                    let size = (0.7 * diag / text_width(&text, 1.0)).min(160.0);
                    let width = text_width(&text, size);
                    let angle = geo.h.atan2(geo.w);
                    let (sin, cos) = angle.sin_cos();
                    format!(
                        "q {} cm /MorphG gs 0.5 g 1 0 0 1 {:.2} {:.2} cm {cos:.4} {sin:.4} {:.4} {cos:.4} 0 0 cm BT /MorphF {size:.2} Tf {:.2} {:.2} Td {} Tj ET Q",
                        geo.cm(),
                        geo.w / 2.0,
                        geo.h / 2.0,
                        -sin,
                        -width / 2.0,
                        -size / 3.0,
                        pdf_text(&text)
                    )
                }
            };
            add_resource(&mut doc, page, b"Font", "MorphF", font)?;
            add_resource(&mut doc, page, b"ExtGState", "MorphG", gs)?;
            overlay(&mut doc, page, ops.into_bytes())?;
            ctx.progress.set((i + 1) as f32 / total.max(1) as f32 * 0.8);
        }
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        save(&mut doc, out.path())?;
        Ok(vec![out.commit()?])
    }
}

/// The visible page box and rotation, for drawing upright on the displayed page.
struct PageGeometry {
    x0: f32,
    y0: f32,
    /// Width and height as displayed (after `/Rotate`).
    w: f32,
    h: f32,
    rotate: i64,
}

impl PageGeometry {
    fn of(doc: &Document, page: ObjectId) -> Result<Self> {
        let d = doc.get_dictionary(page)?;
        let bx = d
            .get(b"CropBox")
            .or_else(|_| d.get(b"MediaBox"))
            .and_then(Object::as_array)
            .context("page has no MediaBox")?;
        let n: Vec<f32> = bx
            .iter()
            .map(|o| match doc.dereference(o) {
                Ok((_, o)) => o
                    .as_float()
                    .or_else(|_| o.as_i64().map(|i| i as f32))
                    .unwrap_or(0.0),
                Err(_) => 0.0,
            })
            .collect();
        if n.len() < 4 {
            bail!("bad page box");
        }
        let (x0, y0) = (n[0].min(n[2]), n[1].min(n[3]));
        let (bw, bh) = ((n[2] - n[0]).abs(), (n[3] - n[1]).abs());
        let rotate = d
            .get(b"Rotate")
            .and_then(Object::as_i64)
            .unwrap_or(0)
            .rem_euclid(360);
        let (w, h) = if rotate % 180 == 0 {
            (bw, bh)
        } else {
            (bh, bw)
        };
        Ok(Self {
            x0,
            y0,
            w,
            h,
            rotate,
        })
    }

    /// `cm` operands mapping displayed coordinates (origin bottom-left) to page space.
    fn cm(&self) -> String {
        let (x0, y0) = (self.x0, self.y0);
        let (bw, bh) = if self.rotate % 180 == 0 {
            (self.w, self.h)
        } else {
            (self.h, self.w)
        };
        let m = match self.rotate {
            90 => [0.0, 1.0, -1.0, 0.0, x0 + bw, y0],
            180 => [-1.0, 0.0, 0.0, -1.0, x0 + bw, y0 + bh],
            270 => [0.0, -1.0, 1.0, 0.0, x0, y0 + bh],
            _ => [1.0, 0.0, 0.0, 1.0, x0, y0],
        };
        m.iter()
            .map(|v| format!("{v:.2}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Approximate Helvetica width (in points) of `s` at `size`.
fn text_width(s: &str, size: f32) -> f32 {
    s.chars()
        .map(|c| match c {
            ' ' | '/' | 'i' | 'l' | 'j' | 'I' | '.' | ',' => 0.278,
            'f' | 't' | 'r' => 0.333,
            'm' | 'w' => 0.833,
            'M' | 'W' => 0.9,
            c if c.is_ascii_digit() => 0.556,
            c if c.is_uppercase() => 0.68,
            _ => 0.556,
        })
        .sum::<f32>()
        * size
}

/// A hex string in WinAnsi (Latin-1 range; anything else becomes `?`).
fn pdf_text(s: &str) -> String {
    let hex: String = s
        .chars()
        .map(|c| {
            if (c as u32) < 256 {
                c as u32
            } else {
                '?' as u32
            }
        })
        .map(|b| format!("{b:02X}"))
        .collect();
    format!("<{hex}>")
}

/// Make `/category /name` point at `obj` in the page's resources.
fn add_resource(
    doc: &mut Document,
    page: ObjectId,
    category: &[u8],
    name: &str,
    obj: ObjectId,
) -> Result<()> {
    // Resources may be inline or shared by reference; categories likewise.
    let res = match doc.get_dictionary(page)?.get(b"Resources") {
        Ok(Object::Reference(id)) => Some(*id),
        Ok(Object::Dictionary(_)) => None,
        _ => {
            doc.get_dictionary_mut(page)?
                .set("Resources", Dictionary::new());
            None
        }
    };
    let cat_ref = {
        let res_dict = match res {
            Some(id) => doc.get_dictionary_mut(id)?,
            None => doc
                .get_dictionary_mut(page)?
                .get_mut(b"Resources")?
                .as_dict_mut()?,
        };
        match res_dict.get(category) {
            Ok(Object::Reference(id)) => Some(*id),
            Ok(Object::Dictionary(_)) => None,
            _ => {
                res_dict.set(category, Dictionary::new());
                None
            }
        }
    };
    let cat = match cat_ref {
        Some(id) => doc.get_dictionary_mut(id)?,
        None => {
            let res_dict = match res {
                Some(id) => doc.get_dictionary_mut(id)?,
                None => doc
                    .get_dictionary_mut(page)?
                    .get_mut(b"Resources")?
                    .as_dict_mut()?,
            };
            res_dict.get_mut(category)?.as_dict_mut()?
        }
    };
    cat.set(name, Object::Reference(obj));
    Ok(())
}

/// Draw `ops` over the page: the original content is wrapped in q/Q so its graphics
/// state can't leak into ours.
fn overlay(doc: &mut Document, page: ObjectId, ops: Vec<u8>) -> Result<()> {
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let close = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
    let stamp = doc.add_object(Stream::new(Dictionary::new(), ops));
    let d = doc.get_dictionary_mut(page)?;
    let mut contents = vec![Object::Reference(open)];
    match d.get(b"Contents") {
        Ok(Object::Array(a)) => contents.extend(a.iter().cloned()),
        Ok(o @ Object::Reference(_)) => contents.push(o.clone()),
        _ => {}
    }
    contents.extend([Object::Reference(close), Object::Reference(stamp)]);
    d.set("Contents", contents);
    Ok(())
}

struct Protect {
    spec: OpSpec,
}

impl Operation for Protect {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
        use lopdf::{EncryptionState, EncryptionVersion, Permissions};
        let password = ctx.params.str("password").unwrap_or("");
        if password.is_empty() {
            bail!("type a password");
        }
        if let Some(confirm) = ctx.params.get("confirm").and_then(Value::as_str) {
            if confirm != password {
                bail!("the passwords don't match");
            }
        }
        let mut doc = load(ctx.input())?;
        doc.compress();
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
        let filter: std::sync::Arc<dyn CryptFilter> = std::sync::Arc::new(Aes256CryptFilter);
        let state = EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: true,
            crypt_filters: std::collections::BTreeMap::from([(b"StdCF".to_vec(), filter)]),
            file_encryption_key: &key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: password,
            user_password: password,
            permissions: Permissions::all(),
        })
        .context("setting up encryption")?;
        doc.encrypt(&state).context("encrypting")?;
        ctx.progress.set(0.7);
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        // Plain cross-reference table: objects were encrypted one by one, so they
        // mustn't be packed into (unencrypted) object streams afterwards.
        let mut f = std::io::BufWriter::new(std::fs::File::create(out.path())?);
        doc.save_to(&mut f).context("writing PDF")?;
        drop(f);
        Ok(vec![out.commit()?])
    }
}

struct Unlock {
    spec: OpSpec,
}

impl Operation for Unlock {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let mut doc =
            Document::load(input).with_context(|| format!("opening {}", input.display()))?;
        if !doc.is_encrypted() {
            bail!("this PDF isn't password-protected");
        }
        let out = ctx.sink.file(input, "pdf")?;
        if doc.encryption_state.is_some() {
            // Opens without a password (only printing/copying locked): lopdf already
            // decrypted it, so dropping the security handler keeps everything else.
            if let Ok(id) = doc.trailer.get(b"Encrypt").and_then(Object::as_reference) {
                doc.objects.remove(&id);
            }
            doc.trailer.remove(b"Encrypt");
            doc.encryption_state = None;
            save(&mut doc, out.path())?;
        } else {
            // Needs the password to open: PDFium reads it, and its pages go into a new,
            // unprotected document.
            let password = ctx.params.str("password").unwrap_or("");
            if password.is_empty() {
                bail!("this PDF needs its password to open");
            }
            let pdfium = super::lib::pdfium()?;
            let src = pdfium
                .load_pdf_from_file(input, Some(password))
                .map_err(|e| match e {
                    pdfium_render::prelude::PdfiumError::PdfiumLibraryInternalError(
                        pdfium_render::prelude::PdfiumInternalError::PasswordError,
                    ) => anyhow::anyhow!("wrong password"),
                    e => anyhow::anyhow!("opening the PDF: {e}"),
                })?;
            ctx.progress.set(0.4);
            let mut copy = pdfium.create_new_pdf()?;
            copy.pages_mut().append(&src)?;
            copy.save_to_file(out.path())?;
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
