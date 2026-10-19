//! Image engine: conversions between raster formats, SVG and HEIC input,
//! images → PDF and images → ICO.

pub mod adjust;
pub mod compose;
pub mod compress;
pub mod crop;
pub mod decode;
pub mod encode;
pub mod heif;
pub mod metadata;
pub mod pdf_out;
pub mod tools;
pub mod tools2;

use anyhow::Result;
use encode::EncodeOptions;
use morph_core::{Arity, OpContext, OpSpec, Operation, ParamKind, ParamSpec, Registry};
use serde_json::json;
use std::path::PathBuf;

/// Every image kind we can read.
pub const INPUTS: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/tiff",
    "image/avif",
    "image/bmp",
    "image/gif",
    "image/heic",
    "image/svg+xml",
    "image/x-icon",
];

pub fn register(reg: &mut Registry) {
    let formats = [
        ("jpg", "JPG", 1),
        ("png", "PNG", 2),
        ("webp", "WebP", 3),
        ("avif", "AVIF", 4),
        ("tiff", "TIFF", 5),
    ];
    for (ext, label, order) in formats {
        let mut spec = OpSpec::convert(ext, label, INPUTS).order(order);
        if matches!(ext, "jpg" | "webp" | "avif") {
            spec = spec.param(
                ParamSpec::new(
                    "quality",
                    "Quality",
                    ParamKind::Int { min: 1, max: 100 },
                    json!(default_quality(ext)),
                )
                .help("1–100; higher is better and larger"),
            );
        }
        if ext == "webp" {
            spec = spec.param(ParamSpec::new(
                "lossless",
                "Lossless",
                ParamKind::Bool,
                json!(false),
            ));
        }
        spec = spec.param(
            ParamSpec::new(
                "width",
                "SVG width",
                ParamKind::Int { min: 0, max: 32768 },
                json!(0),
            )
            .help("Rasterize SVG input at this width in pixels (0 = intrinsic size)"),
        );
        reg.register(Convert { spec, ext });
    }

    tools::register(reg);
    tools2::register(reg);

    reg.register(ToIco {
        spec: OpSpec::convert("ico", "ICO", INPUTS).order(9),
    });

    reg.register(ToPdf {
        spec: OpSpec::convert("pdf", "PDF", INPUTS)
            .arity(Arity::All)
            .order(7)
            .param(ParamSpec::new(
                "page",
                "Page size",
                ParamKind::Choice {
                    options: vec!["fit".into(), "a4".into(), "letter".into()],
                },
                json!("fit"),
            ))
            .param(ParamSpec::new(
                "dpi",
                "DPI",
                ParamKind::Int { min: 36, max: 1200 },
                json!(150),
            )),
    });
}

fn default_quality(ext: &str) -> u8 {
    match ext {
        "avif" => 70,
        "webp" => 85,
        _ => 90,
    }
}

struct Convert {
    spec: OpSpec,
    ext: &'static str,
}

impl Operation for Convert {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        ctx.progress.set(0.05);
        let img = match (ctx.kind().ext.as_str(), ctx.params.u32("width")) {
            ("svg", Some(w)) if w > 0 => decode::decode_svg(input, Some(w))?,
            _ => decode::decode(input, ctx.kind())?,
        };
        ctx.progress.check()?;
        ctx.progress.set(0.5);
        let opts = EncodeOptions {
            quality: ctx
                .params
                .u32("quality")
                .unwrap_or(default_quality(self.ext) as u32)
                .clamp(1, 100) as u8,
            lossless: ctx.params.bool("lossless").unwrap_or(false),
            ..Default::default()
        };
        let out = ctx.sink.file(input, self.ext)?;
        encode::encode(&img, self.ext, out.path(), &opts)?;
        ctx.progress.set(1.0);
        Ok(vec![out.commit()?])
    }
}

struct ToPdf {
    spec: OpSpec,
}

impl Operation for ToPdf {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let dpi = ctx.params.f64("dpi").unwrap_or(150.0) as f32;
        let size = match ctx.params.str("page").unwrap_or("fit") {
            "a4" => pdf_out::PageSize::A4 { margin: 36.0 },
            "letter" => pdf_out::PageSize::Letter { margin: 36.0 },
            _ => pdf_out::PageSize::Fit { dpi },
        };
        let items: Vec<_> = ctx
            .inputs
            .iter()
            .cloned()
            .zip(ctx.kinds.iter().cloned())
            .collect();
        let out = ctx.sink.file(ctx.input(), "pdf")?;
        pdf_out::images_to_pdf(&items, out.path(), size, ctx.progress)?;
        Ok(vec![out.commit()?])
    }
}

/// Icon sizes written into an ICO, up to the source's size (16 px at least).
const ICO_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// Image → multi-size ICO for favicons and app icons. Non-square images are
/// centred on a transparent square.
struct ToIco {
    spec: OpSpec,
}

impl Operation for ToIco {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        use ::image::codecs::ico::{IcoEncoder, IcoFrame};
        use ::image::{imageops, GenericImageView, RgbaImage};
        let input = ctx.input();
        let img = decode::decode(input, ctx.kind())?;
        let (w, h) = img.dimensions();
        let largest = w.max(h);
        let mut frames = Vec::new();
        for &size in ICO_SIZES
            .iter()
            .filter(|&&s| s <= largest || s == ICO_SIZES[0])
        {
            ctx.progress.check()?;
            let f = size as f64 / largest as f64;
            let (fw, fh) = (
                ((w as f64 * f).round() as u32).max(1),
                ((h as f64 * f).round() as u32).max(1),
            );
            let scaled = img
                .resize_exact(fw, fh, imageops::FilterType::Lanczos3)
                .to_rgba8();
            let mut square = RgbaImage::new(size, size);
            imageops::overlay(
                &mut square,
                &scaled,
                ((size - fw) / 2).into(),
                ((size - fh) / 2).into(),
            );
            frames.push(IcoFrame::as_png(
                square.as_raw(),
                size,
                size,
                ::image::ExtendedColorType::Rgba8,
            )?);
        }
        let out = ctx.sink.file(input, "ico")?;
        let file = std::io::BufWriter::new(std::fs::File::create(out.path())?);
        IcoEncoder::new(file).encode_images(&frames)?;
        ctx.progress.set(1.0);
        Ok(vec![out.commit()?])
    }
}

/// Is an output format for this engine? Used by other engines (PDF render, frames).
pub fn is_output(ext: &str) -> bool {
    encode::OUTPUTS.contains(&ext)
}

/// Width and height from the file header, without decoding pixels.
pub fn probe_size(path: &std::path::Path) -> Option<(u32, u32)> {
    ::image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}
