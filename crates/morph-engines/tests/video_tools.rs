//! S10: each video tool produces the expected duration/frames (±1 frame).

use morph_core::{Job, Params, Runner};
use morph_engines::media::ffmpeg;
use std::path::{Path, PathBuf};

const FPS: f64 = 25.0;

fn ready() -> bool {
    let ok = ffmpeg::available();
    if !ok {
        eprintln!("FFmpeg not found — skipping");
    }
    ok
}

/// 6 s, 640×360 @ 25 fps with a tone.
fn clip(dir: &Path, name: &str, size: &str, secs: u32) -> PathBuf {
    let out = dir.join(name);
    let args: Vec<String> = [
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc2=s={size}:r=25:d={secs}"),
        "-f",
        "lavfi",
        "-i",
        &format!("sine=f=440:d={secs}"),
        "-c:v",
        "libopenh264",
        "-b:v",
        "1500k",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        &out.to_string_lossy(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    ffmpeg::run(&args, 0.0, &morph_core::Progress::silent()).unwrap();
    out
}

fn run(op: &str, inputs: Vec<PathBuf>, params: Params) -> PathBuf {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(&Job::new(op, inputs).params(params), |_| {})
        .unwrap();
    assert!(r.ok(), "{op}: {:?}", r.errors);
    r.outputs[0].clone()
}

/// Frames by decoding (exact).
fn frames(path: &Path) -> u64 {
    let ffprobe = ffmpeg::tool("ffprobe").unwrap();
    let out = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

fn near(got: u64, want: f64, what: &str) {
    assert!(
        (got as f64 - want).abs() <= 1.0,
        "{what}: {got} frames, want {want}"
    );
}

#[test]
fn trim_crop_speed() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = clip(d.path(), "clip.mp4", "640x360", 6);

    let t = run(
        "video.trim",
        vec![src.clone()],
        Params::new().with("start", "1.0").with("end", "00:00:03.4"),
    );
    near(frames(&t), 2.4 * FPS, "trim");
    assert!(ffmpeg::probe(&t).unwrap().audio.is_some());

    let c = run(
        "video.crop",
        vec![src.clone()],
        Params::new()
            .with("x", 100)
            .with("y", 20)
            .with("w", 320)
            .with("h", 240),
    );
    let p = ffmpeg::probe(&c).unwrap();
    let v = p.video.unwrap();
    assert_eq!((v.width, v.height), (320, 240));
    assert!(p.audio.is_some(), "crop keeps audio");
    near(frames(&c), 6.0 * FPS, "crop");

    let sq = run(
        "video.crop",
        vec![src.clone()],
        Params::new().with("ratio", "1:1"),
    );
    let v = ffmpeg::probe(&sq).unwrap().video.unwrap();
    assert_eq!((v.width, v.height), (360, 360));

    let fast = run(
        "video.speed",
        vec![src.clone()],
        Params::new().with("factor", 2.0),
    );
    let p = ffmpeg::probe(&fast).unwrap();
    assert!((p.duration - 3.0).abs() < 0.1, "2× speed: {}", p.duration);
    near(frames(&fast), 3.0 * FPS, "speed");
    let slow = run("video.speed", vec![src], Params::new().with("factor", 0.5));
    assert!((ffmpeg::probe(&slow).unwrap().duration - 12.0).abs() < 0.15);
}

#[test]
fn join_mixed_clips_on_the_first_clips_canvas() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let a = clip(d.path(), "a.mp4", "640x360", 2);
    let b = clip(d.path(), "b.mp4", "320x240", 3);
    // Mixed container, no audio.
    let c = d.path().join("c.webm");
    let args: Vec<String> = [
        "-f",
        "lavfi",
        "-i",
        "testsrc2=s=480x480:r=30:d=1",
        "-c:v",
        "libvpx-vp9",
        "-b:v",
        "500k",
        &c.to_string_lossy(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    ffmpeg::run(&args, 0.0, &morph_core::Progress::silent()).unwrap();
    let j = run("video.join", vec![a, b, c], Params::new());
    let p = ffmpeg::probe(&j).unwrap();
    let v = p.video.unwrap();
    assert_eq!((v.width, v.height), (640, 360), "first clip is the canvas");
    assert!(
        (p.duration - 6.0).abs() < 0.15,
        "joined duration {}",
        p.duration
    );
    near(frames(&j), 6.0 * FPS, "join");
}

#[test]
fn frames_split_redact() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = clip(d.path(), "clip.mp4", "640x360", 6);

    let one = run(
        "video.frames",
        vec![src.clone()],
        Params::new().with("times", "2.5"),
    );
    assert_eq!(
        image::image_dimensions(&one).unwrap(),
        (640, 360),
        "full-res frame"
    );
    let many = run(
        "video.frames",
        vec![src.clone()],
        Params::new().with("times", "0, 1.5, 4"),
    );
    assert_eq!(std::fs::read_dir(&many).unwrap().count(), 3);

    let parts = run(
        "video.split",
        vec![src.clone()],
        Params::new().with("parts", 3),
    );
    let mut files: Vec<_> = std::fs::read_dir(&parts)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 3);
    for f in &files {
        near(frames(f), 2.0 * FPS, "equal part");
    }
    let at = run(
        "video.split",
        vec![src.clone()],
        Params::new().with("points", "1, 4.2").with("parts", 0),
    );
    let mut files: Vec<_> = std::fs::read_dir(&at)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    near(frames(&files[1]), 3.2 * FPS, "split at points");

    // Black box over the top-left during 1–3 s only.
    let regions =
        serde_json::json!([{ "x": 0, "y": 0, "w": 200, "h": 120, "start": 1.0, "end": 3.0 }]);
    let red = run(
        "video.redact",
        vec![src],
        Params::new().with("regions", regions),
    );
    near(frames(&red), 6.0 * FPS, "redact");
    let grab = |t: &str| {
        let f = run(
            "video.frames",
            vec![red.clone()],
            Params::new().with("times", t),
        );
        image::open(f).unwrap().to_rgb8()
    };
    assert!(
        grab("2").get_pixel(50, 50).0.iter().all(|&c| c < 20),
        "redacted inside the range"
    );
    assert!(
        grab("5").get_pixel(50, 50).0.iter().any(|&c| c > 40),
        "untouched outside the range"
    );
}
