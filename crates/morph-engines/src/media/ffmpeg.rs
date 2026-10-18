//! Running FFmpeg/ffprobe as separate processes (LGPL build, no linking).

use anyhow::{bail, Context, Result};
use morph_core::Progress;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// `ffmpeg` / `ffprobe` from the component, a bundled sidecar or PATH.
pub fn tool(name: &str) -> Option<PathBuf> {
    morph_core::components::find_tool("ffmpeg", name)
}

pub fn available() -> bool {
    tool("ffmpeg").is_some() && tool("ffprobe").is_some()
}

fn command(name: &str) -> Result<Command> {
    let exe = tool(name)
        .ok_or_else(|| anyhow::anyhow!(morph_core::Error::ComponentMissing("ffmpeg".into())))?;
    let mut c = Command::new(&exe);
    // Shared builds keep their libraries in ../lib next to bin/ on Linux.
    #[cfg(target_os = "linux")]
    if let Some(lib) = exe.parent().and_then(|b| b.parent()).map(|r| r.join("lib")) {
        if lib.is_dir() {
            let old = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
            let mut paths = vec![lib];
            paths.extend(std::env::split_paths(&old));
            if let Ok(joined) = std::env::join_paths(paths) {
                c.env("LD_LIBRARY_PATH", joined);
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c.stdin(Stdio::null());
    Ok(c)
}

/// What ffprobe knows about a file.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub duration: f64,
    pub size: u64,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
}

#[derive(Debug, Clone, Default)]
pub struct VideoStream {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frames: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct AudioStream {
    pub codec: String,
    pub channels: u32,
    pub sample_rate: u32,
}

pub fn probe(path: &Path) -> Result<Probe> {
    let out = command("ffprobe")?
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .context("running ffprobe")?;
    if !out.status.success() {
        bail!("ffprobe: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let j: Value = serde_json::from_slice(&out.stdout)?;
    let num = |v: &Value| {
        v.as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| v.as_f64())
    };
    let mut p = Probe {
        duration: j["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        size: j["format"]["size"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        ..Default::default()
    };
    for s in j["streams"].as_array().cloned().unwrap_or_default() {
        match s["codec_type"].as_str() {
            Some("video")
                if p.video.is_none() && s["disposition"]["attached_pic"].as_i64() != Some(1) =>
            {
                let fps = s["avg_frame_rate"]
                    .as_str()
                    .and_then(|r| r.split_once('/'))
                    .and_then(|(a, b)| {
                        Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?.max(1e-9))
                    })
                    .filter(|f| f.is_finite() && *f > 0.0)
                    .unwrap_or(0.0);
                p.video = Some(VideoStream {
                    codec: s["codec_name"].as_str().unwrap_or("").into(),
                    width: s["width"].as_u64().unwrap_or(0) as u32,
                    height: s["height"].as_u64().unwrap_or(0) as u32,
                    fps,
                    frames: s["nb_frames"].as_str().and_then(|n| n.parse().ok()),
                });
                if p.duration == 0.0 {
                    p.duration = num(&s["duration"]).unwrap_or(0.0);
                }
            }
            Some("audio") if p.audio.is_none() => {
                p.audio = Some(AudioStream {
                    codec: s["codec_name"].as_str().unwrap_or("").into(),
                    channels: s["channels"].as_u64().unwrap_or(0) as u32,
                    sample_rate: s["sample_rate"]
                        .as_str()
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(0),
                });
            }
            _ => {}
        }
    }
    Ok(p)
}

/// Run ffmpeg with `args` (inputs and outputs included). Reports progress against
/// `duration` seconds and kills ffmpeg when the job is cancelled.
pub fn run(args: &[String], duration: f64, progress: &Progress) -> Result<()> {
    let mut cmd = command("ffmpeg")?;
    cmd.args([
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "error",
        "-progress",
        "pipe:1",
        "-nostats",
    ]);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    log::debug!("ffmpeg {}", args.join(" "));
    let mut child = cmd.spawn().context("starting ffmpeg")?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap_or_default();
        if progress.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!(morph_core::Error::Cancelled);
        }
        if let Some(us) = line
            .strip_prefix("out_time_us=")
            .or_else(|| line.strip_prefix("out_time_ms="))
        {
            if let Ok(us) = us.trim().parse::<f64>() {
                if duration > 0.0 {
                    progress.set((us / 1e6 / duration) as f32);
                }
            }
        }
    }
    let status = child.wait()?;
    let err = err_thread.join().unwrap_or_default();
    if !status.success() {
        bail!(
            "ffmpeg failed: {}",
            err.lines()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    Ok(())
}

/// The best working H.264 encoder: hardware first, OpenH264 (software) as fallback.
pub fn h264_encoder() -> &'static str {
    static ENC: OnceLock<&'static str> = OnceLock::new();
    ENC.get_or_init(|| {
        if std::env::var_os("MORPH_SOFTWARE_ENCODE").is_some() {
            return "libopenh264";
        }
        let candidates: &[&str] = if cfg!(windows) {
            &["h264_nvenc", "h264_qsv", "h264_amf"]
        } else {
            &["h264_nvenc", "h264_qsv", "h264_vaapi"]
        };
        for enc in candidates {
            if encoder_works(enc) {
                log::info!("using hardware encoder {enc}");
                return enc;
            }
        }
        "libopenh264"
    })
}

/// Can `enc` actually encode on this machine (driver present)?
pub fn encoder_works(enc: &str) -> bool {
    let Ok(mut c) = command("ffmpeg") else {
        return false;
    };
    let mut args: Vec<&str> = vec!["-hide_banner", "-loglevel", "error"];
    if enc.ends_with("_vaapi") {
        args.extend(["-vaapi_device", "/dev/dri/renderD128"]);
    }
    args.extend(["-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.2"]);
    if enc.ends_with("_vaapi") {
        args.extend(["-vf", "format=nv12,hwupload"]);
    }
    args.extend(["-c:v", enc, "-f", "null", "-"]);
    c.args(&args).stdout(Stdio::null()).stderr(Stdio::null());
    c.status().map(|s| s.success()).unwrap_or(false)
}

/// Rate-control flags for `enc` at `kbps` (target-size compression).
pub fn bitrate_args(enc: &str, kbps: u64) -> Vec<String> {
    let b = format!("{kbps}k");
    let mut v = vec![
        "-b:v".into(),
        b.clone(),
        "-maxrate".into(),
        format!("{}k", kbps * 3 / 2),
        "-bufsize".into(),
        format!("{}k", kbps * 2),
    ];
    match enc {
        "h264_nvenc" => v.extend(["-rc".into(), "vbr".into()]),
        "h264_qsv" => v.extend(["-look_ahead".into(), "0".into()]),
        "libopenh264" => v.extend(["-rc_mode".into(), "bitrate".into()]),
        _ => {}
    }
    v
}

/// Quality flags for `enc` (normal conversions).
pub fn quality_args(enc: &str) -> Vec<String> {
    let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect();
    match enc {
        "h264_nvenc" => s(&["-preset", "p5", "-cq", "23"]),
        "h264_qsv" => s(&["-global_quality", "23", "-look_ahead", "0"]),
        "h264_amf" => s(&[
            "-quality", "quality", "-rc", "cqp", "-qp_i", "22", "-qp_p", "24",
        ]),
        "h264_vaapi" => s(&["-qp", "23"]),
        _ => s(&[
            "-b:v",
            "0",
            "-rc_mode",
            "quality",
            "-allow_skip_frames",
            "0",
        ]),
    }
}
