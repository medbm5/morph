//! Media engine (FFmpeg sidecar): video/audio conversions, video → GIF/MP3,
//! and compression to a target size.

pub mod audio;
pub mod ffmpeg;
pub mod tools;

use anyhow::{bail, Result};
use ffmpeg::Probe;
use morph_core::{OpContext, OpSpec, Operation, OutputKind, ParamKind, ParamSpec, Registry};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Animated GIFs count as video input too (GIF → MP4).
pub const VIDEO: &[&str] = &["video/*", "image/gif"];
pub const AUDIO: &[&str] = &["audio/*"];
pub const MEDIA: &[&str] = &["video/*", "audio/*"];
pub const VIDEO_ONLY: &[&str] = &["video/*"];

const VIDEO_OUT: &[(&str, &str, i32)] = &[
    ("mp4", "MP4", 1),
    ("mov", "MOV", 2),
    ("mkv", "MKV", 3),
    ("webm", "WebM", 4),
    ("gif", "GIF", 5),
    ("avi", "AVI", 7),
    ("wmv", "WMV", 8),
];
const AUDIO_OUT: &[(&str, &str, i32)] = &[
    ("mp3", "MP3", 6),
    ("m4a", "M4A", 10),
    ("wav", "WAV", 11),
    ("flac", "FLAC", 12),
    ("ogg", "OGG", 13),
    ("opus", "Opus", 14),
    ("aiff", "AIFF", 15),
    ("wma", "WMA", 16),
];

pub fn register(reg: &mut Registry) {
    for (ext, label, order) in VIDEO_OUT {
        let mut spec = OpSpec::convert(ext, label, VIDEO)
            .order(*order)
            .component("ffmpeg");
        spec.id = format!("media.to-{ext}");
        if *ext == "gif" {
            spec = spec
                .param(ParamSpec::new(
                    "fps",
                    "Frame rate",
                    ParamKind::Int { min: 1, max: 50 },
                    json!(12),
                ))
                .param(ParamSpec::new(
                    "width",
                    "Width",
                    ParamKind::Int { min: 64, max: 3840 },
                    json!(720),
                ));
        }
        reg.register(Convert { spec, ext });
    }
    for (ext, label, order) in AUDIO_OUT {
        // MP3 from video too ("extract audio"); other audio formats from audio and video.
        let mut spec = OpSpec::convert(ext, label, MEDIA)
            .order(*order)
            .component("ffmpeg");
        spec.id = format!("media.to-{ext}");
        reg.register(Convert { spec, ext });
    }
    reg.register(Compress {
        spec: OpSpec::tool(
            "media.compress",
            "Compress",
            "Compressed",
            MEDIA,
            OutputKind::SameAsInput,
        )
        .order(1)
        .editor("compress")
        .component("ffmpeg")
        .param(
            ParamSpec::new("target", "Target size", ParamKind::Size, json!("")).help("e.g. 25MB"),
        )
        .param(ParamSpec::new(
            "percent",
            "Reduce by %",
            ParamKind::Int { min: 0, max: 95 },
            json!(50),
        ))
        .param(ParamSpec::new(
            "max_height",
            "Max height",
            ParamKind::Int { min: 0, max: 4320 },
            json!(0),
        )),
    });
    tools::register(reg);
}

struct Convert {
    spec: OpSpec,
    ext: &'static str,
}

/// Output codec arguments for a container.
pub fn codec_args(ext: &str, p: &Probe, params: &morph_core::Params) -> Result<Vec<String>> {
    let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let has_video = p.video.is_some();
    let mut v: Vec<String> = Vec::new();
    match ext {
        "mp4" | "mov" | "mkv" => {
            if !has_video {
                bail!("no video stream");
            }
            let enc = ffmpeg::h264_encoder();
            v.extend(s(&["-map", "0:v:0", "-map", "0:a:0?", "-c:v", enc]));
            v.extend(video_quality(enc, p));
            v.extend(s(&["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]));
            if ext != "mkv" {
                v.extend(s(&["-movflags", "+faststart"]));
            }
        }
        "webm" => v.extend(s(&[
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "libvpx-vp9",
            "-b:v",
            "0",
            "-crf",
            "32",
            "-row-mt",
            "1",
            "-deadline",
            "good",
            "-cpu-used",
            "4",
            "-c:a",
            "libopus",
            "-b:a",
            "128k",
        ])),
        "avi" => v.extend(s(&[
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "mpeg4",
            "-q:v",
            "3",
            "-c:a",
            "libmp3lame",
            "-q:a",
            "3",
        ])),
        "wmv" => v.extend(s(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-c:v", "wmv2", "-q:v", "3", "-c:a", "wmav2",
            "-b:a", "192k",
        ])),
        "gif" => {
            let fps = params.u32("fps").unwrap_or(12);
            let width = params.u32("width").unwrap_or(720);
            v.extend(s(&["-an", "-filter_complex"]));
            v.push(format!(
                "[0:v]fps={fps},scale='min({width},iw)':-2:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4"
            ));
            v.extend(s(&["-loop", "0"]));
        }
        audio => {
            if p.audio.is_none() {
                bail!("no audio stream");
            }
            v.extend(s(&["-vn", "-map", "0:a:0"]));
            v.extend(match audio {
                "mp3" => s(&["-c:a", "libmp3lame", "-q:a", "2"]),
                "m4a" => s(&["-c:a", "aac", "-b:a", "192k"]),
                "wav" => s(&["-c:a", "pcm_s16le"]),
                "flac" => s(&["-c:a", "flac"]),
                "ogg" => s(&["-c:a", "libvorbis", "-q:a", "5"]),
                "opus" => s(&["-c:a", "libopus", "-b:a", "128k"]),
                "aiff" => s(&["-c:a", "pcm_s16be"]),
                "wma" => s(&["-c:a", "wmav2", "-b:a", "192k"]),
                other => bail!("can't write .{other}"),
            });
        }
    }
    Ok(v)
}

/// Quality-based settings; OpenH264 has no CRF, so it gets a bitrate scaled to the picture.
fn video_quality(enc: &str, p: &Probe) -> Vec<String> {
    if enc == "libopenh264" {
        let v = p.video.clone().unwrap_or_default();
        let fps = if v.fps > 0.0 { v.fps } else { 30.0 };
        let kbps = ((v.width.max(320) as f64 * v.height.max(240) as f64 * fps * 0.08) / 1000.0)
            .clamp(400.0, 40_000.0) as u64;
        return ffmpeg::bitrate_args(enc, kbps);
    }
    ffmpeg::quality_args(enc)
}

/// Run ffmpeg from `input` into a pending output with the given extension.
pub fn transcode(
    ctx: &OpContext,
    input: &Path,
    ext: &str,
    args: Vec<String>,
    duration: f64,
) -> Result<PathBuf> {
    let out = ctx.sink.file(input, ext)?;
    // ffmpeg picks the muxer from the extension; our temp file has none.
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

/// FFmpeg muxer name for an extension.
pub fn muxer(ext: &str) -> &'static str {
    match ext {
        "mp4" => "mp4",
        "mov" => "mov",
        "mkv" => "matroska",
        "webm" => "webm",
        "avi" => "avi",
        "wmv" | "wma" => "asf",
        "gif" => "gif",
        "mp3" => "mp3",
        "m4a" => "ipod",
        "wav" => "wav",
        "flac" => "flac",
        "ogg" => "ogg",
        "opus" => "opus",
        "aiff" => "aiff",
        "png" => "image2",
        "srt" => "srt",
        _ => "matroska",
    }
}

impl Operation for Convert {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        ffmpeg::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let p = ffmpeg::probe(input)?;
        let args = codec_args(self.ext, &p, &ctx.params)?;
        Ok(vec![transcode(ctx, input, self.ext, args, p.duration)?])
    }
}

struct Compress {
    spec: OpSpec,
}

impl Operation for Compress {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        ffmpeg::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let p = ffmpeg::probe(input)?;
        let original = std::fs::metadata(input)?.len();
        let target = match ctx.params.size("target").filter(|&t| t > 0) {
            Some(t) => t,
            None => original * (100 - ctx.params.u32("percent").unwrap_or(50).min(95) as u64) / 100,
        };
        if p.duration <= 0.0 {
            bail!("unknown duration; can't size the output");
        }
        let is_video = p.video.is_some() && ctx.kind().family == morph_core::Family::Video;
        let ext = if is_video {
            "mp4"
        } else {
            compressed_audio_ext(&ctx.kind().ext)
        };
        compress_to(ctx, input, &p, ext, target, is_video)
    }
}

fn compressed_audio_ext(ext: &str) -> &'static str {
    match ext {
        "m4a" => "m4a",
        "ogg" => "ogg",
        "opus" => "opus",
        _ => "mp3",
    }
}

/// Encode, measure, correct and re-encode until within ±4 % (max 4 tries).
///
/// Hardware encoders follow the corrected bitrate. OpenH264 (software) only honours a
/// bitrate it can reach at the current resolution, so on overshoot it scales the
/// picture down, and as a last resort may skip frames.
fn compress_to(
    ctx: &OpContext,
    input: &Path,
    p: &Probe,
    ext: &str,
    target: u64,
    video: bool,
) -> Result<Vec<PathBuf>> {
    let total_kbps = (target as f64 * 8.0 / p.duration / 1000.0) * 0.97; // container overhead
    let audio_kbps = if p.audio.is_some() {
        (total_kbps * 0.1).clamp(32.0, 128.0)
    } else {
        0.0
    };
    let mut video_kbps = (total_kbps - audio_kbps).max(50.0);
    let enc = ffmpeg::h264_encoder();
    let software = enc == "libopenh264";
    let max_h = ctx.params.u32("max_height").unwrap_or(0);
    let src_h = p.video.as_ref().map(|v| v.height).unwrap_or(0);
    let mut height = {
        let auto_h = auto_height(p, video_kbps);
        if max_h > 0 {
            max_h.min(auto_h.unwrap_or(max_h))
        } else {
            auto_h.unwrap_or(0)
        }
    };
    let attempts = 4;
    for attempt in 0..attempts {
        let skip_frames = software && attempt == attempts - 1;
        let mut args: Vec<String> = Vec::new();
        if video {
            args.extend(["-map", "0:v:0", "-map", "0:a:0?", "-c:v", enc].map(String::from));
            args.extend(ffmpeg::bitrate_args(enc, video_kbps as u64));
            if skip_frames {
                args.extend(["-allow_skip_frames".into(), "1".into()]);
            }
            args.extend(["-pix_fmt", "yuv420p", "-movflags", "+faststart"].map(String::from));
            if height > 0 && height < src_h {
                args.extend(["-vf".into(), format!("scale=-2:{height}")]);
            }
            if p.audio.is_some() {
                args.extend([
                    "-c:a".into(),
                    "aac".into(),
                    "-b:a".into(),
                    format!("{}k", audio_kbps as u64),
                ]);
            }
        } else {
            let kbps = total_kbps.clamp(16.0, 320.0) as u64;
            let codec = match ext {
                "m4a" => "aac",
                "ogg" => "libvorbis",
                "opus" => "libopus",
                _ => "libmp3lame",
            };
            args.extend(["-vn", "-map", "0:a:0", "-c:a", codec].map(String::from));
            args.extend(["-b:a".into(), format!("{kbps}k")]);
            if codec == "libmp3lame" {
                args.extend(["-abr".into(), "0".into()]);
            }
        }
        let out = ctx.sink.file(input, ext)?;
        let mut full: Vec<String> = vec!["-i".into(), input.to_string_lossy().into_owned()];
        full.extend(args);
        full.extend([
            "-f".into(),
            muxer(ext).into(),
            out.path().to_string_lossy().into_owned(),
        ]);
        ffmpeg::run(&full, p.duration, ctx.progress)?;
        let got = std::fs::metadata(out.path())?.len();
        let err = got as f64 / target as f64;
        log::debug!("compress attempt {attempt} ({enc}, h={height}): {got} bytes for target {target} ({err:.3})");
        if (0.96..=1.04).contains(&err) || attempt == attempts - 1 || !video {
            if got > target + target / 20 {
                bail!("couldn't get under {} (got {})", target, got);
            }
            return Ok(vec![out.commit()?]);
        }
        if software && err > 1.04 {
            // Fewer pixels for the same bitrate.
            let cur = if height > 0 { height } else { src_h };
            height = (((cur as f64 / err.sqrt()) / 2.0).floor() * 2.0).max(144.0) as u32;
        } else {
            video_kbps = (video_kbps / err).max(30.0);
        }
    }
    unreachable!()
}

/// Suggested output height for a bitrate (≈0.1 bit per pixel per frame).
fn auto_height(p: &Probe, kbps: f64) -> Option<u32> {
    let v = p.video.as_ref()?;
    let fps = if v.fps > 0.0 { v.fps } else { 30.0 };
    let pixels = kbps * 1000.0 / (fps * 0.1);
    let aspect = v.width as f64 / v.height.max(1) as f64;
    let h = (pixels / aspect).sqrt();
    let h = ((h / 2.0).floor() * 2.0) as u32;
    (h < v.height && h >= 144).then_some(h.max(144))
}
