//! Image tools I (S5): Compress, Resize, Crop, Rotate, Metadata, Strip metadata.

use super::encode::{self, EncodeOptions};
use super::{compress, crop, decode, metadata, INPUTS};
use anyhow::Result;
use morph_core::{OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Registry};
use serde_json::json;
use std::path::PathBuf;

/// Raster formats tools write back in kind; anything else is saved as PNG.
pub fn tool_output_ext(input: &morph_core::Kind) -> &'static str {
    match input.ext.as_str() {
        "jpg" => "jpg",
        "webp" => "webp",
        "avif" => "avif",
        "tiff" => "tiff",
        "bmp" => "bmp",
        _ => "png",
    }
}

fn choice(v: &[&str]) -> ParamKind {
    ParamKind::Choice {
        options: v.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn register(reg: &mut Registry) {
    reg.register(Compress {
        spec: OpSpec::tool(
            "image.compress",
            "Compress",
            "Compressed",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(1)
        .editor("compress")
        .param(
            ParamSpec::new("target", "Target size", ParamKind::Size, json!(""))
                .help("e.g. 1MB, 500KB — leave empty to use a preset"),
        )
        .param(ParamSpec::new(
            "percent",
            "Reduce by %",
            ParamKind::Int { min: 0, max: 95 },
            json!(0),
        ))
        .param(ParamSpec::new(
            "preset",
            "Preset",
            choice(&["high", "balanced", "small"]),
            json!("balanced"),
        ))
        .param(ParamSpec::new(
            "format",
            "Format",
            choice(&["same", "jpg", "webp", "avif", "png"]),
            json!("same"),
        ))
        .param(ParamSpec::new(
            "max_width",
            "Max width",
            ParamKind::Int { min: 0, max: 30000 },
            json!(0),
        ))
        .param(ParamSpec::new(
            "max_height",
            "Max height",
            ParamKind::Int { min: 0, max: 30000 },
            json!(0),
        ))
        .param(ParamSpec::new(
            "scale",
            "Scale %",
            ParamKind::Int { min: 1, max: 100 },
            json!(100),
        )),
    });
    reg.register(Resize {
        spec: OpSpec::tool(
            "image.resize",
            "Resize",
            "Resized",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(2)
        .editor("form")
        .param(
            ParamSpec::new(
                "percent",
                "Scale %",
                ParamKind::Int { min: 1, max: 400 },
                json!(50),
            )
            .help("Used when width and height are both 0"),
        )
        .param(
            ParamSpec::new(
                "width",
                "Width",
                ParamKind::Int { min: 0, max: 32768 },
                json!(0),
            )
            .help("Pixels; 0 = follow the height"),
        )
        .param(
            ParamSpec::new(
                "height",
                "Height",
                ParamKind::Int { min: 0, max: 32768 },
                json!(0),
            )
            .help("Pixels; 0 = follow the width. Both set: fit inside, keeping the shape"),
        ),
    });
    reg.register(Rotate {
        spec: OpSpec::tool(
            "image.rotate",
            "Rotate",
            "Rotated",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(4)
        .editor("form")
        .quick()
        .param(ParamSpec::new(
            "turn",
            "Turn",
            choice(ROTATIONS),
            json!("right"),
        )),
    });
    reg.register(Crop {
        spec: OpSpec::tool(
            "image.crop",
            "Crop",
            "Cropped",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(3)
        .editor("crop")
        .param(ParamSpec::new(
            "ratio",
            "Ratio",
            choice(&["free", "1:1", "4:5", "16:9", "3:2", "4:3", "9:16", "2:3"]),
            json!("free"),
        ))
        .param(ParamSpec::new(
            "x",
            "X",
            ParamKind::Int {
                min: 0,
                max: 100000,
            },
            json!(0),
        ))
        .param(ParamSpec::new(
            "y",
            "Y",
            ParamKind::Int {
                min: 0,
                max: 100000,
            },
            json!(0),
        ))
        .param(
            ParamSpec::new(
                "w",
                "Crop width",
                ParamKind::Int {
                    min: 0,
                    max: 100000,
                },
                json!(0),
            )
            .help("0 = largest centred area of the ratio"),
        )
        .param(ParamSpec::new(
            "h",
            "Crop height",
            ParamKind::Int {
                min: 0,
                max: 100000,
            },
            json!(0),
        ))
        .param(
            ParamSpec::new(
                "width",
                "Output width",
                ParamKind::Int { min: 0, max: 30000 },
                json!(0),
            )
            .help("Resize the crop to exactly W×H"),
        )
        .param(ParamSpec::new(
            "height",
            "Output height",
            ParamKind::Int { min: 0, max: 30000 },
            json!(0),
        )),
    });
    let meta_inputs: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/tiff"];
    reg.register(MetaEdit {
        spec: OpSpec::tool(
            "image.metadata",
            "Metadata",
            "Edited",
            meta_inputs,
            OutputKind::SameAsInput,
        )
        .order(12)
        .editor("metadata")
        .param(ParamSpec::new("set", "Fields", ParamKind::Json, json!({})))
        .param(ParamSpec::new(
            "remove_gps",
            "Remove location",
            ParamKind::Bool,
            json!(false),
        ))
        .param(ParamSpec::new(
            "remove_xmp_iptc",
            "Remove XMP and IPTC",
            ParamKind::Bool,
            json!(false),
        )),
    });
    reg.register(Strip {
        spec: OpSpec::tool(
            "image.strip",
            "Strip metadata",
            "Clean",
            meta_inputs,
            OutputKind::SameAsInput,
        )
        .order(20)
        // Reached through Metadata's STRIP ALL, recipes and `morph tool strip`.
        .unlisted(),
    });
}

struct Compress {
    spec: OpSpec,
}

impl Operation for Compress {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let original = std::fs::metadata(input)?.len();
        let ext = match ctx.params.str("format").unwrap_or("same") {
            "same" => match tool_output_ext(ctx.kind()) {
                "tiff" | "bmp" => "jpg",
                e => e,
            },
            "jpg" => "jpg",
            "webp" => "webp",
            "avif" => "avif",
            _ => "png",
        };
        let goal = if let Some(t) = ctx.params.size("target").filter(|&t| t > 0) {
            compress::Goal::Target(t)
        } else if let Some(p) = ctx.params.u32("percent").filter(|&p| p > 0) {
            compress::Goal::Target(original * (100 - p as u64) / 100)
        } else {
            compress::Goal::Quality(match ctx.params.str("preset").unwrap_or("balanced") {
                "high" => 88,
                "small" => 55,
                _ => 74,
            })
        };
        let img = decode::decode(input, ctx.kind())?;
        ctx.progress.set(0.2);
        let img = compress::resize(
            img,
            compress::Resize {
                max_width: ctx.params.u32("max_width").unwrap_or(0),
                max_height: ctx.params.u32("max_height").unwrap_or(0),
                scale: ctx.params.u32("scale").unwrap_or(100),
            },
        );
        let progress = ctx.progress.clone();
        let (bytes, _q) = compress::compress(img, ext, goal, &move || progress.is_cancelled())?;
        let out = ctx.sink.file(input, ext)?;
        std::fs::write(out.path(), bytes)?;
        ctx.progress.set(1.0);
        Ok(vec![out.commit()?])
    }
}

/// Rotate / flip choices, shared with video.
pub const ROTATIONS: &[&str] = &["right", "left", "180", "flip-horizontal", "flip-vertical"];

/// New size for Resize: a box (either side may be 0 = keep the ratio) or a percentage.
pub fn resize_target(w: u32, h: u32, width: u32, height: u32, percent: u32) -> (u32, u32) {
    let f = match (width, height) {
        (0, 0) => percent.clamp(1, 400) as f64 / 100.0,
        (bw, 0) => bw as f64 / w as f64,
        (0, bh) => bh as f64 / h as f64,
        (bw, bh) => (bw as f64 / w as f64).min(bh as f64 / h as f64),
    };
    (
        ((w as f64 * f).round() as u32).max(1),
        ((h as f64 * f).round() as u32).max(1),
    )
}

/// Save a tool result in the input's format.
fn save(ctx: &OpContext, img: &image::DynamicImage) -> Result<Vec<PathBuf>> {
    let input = ctx.input();
    let ext = tool_output_ext(ctx.kind());
    let out = ctx.sink.file(input, ext)?;
    encode::encode(
        img,
        ext,
        out.path(),
        &EncodeOptions {
            quality: 95,
            ..Default::default()
        },
    )?;
    Ok(vec![out.commit()?])
}

struct Resize {
    spec: OpSpec,
}

impl Operation for Resize {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        use image::GenericImageView;
        let img = decode::decode(ctx.input(), ctx.kind())?;
        let (w, h) = img.dimensions();
        let p = &ctx.params;
        let (nw, nh) = resize_target(
            w,
            h,
            p.u32("width").unwrap_or(0),
            p.u32("height").unwrap_or(0),
            p.u32("percent").unwrap_or(50),
        );
        ctx.progress.set(0.3);
        let img = if (nw, nh) == (w, h) {
            img
        } else {
            img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
        };
        ctx.progress.check()?;
        save(ctx, &img)
    }
}

struct Rotate {
    spec: OpSpec,
}

impl Operation for Rotate {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let img = decode::decode(ctx.input(), ctx.kind())?;
        let img = match ctx.params.str("turn").unwrap_or("right") {
            "right" => img.rotate90(),
            "left" => img.rotate270(),
            "180" => img.rotate180(),
            "flip-horizontal" => img.fliph(),
            "flip-vertical" => img.flipv(),
            other => anyhow::bail!("unknown turn `{other}`"),
        };
        save(ctx, &img)
    }
}

struct Crop {
    spec: OpSpec,
}

impl Operation for Crop {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        use image::GenericImageView;
        let input = ctx.input();
        let img = decode::decode(input, ctx.kind())?;
        let (w, h) = img.dimensions();
        let p = &ctx.params;
        let (cw, ch) = (p.u32("w").unwrap_or(0), p.u32("h").unwrap_or(0));
        let (ow, oh) = (p.u32("width").unwrap_or(0), p.u32("height").unwrap_or(0));
        let rect = if cw > 0 && ch > 0 {
            crop::Rect {
                x: p.u32("x").unwrap_or(0),
                y: p.u32("y").unwrap_or(0),
                w: cw,
                h: ch,
            }
        } else {
            // No rectangle: centre the requested ratio (or the output size's ratio).
            let ratio = crop::parse_ratio(p.str("ratio").unwrap_or("free"))?
                .or((ow > 0 && oh > 0).then_some((ow as f64, oh as f64)));
            match ratio {
                Some(r) => crop::centred(w, h, r),
                None => crop::Rect { x: 0, y: 0, w, h },
            }
        };
        let out_size = (ow > 0 && oh > 0).then_some((ow, oh));
        let cropped = crop::crop(&img, rect, out_size)?;
        save(ctx, &cropped)
    }
}

struct MetaEdit {
    spec: OpSpec,
}

impl Operation for MetaEdit {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let mut edit = metadata::Edit {
            remove_gps: ctx.params.bool("remove_gps").unwrap_or(false),
            remove_xmp_iptc: ctx.params.bool("remove_xmp_iptc").unwrap_or(false),
            ..Default::default()
        };
        if let Some(serde_json::Value::Object(m)) = ctx.params.get("set") {
            for (k, v) in m {
                edit.set
                    .push((k.clone(), v.as_str().unwrap_or_default().to_string()));
            }
        }
        let data = std::fs::read(input)?;
        let bytes = metadata::apply(&data, &ctx.kind().ext, &edit)?;
        let out = ctx.sink.file(input, &ctx.kind().ext)?;
        std::fs::write(out.path(), bytes)?;
        Ok(vec![out.commit()?])
    }
}

struct Strip {
    spec: OpSpec,
}

impl Operation for Strip {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let ext = ctx.kind().ext.as_str();
        let bytes = if ext == "tiff" {
            // TIFF keeps tags in the image directory itself: re-encode (lossless).
            let img = decode::decode(input, ctx.kind())?;
            encode::encode_to_vec(&img, "tiff", &EncodeOptions::default())?
        } else {
            metadata::strip(&std::fs::read(input)?, ext)?
        };
        let out = ctx.sink.file(input, ext)?;
        std::fs::write(out.path(), bytes)?;
        Ok(vec![out.commit()?])
    }
}
