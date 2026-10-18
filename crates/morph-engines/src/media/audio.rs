//! Audio tools (S11): Normalize, Visualizer, Trim (with auto silence trim), Channels, Bleep.

use super::ffmpeg::{self, Probe};
use super::{muxer, AUDIO};
use crate::media::tools::parse_time;
use anyhow::{bail, Context, Result};
use morph_core::{
    OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Params, Registry,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn choice(v: &[&str]) -> ParamKind {
    ParamKind::Choice {
        options: v.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn register(reg: &mut Registry) {
    let tool = |id: &str, label: &str, suffix: &str, order: i32| {
        OpSpec::tool(id, label, suffix, AUDIO, OutputKind::SameAsInput)
            .order(order)
            .component("ffmpeg")
            .editor("audio")
    };
    reg.register(Audio {
        kind: Kind::Normalize,
        spec: tool("audio.normalize", "Normalize", "Normalized", 2)
            .param(ParamSpec::new(
                "lufs",
                "Target loudness (LUFS)",
                ParamKind::Float {
                    min: -36.0,
                    max: -6.0,
                    step: 0.5,
                },
                json!(-16.0),
            ))
            .param(ParamSpec::new(
                "lra",
                "Loudness range (LU)",
                ParamKind::Float {
                    min: 1.0,
                    max: 20.0,
                    step: 0.5,
                },
                json!(11.0),
            ))
            .param(ParamSpec::new(
                "tp",
                "True peak (dBTP)",
                ParamKind::Float {
                    min: -9.0,
                    max: 0.0,
                    step: 0.1,
                },
                json!(-1.5),
            )),
    });
    reg.register(Audio {
        kind: Kind::Visualize,
        spec: {
            let mut s = tool("audio.visualize", "Visualizer", "Video", 3)
                .param(ParamSpec::new(
                    "style",
                    "Style",
                    choice(&["wave", "still"]),
                    json!("wave"),
                ))
                .param(ParamSpec::new(
                    "orientation",
                    "Shape",
                    choice(&["landscape", "portrait", "square"]),
                    json!("landscape"),
                ))
                .param(ParamSpec::new(
                    "color",
                    "Wave colour",
                    ParamKind::Color,
                    json!("#4FE3FF"),
                ))
                .param(ParamSpec::new(
                    "background",
                    "Background",
                    ParamKind::Color,
                    json!("#04070C"),
                ))
                .param(ParamSpec::new(
                    "image",
                    "Still image (path)",
                    ParamKind::Text,
                    json!(""),
                ));
            s.output = OutputKind::Ext("mp4".into());
            s
        },
    });
    reg.register(Audio {
        kind: Kind::Trim,
        spec: tool("audio.trim", "Trim", "Trimmed", 4)
            .param(ParamSpec::new(
                "start",
                "Start",
                ParamKind::Time,
                json!("0"),
            ))
            .param(ParamSpec::new("end", "End", ParamKind::Time, json!("")))
            .param(ParamSpec::new(
                "silence",
                "Auto-trim silence",
                ParamKind::Bool,
                json!(false),
            ))
            .param(ParamSpec::new(
                "threshold",
                "Silence below (dB)",
                ParamKind::Float {
                    min: -90.0,
                    max: -20.0,
                    step: 1.0,
                },
                json!(-50.0),
            )),
    });
    reg.register(Audio {
        kind: Kind::Channels,
        spec: tool("audio.channels", "Channels", "Channels", 5)
            .param(ParamSpec::new(
                "layout",
                "Output",
                choice(&["mono", "stereo"]),
                json!("mono"),
            ))
            .param(ParamSpec::new(
                "left_gain",
                "Left gain (dB)",
                ParamKind::Float {
                    min: -30.0,
                    max: 12.0,
                    step: 0.5,
                },
                json!(0.0),
            ))
            .param(ParamSpec::new(
                "right_gain",
                "Right gain (dB)",
                ParamKind::Float {
                    min: -30.0,
                    max: 12.0,
                    step: 0.5,
                },
                json!(0.0),
            )),
    });
    reg.register(Audio {
        kind: Kind::Bleep,
        spec: tool("audio.bleep", "Bleep", "Bleeped", 6)
            .param(ParamSpec::new(
                "ranges",
                "Ranges",
                ParamKind::Json,
                json!([]),
            ))
            .param(ParamSpec::new(
                "tone",
                "Tone (Hz)",
                ParamKind::Int {
                    min: 200,
                    max: 4000,
                },
                json!(1000),
            )),
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normalize,
    Visualize,
    Trim,
    Channels,
    Bleep,
}

struct Audio {
    kind: Kind,
    spec: OpSpec,
}

/// Encoder args for writing audio back to `ext`.
fn audio_codec(ext: &str) -> Vec<String> {
    let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match ext {
        "mp3" => s(&["-c:a", "libmp3lame", "-q:a", "2"]),
        "m4a" => s(&["-c:a", "aac", "-b:a", "192k"]),
        "flac" => s(&["-c:a", "flac"]),
        "ogg" => s(&["-c:a", "libvorbis", "-q:a", "5"]),
        "opus" => s(&["-c:a", "libopus", "-b:a", "128k"]),
        "aiff" => s(&["-c:a", "pcm_s16be"]),
        "wma" => s(&["-c:a", "wmav2", "-b:a", "192k"]),
        _ => s(&["-c:a", "pcm_s16le"]),
    }
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav")
        .to_ascii_lowercase()
}

fn time_param(p: &Params, k: &str) -> Result<Option<f64>> {
    match p.get(k) {
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) if !s.trim().is_empty() => parse_time(s).map(Some),
        _ => Ok(None),
    }
}

impl Operation for Audio {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        ffmpeg::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let p = ffmpeg::probe(input)?;
        if p.audio.is_none() {
            bail!("no audio stream");
        }
        let ext = ext_of(input);
        let mut duration = p.duration;
        let mut args: Vec<String> = Vec::new();
        match self.kind {
            Kind::Normalize => {
                let (i, lra, tp) = (
                    ctx.params.f64("lufs").unwrap_or(-16.0),
                    ctx.params.f64("lra").unwrap_or(11.0),
                    ctx.params.f64("tp").unwrap_or(-1.5),
                );
                ctx.progress.message(0.05, "measuring");
                let m = measure(input)?;
                let filter = format!(
                    "loudnorm=I={i}:LRA={lra}:TP={tp}:measured_I={}:measured_LRA={}:measured_TP={}:measured_thresh={}:offset={}:linear=true",
                    m.i, m.lra, m.tp, m.thresh, m.offset
                );
                // loudnorm resamples to 192 kHz internally: put the original rate back.
                let sr = p
                    .audio
                    .as_ref()
                    .map(|a| a.sample_rate)
                    .filter(|&r| r > 0)
                    .unwrap_or(48000);
                args.extend([
                    "-map".into(),
                    "0:a:0".into(),
                    "-af".into(),
                    filter,
                    "-ar".into(),
                    sr.to_string(),
                ]);
            }
            Kind::Trim => {
                let start = time_param(&ctx.params, "start")?.unwrap_or(0.0).max(0.0);
                let end = time_param(&ctx.params, "end")?
                    .unwrap_or(p.duration)
                    .min(p.duration);
                if end <= start {
                    bail!("end must be after start");
                }
                args.extend([
                    "-ss".into(),
                    format!("{start:.3}"),
                    "-to".into(),
                    format!("{end:.3}"),
                    "-map".into(),
                    "0:a:0".into(),
                ]);
                if ctx.params.bool("silence").unwrap_or(false) {
                    let th = ctx.params.f64("threshold").unwrap_or(-50.0);
                    // Leading silence, then reverse-and-repeat for trailing silence.
                    args.extend(["-af".into(), format!(
                        "silenceremove=start_periods=1:start_threshold={th}dB:start_silence=0.05,areverse,silenceremove=start_periods=1:start_threshold={th}dB:start_silence=0.05,areverse"
                    )]);
                }
                duration = end - start;
            }
            Kind::Channels => {
                let db = |k: &str| 10f64.powf(ctx.params.f64(k).unwrap_or(0.0) / 20.0);
                let (l, r) = (db("left_gain"), db("right_gain"));
                let stereo_in = p.audio.as_ref().is_some_and(|a| a.channels >= 2);
                let (cl, cr) = if stereo_in {
                    ("c0", "c1")
                } else {
                    ("c0", "c0")
                };
                let filter = if ctx.params.str("layout") == Some("stereo") {
                    format!("pan=stereo|c0={l:.4}*{cl}|c1={r:.4}*{cr}")
                } else {
                    format!("pan=mono|c0={:.4}*{cl}+{:.4}*{cr}", l * 0.5, r * 0.5)
                };
                args.extend(["-map".into(), "0:a:0".into(), "-af".into(), filter]);
            }
            Kind::Bleep => {
                let ranges = ctx
                    .params
                    .get("ranges")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let cond = bleep_condition(&ranges)?;
                let tone = ctx.params.u32("tone").unwrap_or(1000);
                let sr = p
                    .audio
                    .as_ref()
                    .map(|a| a.sample_rate)
                    .filter(|&r| r > 0)
                    .unwrap_or(48000);
                let graph = format!(
                    "[0:a:0]volume=volume='if({cond},0,1)':eval=frame[m];sine=f={tone}:r={sr}:d={:.3},volume=volume='if({cond},0.35,0)':eval=frame[b];[m][b]amix=inputs=2:duration=first:normalize=0[a]",
                    p.duration
                );
                args.extend(["-filter_complex".into(), graph, "-map".into(), "[a]".into()]);
            }
            Kind::Visualize => return visualize(ctx, input, &p),
        }
        args.extend(audio_codec(&ext));
        let out = ctx.sink.file(input, &ext)?;
        let mut full: Vec<String> = vec!["-i".into(), input.to_string_lossy().into_owned()];
        full.extend(args);
        full.extend([
            "-f".into(),
            muxer(&ext).into(),
            out.path().to_string_lossy().into_owned(),
        ]);
        ffmpeg::run(&full, duration, ctx.progress)?;
        Ok(vec![out.commit()?])
    }
}

fn bleep_condition(ranges: &[Value]) -> Result<String> {
    let mut parts = Vec::new();
    for r in ranges {
        let t = |k: &str| -> Result<f64> {
            match r.get(k) {
                Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(0.0)),
                Some(Value::String(s)) => parse_time(s),
                _ => bail!("range needs `{k}`"),
            }
        };
        let (a, b) = (t("start")?, t("end")?);
        if b > a {
            parts.push(format!("between(t,{a:.3},{b:.3})"));
        }
    }
    if parts.is_empty() {
        bail!("add at least one range to bleep");
    }
    Ok(parts.join("+"))
}

fn visualize(ctx: &OpContext, input: &Path, p: &Probe) -> Result<Vec<PathBuf>> {
    let (w, h) = match ctx.params.str("orientation").unwrap_or("landscape") {
        "portrait" => (1080, 1920),
        "square" => (1080, 1080),
        _ => (1920, 1080),
    };
    let color = ctx
        .params
        .str("color")
        .unwrap_or("#4FE3FF")
        .trim_start_matches('#')
        .to_string();
    let bg = ctx
        .params
        .str("background")
        .unwrap_or("#04070C")
        .trim_start_matches('#')
        .to_string();
    let enc = ffmpeg::h264_encoder();
    let mut args: Vec<String> = Vec::new();
    let image = ctx
        .params
        .str("image")
        .filter(|_| ctx.params.str("style") == Some("still"));
    match image {
        Some(img) => {
            args.extend([
                "-loop".into(),
                "1".into(),
                "-framerate".into(),
                "2".into(),
                "-i".into(),
                img.into(),
            ]);
            args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
            args.extend(["-filter_complex".into(), format!(
                "[0:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=0x{bg},format=yuv420p[v]"
            )]);
            args.extend([
                "-map".into(),
                "[v]".into(),
                "-map".into(),
                "1:a:0".into(),
                "-shortest".into(),
            ]);
        }
        None => {
            args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
            let wave_h = h / 3;
            args.extend(["-filter_complex".into(), format!(
                "color=c=0x{bg}:s={w}x{h}:r=30[bg];[0:a:0]showwaves=s={w}x{wave_h}:mode=cline:rate=30:colors=0x{color}:scale=sqrt,format=rgba[wave];[bg][wave]overlay=0:(H-h)/2:shortest=1,format=yuv420p[v]"
            )]);
            args.extend(["-map".into(), "[v]".into(), "-map".into(), "0:a:0".into()]);
        }
    }
    args.extend(["-c:v".into(), enc.into()]);
    args.extend(ffmpeg::bitrate_args(enc, 2500));
    args.extend(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"].map(String::from));
    let out = ctx.sink.file(input, "mp4")?;
    args.extend([
        "-f".into(),
        "mp4".into(),
        out.path().to_string_lossy().into_owned(),
    ]);
    ffmpeg::run(&args, p.duration, ctx.progress)?;
    Ok(vec![out.commit()?])
}

/// Integrated loudness and friends, from loudnorm's analysis pass.
#[derive(Debug, Clone, Serialize)]
pub struct Loudness {
    pub i: f64,
    pub lra: f64,
    pub tp: f64,
    pub thresh: f64,
    pub offset: f64,
}

pub fn measure(path: &Path) -> Result<Loudness> {
    let exe = ffmpeg::tool("ffmpeg").context("FFmpeg not found")?;
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "loudnorm=print_format=json",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .output()
        .context("measuring loudness")?;
    let err = String::from_utf8_lossy(&out.stderr);
    let start = err.rfind('{').context("no loudnorm report")?;
    let end = err[start..].find('}').context("no loudnorm report")? + start + 1;
    let j: Value = serde_json::from_str(&err[start..end])?;
    let f = |k: &str| {
        j[k].as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    Ok(Loudness {
        i: f("input_i"),
        lra: f("input_lra"),
        tp: f("input_tp"),
        thresh: f("input_thresh"),
        offset: f("target_offset"),
    })
}

/// Min/max peaks in `buckets` slices (waveform view), from 8 kHz mono PCM.
pub fn peaks(path: &Path, buckets: usize) -> Result<Vec<(f32, f32)>> {
    let exe = ffmpeg::tool("ffmpeg").context("FFmpeg not found")?;
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-map", "0:a:0", "-ac", "1", "-ar", "8000", "-f", "f32le", "-",
        ])
        .stdin(Stdio::null())
        .output()
        .context("decoding audio")?;
    let samples: Vec<f32> = out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    if samples.is_empty() {
        return Ok(vec![]);
    }
    let per = (samples.len() as f64 / buckets.max(1) as f64).max(1.0);
    Ok((0..buckets)
        .map(|i| {
            let a = (i as f64 * per) as usize;
            let b = (((i + 1) as f64 * per) as usize)
                .min(samples.len())
                .max(a + 1)
                .min(samples.len());
            samples[a.min(samples.len() - 1)..b]
                .iter()
                .fold((0f32, 0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)))
        })
        .collect())
}
