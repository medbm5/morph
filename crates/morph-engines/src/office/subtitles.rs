//! Subtitles: SRT ↔ VTT, and plain-text export.

use anyhow::{bail, Result};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use std::path::PathBuf;

pub fn register(reg: &mut Registry) {
    for (ext, label, order) in [("srt", "SRT", 1), ("vtt", "VTT", 2), ("txt", "TXT", 3)] {
        let mut spec = OpSpec::convert(ext, label, &["ext:srt", "ext:vtt"]).order(order);
        spec.id = format!("subs.to-{ext}");
        reg.register(Subs { spec, ext });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

fn parse_ts(s: &str) -> Option<u64> {
    let s = s.trim().replace(',', ".");
    let (hms, ms) = s.split_once('.').unwrap_or((&s, "0"));
    let parts: Vec<u64> = hms
        .split(':')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let secs = match parts.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    let ms: u64 = format!("{:0<3}", &ms[..ms.len().min(3)]).parse().ok()?;
    Some(secs * 1000 + ms)
}

/// Parse SRT or WebVTT.
pub fn parse(text: &str) -> Result<Vec<Cue>> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(i) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let (a, b) = lines[i].split_once("-->").unwrap();
        // VTT cue settings follow the end time: "00:01.000 --> 00:02.000 align:start".
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start_ms), Some(end_ms)) = (parse_ts(a), parse_ts(b)) else {
            continue;
        };
        let text = lines[i + 1..].join("\n");
        cues.push(Cue {
            start_ms,
            end_ms,
            text,
        });
    }
    if cues.is_empty() {
        bail!("no subtitle cues found");
    }
    Ok(cues)
}

fn ts(ms: u64, sep: char) -> String {
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn to_srt(cues: &[Cue]) -> String {
    cues.iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "{}\n{} --> {}\n{}\n",
                i + 1,
                ts(c.start_ms, ','),
                ts(c.end_ms, ','),
                c.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn to_vtt(cues: &[Cue]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for c in cues {
        s.push_str(&format!(
            "{} --> {}\n{}\n\n",
            ts(c.start_ms, '.'),
            ts(c.end_ms, '.'),
            c.text
        ));
    }
    s
}

/// Plain text: cue text without timings or markup, one cue per paragraph.
pub fn to_txt(cues: &[Cue]) -> String {
    let strip = |t: &str| {
        let mut out = String::new();
        let mut tag = false;
        for ch in t.chars() {
            match ch {
                '<' => tag = true,
                '>' => tag = false,
                c if !tag => out.push(c),
                _ => {}
            }
        }
        out
    };
    cues.iter()
        .map(|c| strip(&c.text))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Subs {
    spec: OpSpec,
    ext: &'static str,
}

impl Operation for Subs {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let bytes = std::fs::read(ctx.input())?;
        let cues = parse(&String::from_utf8_lossy(&bytes))?;
        let text = match self.ext {
            "srt" => to_srt(&cues),
            "vtt" => to_vtt(&cues),
            _ => to_txt(&cues) + "\n",
        };
        let out = ctx.sink.file(ctx.input(), self.ext)?;
        std::fs::write(out.path(), text)?;
        Ok(vec![out.commit()?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "1\n00:00:01,000 --> 00:00:02,500\nHello <i>there</i>\n\n2\n00:01:00,250 --> 00:01:03,000\nTwo\nlines\n";

    #[test]
    fn round_trips() {
        let cues = parse(SRT).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[1].start_ms, 60_250);
        let vtt = to_vtt(&cues);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:01:00.250 --> 00:01:03.000"));
        assert_eq!(parse(&vtt).unwrap(), cues);
        assert_eq!(parse(&to_srt(&cues)).unwrap(), cues);
        assert_eq!(to_txt(&cues), "Hello there\nTwo\nlines");
    }

    #[test]
    fn vtt_short_timestamps_and_settings() {
        let cues = parse("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nHi\n").unwrap();
        assert_eq!((cues[0].start_ms, cues[0].end_ms), (1000, 2000));
    }
}
