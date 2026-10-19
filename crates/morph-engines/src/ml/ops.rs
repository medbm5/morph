//! Redact (manual areas + local face/text detection), Remove BG, Extract text,
//! Searchable PDF.

use super::{bg, face, text, Region};
use crate::image::compose::{self, Source};
use crate::image::encode::{self, EncodeOptions};
use crate::image::tools::tool_output_ext;
use crate::image::INPUTS;
use anyhow::{Context, Result};
use morph_core::{
    OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Params, Registry,
};
use serde_json::{json, Value};
use std::path::PathBuf;

fn choice(v: &[&str]) -> ParamKind {
    ParamKind::Choice {
        options: v.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn register(reg: &mut Registry) {
    reg.register(Redact {
        spec: OpSpec::tool(
            "image.redact",
            "Redact",
            "Redacted",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(8)
        .editor("redact")
        .param(ParamSpec::new("areas", "Areas", ParamKind::Json, json!([])))
        .param(
            ParamSpec::new(
                "auto",
                "Detect",
                choice(&["none", "faces", "text", "faces+text"]),
                json!("none"),
            )
            .help("Find faces and/or text locally (needs the `ml` component)"),
        )
        .param(ParamSpec::new(
            "style",
            "Style",
            choice(&["solid", "blur"]),
            json!("solid"),
        ))
        .param(ParamSpec::new(
            "color",
            "Colour",
            ParamKind::Color,
            json!("#000000"),
        )),
    });
    reg.register(RemoveBg {
        spec: OpSpec::tool(
            "image.removebg",
            "Remove BG",
            "No BG",
            INPUTS,
            OutputKind::Ext("png".into()),
        )
        .order(7)
        .component("bgremove"),
    });
    reg.register(ExtractText {
        spec: OpSpec::tool(
            "image.ocr",
            "Copy text",
            "Text",
            INPUTS,
            OutputKind::Ext("txt".into()),
        )
        .order(9)
        .component("ocr"),
    });
    reg.register(SearchablePdf {
        spec: OpSpec::tool(
            "image.searchable-pdf",
            "Searchable PDF",
            "OCR",
            INPUTS,
            OutputKind::Ext("pdf".into()),
        )
        .order(11)
        .component("ocr"),
    });
}

/// Regions to redact: manual areas plus whatever `auto` asks the detectors for.
pub fn redact_regions(img: &image::DynamicImage, p: &Params) -> Result<Vec<Region>> {
    use image::GenericImageView;
    let (w, h) = img.dimensions();
    let mut out: Vec<Region> = Vec::new();
    if let Some(Value::Array(a)) = p.get("areas") {
        for v in a {
            let f = |k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
            if f("w") > 0.0 && f("h") > 0.0 {
                out.push(Region {
                    x: f("x"),
                    y: f("y"),
                    w: f("w"),
                    h: f("h"),
                    score: 1.0,
                });
            }
        }
    }
    let auto = p.str("auto").unwrap_or("none");
    if auto.contains("faces") {
        out.extend(face::detect(img)?.into_iter().map(|r| r.grow(0.12, w, h)));
    }
    if auto.contains("text") {
        out.extend(text::detect(img)?.into_iter().map(|r| r.grow(0.05, w, h)));
    }
    Ok(out)
}

struct Redact {
    spec: OpSpec,
}

impl Operation for Redact {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let src = Source::load(ctx.input(), ctx.kind())?;
        let regions = redact_regions(&src.img, &ctx.params)?;
        ctx.progress.set(0.5);
        let kind = if ctx.params.str("style") == Some("blur") {
            "blur"
        } else {
            "redact"
        };
        let color = ctx.params.str("color").unwrap_or("#000000").to_string();
        let shapes: Vec<Value> = regions
            .iter()
            .map(|r| {
                json!({ "type": kind, "x": r.x, "y": r.y, "w": r.w, "h": r.h, "color": color,
                             // Strong blur: unreadable, not just soft.
                             "strength": (r.w.min(r.h) / 3.0).max(8.0) })
            })
            .collect();
        let scene = compose::annotate(&src, &Value::Array(shapes))?;
        let img = compose::render(&scene, std::slice::from_ref(&src.img))?;
        let ext = tool_output_ext(ctx.kind());
        let out = ctx.sink.file(ctx.input(), ext)?;
        encode::encode(
            &img,
            ext,
            out.path(),
            &EncodeOptions {
                quality: 95,
                ..Default::default()
            },
        )?;
        Ok(vec![out.commit()?])
    }
}

struct RemoveBg {
    spec: OpSpec,
}

impl Operation for RemoveBg {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        super::available() && morph_core::components::is_installed("bgremove")
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let img = crate::image::decode::decode(ctx.input(), ctx.kind())?;
        let cut = bg::remove(&img)?;
        let out = ctx.sink.file(ctx.input(), "png")?;
        encode::encode(&cut, "png", out.path(), &EncodeOptions::default())?;
        Ok(vec![out.commit()?])
    }
}

struct ExtractText {
    spec: OpSpec,
}

impl Operation for ExtractText {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        super::available() && morph_core::components::is_installed("ocr")
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let img = crate::image::decode::decode(ctx.input(), ctx.kind())?;
        let t = text::ocr_text(&img)?;
        let out = ctx.sink.file(ctx.input(), "txt")?;
        std::fs::write(out.path(), t + "\n")?;
        Ok(vec![out.commit()?])
    }
}

struct SearchablePdf {
    spec: OpSpec,
}

impl Operation for SearchablePdf {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        super::available() && morph_core::components::is_installed("ocr")
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let img = crate::image::decode::decode(ctx.input(), ctx.kind())?;
        let words = text::ocr(&img)?;
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        searchable_pdf(&img, &words, out.path())?;
        Ok(vec![out.commit()?])
    }
}

/// One-page PDF: the image, with recognised words as invisible, selectable text on top.
pub fn searchable_pdf(
    img: &image::DynamicImage,
    words: &[(Region, String)],
    out: &std::path::Path,
) -> Result<()> {
    use image::GenericImageView;
    use lopdf::content::{Content, Operation as Op};
    use lopdf::{dictionary, Document, Object, Stream, StringFormat};
    let (w, h) = img.dimensions();
    // 150 DPI page.
    let k = 72.0 / 150.0;
    let (pw, ph) = (w as f32 * k, h as f32 * k);
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let mut jpeg = Vec::new();
    encode::encode_to_vec(
        img,
        "jpg",
        &EncodeOptions {
            quality: 88,
            ..Default::default()
        },
    )
    .map(|b| jpeg = b)?;
    let mut xo = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode" },
        jpeg,
    );
    xo.allows_compression = false;
    let img_id = doc.add_object(xo);
    let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
    let mut ops = vec![
        Op::new("q", vec![]),
        Op::new(
            "cm",
            vec![pw.into(), 0.into(), 0.into(), ph.into(), 0.into(), 0.into()],
        ),
        Op::new("Do", vec!["Im0".into()]),
        Op::new("Q", vec![]),
        Op::new("BT", vec![]),
        Op::new("Tr", vec![3.into()]), // invisible
    ];
    for (r, t) in words {
        let size = (r.h * k * 0.8).max(1.0);
        // Helvetica averages ~0.5 em per character: stretch to the box width.
        let natural = t.chars().count() as f32 * size * 0.5;
        let tz = ((r.w * k) / natural.max(0.1) * 100.0).clamp(10.0, 500.0);
        let latin1: Vec<u8> = t
            .chars()
            .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
            .collect();
        ops.push(Op::new("Tf", vec!["F1".into(), size.into()]));
        ops.push(Op::new("Tz", vec![tz.into()]));
        ops.push(Op::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                (r.x * k).into(),
                (ph - (r.y + r.h * 0.85) * k).into(),
            ],
        ));
        ops.push(Op::new(
            "Tj",
            vec![Object::String(latin1, StringFormat::Literal)],
        ));
    }
    ops.push(Op::new("ET", vec![]));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode()?,
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), pw.into(), ph.into()],
        "Contents" => content,
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img_id }, "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", cat);
    doc.compress();
    doc.save(out).context("writing PDF")?;
    Ok(())
}
