//! Image tools II (S6): Edit, Annotate, Add BG, Collage.

use super::adjust::{self, Adjust};
use super::compose::{self, Background, Collage, Source};
use super::encode::{self, EncodeOptions};
use super::tools::tool_output_ext;
use super::INPUTS;
use anyhow::{Context, Result};
use morph_core::{
    Arity, OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Params, Registry,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

fn float(key: &str, label: &str, min: f64, max: f64) -> ParamSpec {
    ParamSpec::new(
        key,
        label,
        ParamKind::Float {
            min,
            max,
            step: 0.01,
        },
        json!(0.0),
    )
}

fn choice(v: &[&str]) -> ParamKind {
    ParamKind::Choice {
        options: v.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn register(reg: &mut Registry) {
    reg.register(Edit {
        spec: OpSpec::tool(
            "image.edit",
            "Edit",
            "Edited",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(13)
        .editor("edit")
        .param(float("exposure", "Exposure", -3.0, 3.0))
        .param(float("contrast", "Contrast", -1.0, 1.0))
        .param(float("saturation", "Saturation", -1.0, 1.0))
        .param(float("vibrance", "Vibrance", -1.0, 1.0))
        .param(float("temperature", "Temperature", -1.0, 1.0))
        .param(float("tint", "Tint", -1.0, 1.0))
        .param(float("detail", "Detail", 0.0, 1.0))
        .param(float("clarity", "Clarity", 0.0, 1.0))
        .param(float("dehaze", "Dehaze", 0.0, 1.0))
        .param(float("grain", "Grain", 0.0, 1.0))
        .param(float("noise_reduction", "Noise reduction", 0.0, 1.0)),
    });
    reg.register(Annotate {
        spec: OpSpec::tool(
            "image.annotate",
            "Annotate",
            "Annotated",
            INPUTS,
            OutputKind::SameAsInput,
        )
        .order(6)
        .editor("annotate")
        .param(ParamSpec::new(
            "shapes",
            "Shapes",
            ParamKind::Json,
            json!([]),
        )),
    });
    let d = Background::default();
    reg.register(AddBg {
        spec: OpSpec::tool(
            "image.background",
            "Add BG",
            "With BG",
            INPUTS,
            OutputKind::Ext("png".into()),
        )
        .order(5)
        .alias("bg")
        .editor("background")
        .param(ParamSpec::new(
            "kind",
            "Background",
            choice(&["gradient", "solid", "blur", "image"]),
            json!(d.kind),
        ))
        .param(ParamSpec::new(
            "color",
            "Colour",
            ParamKind::Color,
            json!(d.color),
        ))
        .param(ParamSpec::new(
            "color2",
            "Second colour",
            ParamKind::Color,
            json!(d.color2),
        ))
        .param(ParamSpec::new(
            "angle",
            "Angle",
            ParamKind::Int { min: 0, max: 360 },
            json!(d.angle),
        ))
        .param(ParamSpec::new(
            "padding",
            "Padding %",
            ParamKind::Float {
                min: 0.0,
                max: 50.0,
                step: 0.5,
            },
            json!(d.padding),
        ))
        .param(ParamSpec::new(
            "radius",
            "Corner radius",
            ParamKind::Int { min: 0, max: 400 },
            json!(d.radius),
        ))
        .param(ParamSpec::new(
            "shadow",
            "Shadow",
            ParamKind::Float {
                min: 0.0,
                max: 1.0,
                step: 0.05,
            },
            json!(d.shadow),
        ))
        .param(
            ParamSpec::new("size", "Size", ParamKind::Text, json!(d.size))
                .help("auto, a ratio (1:1, 4:5, 16:9) or exact pixels (1080x1350)"),
        )
        .param(ParamSpec::new(
            "image",
            "Background picture",
            ParamKind::Text,
            json!(""),
        )),
    });
    let c = Collage::default();
    reg.register(MakeCollage {
        spec: OpSpec::tool(
            "image.collage",
            "Collage",
            "Collage",
            INPUTS,
            OutputKind::Ext("png".into()),
        )
        .arity(Arity::All)
        .order(10)
        .editor("collage")
        .param(ParamSpec::new(
            "layout",
            "Layout",
            choice(&["grid", "row", "column", "featured"]),
            json!(c.layout),
        ))
        .param(ParamSpec::new(
            "width",
            "Width",
            ParamKind::Int {
                min: 200,
                max: 20000,
            },
            json!(c.width),
        ))
        .param(ParamSpec::new(
            "spacing",
            "Spacing",
            ParamKind::Int { min: 0, max: 400 },
            json!(c.spacing),
        ))
        .param(ParamSpec::new(
            "radius",
            "Corner radius",
            ParamKind::Int { min: 0, max: 400 },
            json!(c.radius),
        ))
        .param(ParamSpec::new(
            "background",
            "Background",
            ParamKind::Color,
            json!(c.background),
        )),
    });
}

fn params_json(p: &Params) -> Value {
    Value::Object(p.0.clone())
}

fn save(ctx: &OpContext, img: &image::DynamicImage, ext: &str) -> Result<Vec<PathBuf>> {
    let out = ctx.sink.file(ctx.input(), ext)?;
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

struct Edit {
    spec: OpSpec,
}

impl Operation for Edit {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let a: Adjust =
            serde_json::from_value(params_json(&ctx.params)).context("adjustment values")?;
        let img = super::decode::decode(ctx.input(), ctx.kind())?;
        ctx.progress.set(0.3);
        let out = adjust::apply(&img, &a);
        ctx.progress.set(0.8);
        save(ctx, &out, tool_output_ext(ctx.kind()))
    }
}

struct Annotate {
    spec: OpSpec,
}

impl Operation for Annotate {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let src = Source::load(ctx.input(), ctx.kind())?;
        let shapes = ctx.params.get("shapes").cloned().unwrap_or(json!([]));
        let scene = compose::annotate(&src, &shapes)?;
        let img = compose::render(&scene, std::slice::from_ref(&src.img))?;
        save(ctx, &img, tool_output_ext(ctx.kind()))
    }
}

/// Load the optional background picture named in params.
fn bg_picture(p: &Params) -> Result<Option<Source>> {
    match p.str("image") {
        Some(path) => {
            let path = PathBuf::from(path);
            let kind = morph_core::detect(&path)?;
            Ok(Some(Source::load(&path, &kind)?))
        }
        None => Ok(None),
    }
}

/// The scene for a background op, shared by run and the editor preview.
pub fn background_scene(src: &Source, p: &Params) -> Result<(compose::Scene, Option<Source>)> {
    let bg: Background = serde_json::from_value(params_json(p)).context("background options")?;
    let extra = bg_picture(p)?;
    Ok((compose::background(src, &bg, extra.as_ref())?, extra))
}

struct AddBg {
    spec: OpSpec,
}

impl Operation for AddBg {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let src = Source::load(ctx.input(), ctx.kind())?;
        let (scene, extra) = background_scene(&src, &ctx.params)?;
        let mut imgs = vec![src.img.clone()];
        if let Some(e) = extra {
            imgs.push(e.img.clone());
        }
        let img = compose::render(&scene, &imgs)?;
        save(ctx, &img, "png")
    }
}

struct MakeCollage {
    spec: OpSpec,
}

impl Operation for MakeCollage {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let srcs: Vec<Source> = ctx
            .inputs
            .iter()
            .zip(&ctx.kinds)
            .map(|(p, k)| Source::load(p, k))
            .collect::<Result<_>>()?;
        ctx.progress.set(0.4);
        let c: Collage =
            serde_json::from_value(params_json(&ctx.params)).context("collage options")?;
        let scene = compose::collage(&srcs, &c)?;
        let imgs: Vec<Arc<image::DynamicImage>> = srcs.iter().map(|s| s.img.clone()).collect();
        let img = compose::render(&scene, &imgs)?;
        save(ctx, &img, "png")
    }
}

/// Preview scene for an editor: (svg, width, height, source paths behind morph:i).
pub fn preview(
    op: &str,
    inputs: &[(PathBuf, morph_core::Kind)],
    params: &Params,
) -> Result<(compose::Scene, Vec<PathBuf>)> {
    let first = inputs.first().context("no input")?;
    match op {
        "image.annotate" => {
            let src = Source::load(&first.0, &first.1)?;
            let shapes = params.get("shapes").cloned().unwrap_or(json!([]));
            Ok((compose::annotate(&src, &shapes)?, vec![first.0.clone()]))
        }
        "image.background" => {
            let src = Source::load(&first.0, &first.1)?;
            let (scene, extra) = background_scene(&src, params)?;
            let mut paths = vec![first.0.clone()];
            if let Some(e) = extra {
                paths.push(e.path);
            }
            Ok((scene, paths))
        }
        "image.collage" => {
            let srcs: Vec<Source> = inputs
                .iter()
                .map(|(p, k)| Source::load(p, k))
                .collect::<Result<_>>()?;
            let c: Collage =
                serde_json::from_value(params_json(params)).context("collage options")?;
            Ok((
                compose::collage(&srcs, &c)?,
                inputs.iter().map(|i| i.0.clone()).collect(),
            ))
        }
        "image.redact" => {
            // Areas only: detection runs once in the editor, which adds the results as areas.
            let src = Source::load(&first.0, &first.1)?;
            let kind = if params.str("style") == Some("blur") {
                "blur"
            } else {
                "redact"
            };
            let color = params.str("color").unwrap_or("#000000").to_string();
            let shapes: Vec<Value> = params
                .get("areas")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|r| {
                    let f = |k: &str| r.get(k).and_then(Value::as_f64).unwrap_or(0.0);
                    json!({ "type": kind, "x": f("x"), "y": f("y"), "w": f("w"), "h": f("h"), "color": color,
                            "strength": (f("w").min(f("h")) / 3.0).max(8.0) })
                }).collect())
                .unwrap_or_default();
            Ok((
                compose::annotate(&src, &Value::Array(shapes))?,
                vec![first.0.clone()],
            ))
        }
        other => anyhow::bail!("no preview for {other}"),
    }
}
