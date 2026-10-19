//! S9: media conversion matrix and target-size compression. Needs FFmpeg
//! (`morph component install ffmpeg`, or ffmpeg/ffprobe on PATH); skipped otherwise.

use morph_core::{Job, Params, Runner};
use morph_engines::media::ffmpeg;
use std::path::{Path, PathBuf};

fn ready() -> bool {
    let ok = ffmpeg::available();
    if !ok {
        eprintln!("FFmpeg not found — skipping (morph component install ffmpeg)");
    }
    ok
}

/// Generate a fixture with ffmpeg's lavfi sources.
fn make(dir: &Path, name: &str, args: &[&str]) -> PathBuf {
    let out = dir.join(name);
    let mut full: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    full.push(out.to_string_lossy().into_owned());
    ffmpeg::run(&full, 0.0, &morph_core::Progress::silent()).unwrap();
    out
}

fn video_fixture(dir: &Path) -> PathBuf {
    make(
        dir,
        "clip.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=640x360:r=30:d=4",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=4",
            "-c:v",
            "libopenh264",
            "-b:v",
            "1500k",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
    )
}

fn run(op: &str, input: &Path, params: Params) -> PathBuf {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(
            &Job::new(op, vec![input.to_path_buf()]).params(params),
            |_| {},
        )
        .unwrap();
    assert!(r.ok(), "{op} on {}: {:?}", input.display(), r.errors);
    r.outputs[0].clone()
}

#[test]
fn video_conversion_matrix() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let mp4 = video_fixture(d.path());
    let webm = run("media.to-webm", &mp4, Params::new());
    for input in [&mp4, &webm] {
        let in_ext = input.extension().unwrap().to_string_lossy().to_string();
        for ext in ["mp4", "mov", "mkv", "webm", "gif", "mp3"] {
            if ext == in_ext {
                continue;
            }
            let out = run(&format!("media.to-{ext}"), input, Params::new());
            assert_eq!(out.extension().unwrap(), ext);
            let p = ffmpeg::probe(&out).unwrap();
            assert!(
                (p.duration - 4.0).abs() < 0.35,
                "{in_ext} → {ext}: duration {}",
                p.duration
            );
            if ext == "mp3" {
                assert!(
                    p.video.is_none() && p.audio.is_some(),
                    "{in_ext} → mp3 streams"
                );
            } else {
                assert!(p.video.is_some(), "{in_ext} → {ext}: no video");
                if ext != "gif" {
                    assert!(p.audio.is_some(), "{in_ext} → {ext}: lost audio");
                }
            }
        }
    }
    // GIF → MP4.
    let gif = run("media.to-gif", &mp4, Params::new());
    let back = run("media.to-mp4", &gif, Params::new());
    assert!(ffmpeg::probe(&back).unwrap().video.is_some());
}

#[test]
fn audio_conversion_matrix() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let wav = make(
        d.path(),
        "tone.wav",
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=f=330:d=3",
            "-ac",
            "2",
            "-c:a",
            "pcm_s16le",
        ],
    );
    let mp3 = run("media.to-mp3", &wav, Params::new());
    for input in [&wav, &mp3] {
        let in_ext = input.extension().unwrap().to_string_lossy().to_string();
        for ext in ["mp3", "m4a", "wav", "flac", "opus"] {
            if ext == in_ext {
                continue;
            }
            let out = run(&format!("media.to-{ext}"), input, Params::new());
            let p = ffmpeg::probe(&out).unwrap();
            assert!(p.audio.is_some(), "{in_ext} → {ext}: no audio");
            assert!(
                (p.duration - 3.0).abs() < 0.2,
                "{in_ext} → {ext}: duration {}",
                p.duration
            );
        }
    }
}

#[test]
fn compress_video_to_a_quarter_within_five_percent() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let big = make(
        d.path(),
        "big.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=1920x1080:r=30:d=12,noise=alls=30:allf=t",
            "-f",
            "lavfi",
            "-i",
            "sine=f=220:d=12",
            "-c:v",
            "libopenh264",
            "-b:v",
            "24000k",
            "-maxrate",
            "26000k",
            "-bufsize",
            "48000k",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
        ],
    );
    let size = std::fs::metadata(&big).unwrap().len();
    eprintln!("fixture {} MB", size / 1_000_000);
    let target = size / 4;
    let out = run(
        "media.compress",
        &big,
        Params::new().with("target", target.to_string()),
    );
    let got = std::fs::metadata(&out).unwrap().len();
    let err = got as f64 / target as f64 - 1.0;
    eprintln!("target {target}, got {got} ({:+.1} %)", err * 100.0);
    assert!(err.abs() <= 0.05, "{:+.1} % off target", err * 100.0);
    assert!((ffmpeg::probe(&out).unwrap().duration - 12.0).abs() < 0.4);
}

#[test]
fn compress_audio() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let wav = make(
        d.path(),
        "long.wav",
        &[
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=20:c=pink",
            "-ac",
            "2",
            "-c:a",
            "pcm_s16le",
        ],
    );
    let out = run(
        "media.compress",
        &wav,
        Params::new().with("target", "300KB"),
    );
    assert_eq!(out.extension().unwrap(), "mp3");
    assert!(std::fs::metadata(&out).unwrap().len() <= 315_000);
}
