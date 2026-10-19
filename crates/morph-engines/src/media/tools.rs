//! Video tools (S10): Trim, Crop, Rotate, Mute, Speed, Join, Save frames, Split, Redact.
//! Audio tools (S11) live in `audio.rs`.

use super::ffmpeg::{self, Probe};
use super::{codec_args, muxer, VIDEO_ONLY};
use anyhow::{bail, Context, Result};
use morph_core::{
    Arity, OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Params, Registry,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn time(key: &str, label: &str, default: Value) -> ParamSpec {
    ParamSpec::new(key, label, ParamKind::Time, default)
}

pub fn register(reg: &mut Registry) {
    let tool = |id: &str, label: &str, suffix: &str, order: i32| {
        OpSpec::tool(id, label, suffix, VIDEO_ONLY, OutputKind::SameAsInput)
            .order(order)
            .component("ffmpeg")
            .editor("video")
    };
    reg.register(Video {
        kind: Kind::Trim,
        spec: tool("video.trim", "Trim", "Trimmed", 2)
            .param(time("start", "Start", json!("0")))
            .param(time("end", "End", json!("")).help("Empty = to the end")),
    });
    reg.register(Video {
        kind: Kind::Crop,
        spec: tool("video.crop", "Crop", "Cropped", 3)
            .param(ParamSpec::new(
                "x",
                "X",
                ParamKind::Int { min: 0, max: 16384 },
                json!(0),
            ))
            .param(ParamSpec::new(
                "y",
                "Y",
                ParamKind::Int { min: 0, max: 16384 },
                json!(0),
            ))
            .param(ParamSpec::new(
                "w",
                "Width",
                ParamKind::Int { min: 0, max: 16384 },
                json!(0),
            ))
            .param(ParamSpec::new(
                "h",
                "Height",
                ParamKind::Int { min: 0, max: 16384 },
                json!(0),
            ))
            .param(
                ParamSpec::new("ratio", "Ratio", ParamKind::Text, json!(""))
                    .help("1:1, 9:16… centred, when no rectangle"),
            ),
    });
    reg.register(Video {
        kind: Kind::Rotate,
        spec: {
            let mut s = tool("video.rotate", "Rotate", "Rotated", 4).param(ParamSpec::new(
                "turn",
                "Turn",
                ParamKind::Choice {
                    options: crate::image::tools::ROTATIONS
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                },
                json!("right"),
            ));
            s.editor = Some("form".into());
            s.quick = true;
            s
        },
    });
    reg.register(Video {
        kind: Kind::Mute,
        spec: {
            let mut s = tool("video.mute", "Mute", "Muted", 5);
            s.editor = None;
            s
        },
    });
    reg.register(Video {
        kind: Kind::Speed,
        spec: tool("video.speed", "Speed", "Speed", 8).param(ParamSpec::new(
            "factor",
            "Speed ×",
            ParamKind::Float {
                min: 0.25,
                max: 4.0,
                step: 0.05,
            },
            json!(2.0),
        )),
    });
    reg.register(Video {
        kind: Kind::Join,
        spec: tool("video.join", "Join", "Joined", 7).arity(Arity::All),
    });
    reg.register(Video {
        kind: Kind::Frames,
        spec: {
            let mut s = tool("video.frames", "Save frames", "Frames", 9).param(
                ParamSpec::new("times", "At (seconds)", ParamKind::Text, json!("0"))
                    .help("Comma-separated, e.g. 1.5, 3, 12"),
            );
            s.output = OutputKind::Folder;
            s
        },
    });
    reg.register(Video {
        kind: Kind::Split,
        spec: {
            let mut s = tool("video.split", "Split", "Parts", 6)
                .param(ParamSpec::new(
                    "points",
                    "Split at (seconds)",
                    ParamKind::Text,
                    json!(""),
                ))
                .param(ParamSpec::new(
                    "parts",
                    "Or equal parts",
                    ParamKind::Int { min: 0, max: 100 },
                    json!(2),
                ));
            s.output = OutputKind::Folder;
            s
        },
    });
    reg.register(Video {
        kind: Kind::Redact,
        spec: tool("video.redact", "Redact", "Redacted", 10)
            .param(ParamSpec::new(
                "regions",
                "Regions",
                ParamKind::Json,
                json!([]),
            ))
            .param(ParamSpec::new(
                "style",
                "Style",
                ParamKind::Choice {
                    options: vec!["solid".into(), "blur".into()],
                },
                json!("solid"),
            )),
    });
    super::audio::register(reg);
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Trim,
    Crop,
    Rotate,
    Mute,
    Speed,
    Join,
    Frames,
    Split,
    Redact,
}

struct Video {
    kind: Kind,
    spec: OpSpec,
}

/// Parse `12.5`, `1:02.5`, `00:01:02.500`.
pub fn parse_time(s: &str) -> Result<f64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty time");
    }
    let mut total = 0.0;
    for part in s.split(':') {
        total = total * 60.0
            + part
                .trim()
                .parse::<f64>()
                .with_context(|| format!("bad time `{s}`"))?;
    }
    Ok(total)
}

fn param_time(p: &Params, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) if !s.trim().is_empty() => parse_time(s).map(Some),
        _ => Ok(None),
    }
}

/// `00-01-02.500` for file names.
fn stamp(t: f64) -> String {
    let ms = (t * 1000.0).round() as u64;
    format!(
        "{:02}-{:02}-{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn out_ext(input: &Path) -> String {
    let e = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp4")
        .to_ascii_lowercase();
    match e.as_str() {
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" => e,
        "m4v" => "mp4".into(),
        _ => "mp4".into(),
    }
}

fn s(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

/// Video/audio codec args for `ext`, minus stream mapping (filters do the mapping).
fn encode_args(ext: &str, p: &Probe) -> Result<Vec<String>> {
    let mut v = codec_args(ext, p, &Params::new())?;
    // Drop -map pairs; callers map explicitly.
    let mut out = Vec::new();
    let mut i = 0;
    while i < v.len() {
        if v[i] == "-map" {
            i += 2;
            continue;
        }
        out.push(std::mem::take(&mut v[i]));
        i += 1;
    }
    Ok(out)
}

/// Encoder args with the audio copied untouched instead of re-encoded.
fn copy_audio(mut enc: Vec<String>) -> Vec<String> {
    if let Some(i) = enc.iter().position(|a| a == "-c:a") {
        enc.splice(i..(i + 2).min(enc.len()), s(&["-c:a", "copy"]));
        if let Some(j) = enc.iter().position(|a| a == "-b:a") {
            enc.drain(j..j + 2);
        }
    }
    enc
}

impl Operation for Video {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        ffmpeg::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        match self.kind {
            Kind::Join => return join(ctx),
            Kind::Frames => return frames(ctx),
            Kind::Split => return split(ctx),
            _ => {}
        }
        let input = ctx.input();
        let p = ffmpeg::probe(input)?;
        let v = p.video.clone().context("no video stream")?;
        let ext = out_ext(input);
        let mut args: Vec<String> = Vec::new();
        let mut duration = p.duration;
        let has_audio = p.audio.is_some();
        match self.kind {
            Kind::Trim => {
                let start = param_time(&ctx.params, "start")?.unwrap_or(0.0).max(0.0);
                let end = param_time(&ctx.params, "end")?
                    .unwrap_or(p.duration)
                    .min(p.duration);
                if end <= start {
                    bail!("end must be after start");
                }
                // Decode-accurate trim: seek after the input, re-encode.
                args.extend([
                    "-ss".into(),
                    format!("{start:.3}"),
                    "-to".into(),
                    format!("{end:.3}"),
                ]);
                args.extend(s(&["-map", "0:v:0", "-map", "0:a:0?"]));
                duration = end - start;
            }
            Kind::Crop => {
                let (mut x, mut y) = (
                    ctx.params.u32("x").unwrap_or(0),
                    ctx.params.u32("y").unwrap_or(0),
                );
                let (mut w, mut h) = (
                    ctx.params.u32("w").unwrap_or(0),
                    ctx.params.u32("h").unwrap_or(0),
                );
                if w == 0 || h == 0 {
                    let ratio =
                        crate::image::crop::parse_ratio(ctx.params.str("ratio").unwrap_or(""))?
                            .unwrap_or((v.width as f64, v.height as f64));
                    let r = crate::image::crop::centred(v.width, v.height, ratio);
                    (x, y, w, h) = (r.x, r.y, r.w, r.h);
                }
                // Encoders want even sizes.
                let (w, h) = (
                    w.min(v.width - x.min(v.width - 1)) & !1,
                    h.min(v.height - y.min(v.height - 1)) & !1,
                );
                args.extend(["-filter:v".into(), format!("crop={w}:{h}:{x}:{y}")]);
                args.extend(s(&["-map", "0:v:0", "-map", "0:a:0?"]));
                args.extend(copy_audio(encode_args(&ext, &p)?));
                return Ok(vec![run_ff(ctx, input, &ext, args, duration)?]);
            }
            Kind::Rotate => {
                let filter = match ctx.params.str("turn").unwrap_or("right") {
                    "right" => "transpose=clock",
                    "left" => "transpose=cclock",
                    "180" => "hflip,vflip",
                    "flip-horizontal" => "hflip",
                    "flip-vertical" => "vflip",
                    other => bail!("unknown turn `{other}`"),
                };
                args.extend(["-filter:v".into(), filter.into()]);
                args.extend(s(&["-map", "0:v:0", "-map", "0:a:0?"]));
                args.extend(copy_audio(encode_args(&ext, &p)?));
                return Ok(vec![run_ff(ctx, input, &ext, args, duration)?]);
            }
            Kind::Mute => {
                // The picture is copied as is: fast and lossless.
                args.extend(s(&["-map", "0:v:0", "-c:v", "copy", "-an"]));
                if matches!(ext.as_str(), "mp4" | "mov") {
                    args.extend(s(&["-movflags", "+faststart"]));
                }
                return Ok(vec![run_ff(ctx, input, &ext, args, duration)?]);
            }
            Kind::Speed => {
                let f = ctx.params.f64("factor").unwrap_or(2.0).clamp(0.25, 4.0);
                // Retimed frames are conformed back to the source rate (drop/duplicate evenly).
                let fps = if v.fps > 0.0 { v.fps } else { 30.0 };
                let video = format!("[0:v]setpts=PTS/{f},fps={fps:.3}[v]");
                if has_audio {
                    args.extend([
                        "-filter_complex".into(),
                        format!("{video};[0:a]{}[a]", atempo(f)),
                    ]);
                    args.extend(s(&["-map", "[v]", "-map", "[a]", "-shortest"]));
                } else {
                    args.extend(["-filter_complex".into(), video, "-map".into(), "[v]".into()]);
                }
                duration = p.duration / f;
            }
            Kind::Redact => {
                let style = ctx.params.str("style").unwrap_or("solid");
                let regions = ctx
                    .params
                    .get("regions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let graph = redact_graph(&regions, style, p.duration)?;
                args.extend(["-filter_complex".into(), graph]);
                args.extend(s(&["-map", "[v]", "-map", "0:a:0?"]));
            }
            _ => unreachable!(),
        }
        args.extend(encode_args(&ext, &p)?);
        Ok(vec![run_ff(ctx, input, &ext, args, duration)?])
    }
}

/// atempo accepts 0.5–2.0 per stage: chain stages for other factors (pitch preserved).
pub fn atempo(f: f64) -> String {
    let mut stages = Vec::new();
    let mut r = f;
    while r > 2.0 {
        stages.push(2.0);
        r /= 2.0;
    }
    while r < 0.5 {
        stages.push(0.5);
        r /= 0.5;
    }
    stages.push(r);
    stages
        .iter()
        .map(|s| format!("atempo={s:.6}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Filter graph drawing solid boxes or blurring regions during their time range.
fn redact_graph(regions: &[Value], style: &str, duration: f64) -> Result<String> {
    if regions.is_empty() {
        bail!("no regions to redact");
    }
    let f = |r: &Value, k: &str, d: f64| r.get(k).and_then(Value::as_f64).unwrap_or(d);
    let mut g = String::new();
    let mut last = "0:v".to_string();
    for (i, r) in regions.iter().enumerate() {
        let (x, y, w, h) = (
            f(r, "x", 0.0),
            f(r, "y", 0.0),
            f(r, "w", 0.0).max(2.0),
            f(r, "h", 0.0).max(2.0),
        );
        let (t0, t1) = (f(r, "start", 0.0), f(r, "end", duration));
        let enable = format!("between(t,{t0:.3},{t1:.3})");
        let out = format!("r{i}");
        if style == "blur" {
            let radius = ((w.min(h) / 4.0).clamp(4.0, 60.0)) as u32;
            g.push_str(&format!(
                "[{last}]split[b{i}][c{i}];[c{i}]crop={w:.0}:{h:.0}:{x:.0}:{y:.0},boxblur={radius}:2[k{i}];[b{i}][k{i}]overlay={x:.0}:{y:.0}:enable='{enable}'[{out}];"
            ));
        } else {
            g.push_str(&format!("[{last}]drawbox=x={x:.0}:y={y:.0}:w={w:.0}:h={h:.0}:color=black@1:t=fill:enable='{enable}'[{out}];"));
        }
        last = out;
    }
    g.push_str(&format!("[{last}]format=yuv420p[v]"));
    Ok(g)
}

fn run_ff(
    ctx: &OpContext,
    input: &Path,
    ext: &str,
    args: Vec<String>,
    duration: f64,
) -> Result<PathBuf> {
    let out = ctx.sink.file(input, ext)?;
    let mut full: Vec<String> = vec!["-i".into(), input.to_string_lossy().into_owned()];
    full.extend(args);
    full.extend([
        "-f".into(),
        muxer(ext).into(),
        out.path().to_string_lossy().into_owned(),
    ]);
    ffmpeg::run(&full, duration, ctx.progress)?;
    Ok(out.commit()?)
}

/// Join clips in selection order, conformed to the first clip's size, frame rate and audio.
fn join(ctx: &OpContext) -> Result<Vec<PathBuf>> {
    if ctx.inputs.len() < 2 {
        bail!("select two or more clips to join");
    }
    let probes: Vec<Probe> = ctx
        .inputs
        .iter()
        .map(|p| ffmpeg::probe(p))
        .collect::<Result<_>>()?;
    let first = probes[0]
        .video
        .clone()
        .context("the first clip has no video")?;
    let (w, h) = (first.width & !1, first.height & !1);
    let fps = if first.fps > 0.0 { first.fps } else { 30.0 };
    let mut args: Vec<String> = Vec::new();
    for p in &ctx.inputs {
        args.extend(["-i".into(), p.to_string_lossy().into_owned()]);
    }
    let mut g = String::new();
    let mut cat = String::new();
    for (i, p) in probes.iter().enumerate() {
        g.push_str(&format!(
            "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps:.3},format=yuv420p[v{i}];"
        ));
        if p.audio.is_some() {
            g.push_str(&format!(
                "[{i}:a:0]aformat=sample_rates=48000:channel_layouts=stereo[a{i}];"
            ));
        } else {
            g.push_str(&format!(
                "anullsrc=r=48000:cl=stereo,atrim=duration={:.3}[a{i}];",
                p.duration
            ));
        }
        cat.push_str(&format!("[v{i}][a{i}]"));
    }
    g.push_str(&format!("{cat}concat=n={}:v=1:a=1[v][a]", probes.len()));
    args.extend(["-filter_complex".into(), g]);
    args.extend(s(&["-map", "[v]", "-map", "[a]"]));
    let ext = out_ext(&ctx.inputs[0]);
    args.extend(encode_args(&ext, &probes[0])?);
    let total: f64 = probes.iter().map(|p| p.duration).sum();
    let out = ctx.sink.file(ctx.input(), &ext)?;
    args.extend([
        "-f".into(),
        muxer(&ext).into(),
        out.path().to_string_lossy().into_owned(),
    ]);
    ffmpeg::run(&args, total, ctx.progress)?;
    Ok(vec![out.commit()?])
}

/// Full-resolution PNG frames at the given times.
fn frames(ctx: &OpContext) -> Result<Vec<PathBuf>> {
    let input = ctx.input();
    let p = ffmpeg::probe(input)?;
    let times: Vec<f64> = ctx
        .params
        .str("times")
        .unwrap_or("0")
        .split(',')
        .filter(|t| !t.trim().is_empty())
        .map(parse_time)
        .collect::<Result<_>>()?;
    if times.is_empty() {
        bail!("no frame times");
    }
    let grab = |t: f64, out: &Path| -> Result<()> {
        let t = t.clamp(0.0, (p.duration - 0.001).max(0.0));
        let args = vec![
            "-ss".into(),
            format!("{t:.3}"),
            "-i".into(),
            input.to_string_lossy().into_owned(),
            "-frames:v".into(),
            "1".into(),
            "-update".into(),
            "1".into(),
            "-f".into(),
            "image2".into(),
            "-c:v".into(),
            "png".into(),
            out.to_string_lossy().into_owned(),
        ];
        ffmpeg::run(&args, 0.0, ctx.progress)
    };
    let stem = morph_core::output::stem_of(input);
    if times.len() == 1 {
        let out = ctx.sink.file_named(
            &ctx.sink.dir_for(input),
            &format!("{stem} Frame {}.png", stamp(times[0])),
        )?;
        grab(times[0], out.path())?;
        return Ok(vec![out.commit()?]);
    }
    let dir = ctx.sink.folder(input)?;
    for (i, t) in times.iter().enumerate() {
        ctx.progress.check()?;
        grab(*t, &dir.path().join(format!("Frame {}.png", stamp(*t))))?;
        ctx.progress.set((i + 1) as f32 / times.len() as f32);
    }
    Ok(vec![dir.commit()?])
}

/// Split at points or into equal parts, all into one new folder.
fn split(ctx: &OpContext) -> Result<Vec<PathBuf>> {
    let input = ctx.input();
    let p = ffmpeg::probe(input)?;
    let mut cuts: Vec<f64> = match ctx.params.str("points") {
        Some(pts) => pts
            .split(',')
            .filter(|t| !t.trim().is_empty())
            .map(parse_time)
            .collect::<Result<_>>()?,
        None => {
            let n = ctx.params.u32("parts").unwrap_or(2).max(2) as f64;
            (1..n as u32).map(|i| p.duration * i as f64 / n).collect()
        }
    };
    cuts.retain(|&c| c > 0.0 && c < p.duration);
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cuts.dedup();
    if cuts.is_empty() {
        bail!("no split points inside the clip");
    }
    let mut bounds = vec![0.0];
    bounds.extend(&cuts);
    bounds.push(p.duration);
    let ext = out_ext(input);
    let dir = ctx.sink.folder(input)?;
    let n = bounds.len() - 1;
    for i in 0..n {
        ctx.progress.check()?;
        let (a, b) = (bounds[i], bounds[i + 1]);
        let mut args: Vec<String> = vec![
            "-i".into(),
            input.to_string_lossy().into_owned(),
            "-ss".into(),
            format!("{a:.3}"),
            "-to".into(),
            format!("{b:.3}"),
        ];
        args.extend(s(&["-map", "0:v:0", "-map", "0:a:0?"]));
        args.extend(encode_args(&ext, &p)?);
        let out = dir.path().join(format!("Part {:02}.{ext}", i + 1));
        args.extend([
            "-f".into(),
            muxer(&ext).into(),
            out.to_string_lossy().into_owned(),
        ]);
        ffmpeg::run(&args, b - a, &morph_core::Progress::silent())?;
        ctx.progress.set((i + 1) as f32 / n as f32);
    }
    Ok(vec![dir.commit()?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("12.5").unwrap(), 12.5);
        assert_eq!(parse_time("1:02.5").unwrap(), 62.5);
        assert_eq!(parse_time("00:01:02.500").unwrap(), 62.5);
        assert!(parse_time("x").is_err());
        assert_eq!(stamp(62.5), "00-01-02.500");
    }

    #[test]
    fn atempo_chains() {
        assert_eq!(atempo(1.5), "atempo=1.500000");
        assert_eq!(atempo(4.0), "atempo=2.000000,atempo=2.000000");
        assert_eq!(atempo(0.25), "atempo=0.500000,atempo=0.500000");
    }
}
