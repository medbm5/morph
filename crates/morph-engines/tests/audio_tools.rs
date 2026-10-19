//! S11: normalize hits the target loudness (±0.5 LU) and every tool's output decodes.

use morph_core::{Job, Params, Runner};
use morph_engines::media::{audio, ffmpeg};
use std::path::{Path, PathBuf};

fn ready() -> bool {
    let ok = ffmpeg::available();
    if !ok {
        eprintln!("FFmpeg not found — skipping");
    }
    ok
}

fn make(dir: &Path, name: &str, args: &[&str]) -> PathBuf {
    let out = dir.join(name);
    let mut full: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    full.push(out.to_string_lossy().into_owned());
    ffmpeg::run(&full, 0.0, &morph_core::Progress::silent()).unwrap();
    out
}

fn run(op: &str, input: &Path, params: Params) -> PathBuf {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(
            &Job::new(op, vec![input.to_path_buf()]).params(params),
            |_| {},
        )
        .unwrap();
    assert!(r.ok(), "{op}: {:?}", r.errors);
    r.outputs[0].clone()
}

/// Decodes end to end without errors (what a player such as VLC needs).
fn plays(p: &Path) {
    let out = std::process::Command::new(ffmpeg::tool("ffmpeg").unwrap())
        .args(["-v", "error", "-i"])
        .arg(p)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success() && out.stderr.is_empty(),
        "{}: {}",
        p.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Speech-like test signal: pink noise bursts with silences, quiet overall.
fn speechy(dir: &Path, ext: &str, codec: &[&str]) -> PathBuf {
    let mut a = vec![
        "-f", "lavfi", "-i",
        "anoisesrc=d=12:c=pink:a=0.08,volume=volume='if(lt(mod(t,3),2),1,0.02)':eval=frame,adelay=1500|1500,apad=pad_dur=1.5",
        "-ac", "2",
    ];
    a.extend_from_slice(codec);
    make(dir, &format!("talk.{ext}"), &a)
}

#[test]
fn normalize_hits_target_within_half_an_lu() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    for (ext, codec) in [
        ("wav", &["-c:a", "pcm_s16le"][..]),
        ("mp3", &["-c:a", "libmp3lame", "-q:a", "2"][..]),
    ] {
        let src = speechy(d.path(), ext, codec);
        let before = audio::measure(&src).unwrap();
        for target in [-16.0, -23.0] {
            let out = run("audio.normalize", &src, Params::new().with("lufs", target));
            let after = audio::measure(&out).unwrap();
            eprintln!(
                "{ext}: {:.1} → {:.2} LUFS (target {target}), TP {:.1}",
                before.i, after.i, after.tp
            );
            assert!(
                (after.i - target).abs() <= 0.5,
                "{ext}: {} LUFS for target {target}",
                after.i
            );
            assert!(after.tp <= -1.0 + 0.3, "true peak {}", after.tp);
            plays(&out);
        }
    }
}

#[test]
fn every_audio_tool_output_plays() {
    if !ready() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let src = speechy(d.path(), "m4a", &["-c:a", "aac", "-b:a", "128k"]);
    let dur = ffmpeg::probe(&src).unwrap().duration;

    let t = run(
        "audio.trim",
        &src,
        Params::new().with("start", "2").with("end", "8"),
    );
    assert!((ffmpeg::probe(&t).unwrap().duration - 6.0).abs() < 0.1);
    plays(&t);

    let auto = run("audio.trim", &src, Params::new().with("silence", true));
    let auto_dur = ffmpeg::probe(&auto).unwrap().duration;
    assert!(auto_dur < dur - 2.0, "silence trimmed: {dur} → {auto_dur}");
    plays(&auto);

    let v = run(
        "audio.visualize",
        &src,
        Params::new().with("orientation", "portrait"),
    );
    let p = ffmpeg::probe(&v).unwrap();
    let vs = p.video.unwrap();
    assert_eq!((vs.width, vs.height), (1080, 1920));
    assert!(p.audio.is_some() && (p.duration - dur).abs() < 0.3);
    plays(&v);

    let peaks = audio::peaks(&src, 200).unwrap();
    assert_eq!(peaks.len(), 200);
    assert!(peaks.iter().any(|(lo, hi)| hi - lo > 0.05));
}
