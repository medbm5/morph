//! Split trailing CLI arguments into input files and `--key value` parameters,
//! validated against an operation's parameter schema.

use anyhow::{bail, Result};
use morph_core::{OpSpec, ParamKind, Params};
use serde_json::Value;
use std::path::PathBuf;

/// Split `rest` into (files, params) for `spec`.
pub fn split(rest: &[String], spec: &OpSpec) -> Result<(Vec<PathBuf>, Params)> {
    let mut files = Vec::new();
    let mut params = Params::new();
    let mut it = rest.iter().peekable();
    while let Some(arg) = it.next() {
        if arg == "--" {
            files.extend(it.by_ref().cloned());
            break;
        }
        let Some(flag) = arg.strip_prefix("--") else {
            files.push(arg.clone());
            continue;
        };
        let (key, inline) = match flag.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (flag.to_string(), None),
        };
        let key = key.replace('-', "_");
        let Some(p) = spec.params.iter().find(|p| p.key == key) else {
            let known: Vec<_> = spec.params.iter().map(|p| format!("--{}", p.key)).collect();
            bail!(
                "`{}` has no option --{key}{}",
                spec.id,
                if known.is_empty() {
                    String::new()
                } else {
                    format!(" (options: {})", known.join(", "))
                }
            );
        };
        let raw = match (inline, &p.kind) {
            (Some(v), _) => v,
            // Bare boolean flag.
            (None, ParamKind::Bool)
                if it.peek().is_none_or(|n| n.starts_with("--") || !is_bool(n)) =>
            {
                "true".into()
            }
            (None, _) => match it.next() {
                Some(v) => v.clone(),
                None => bail!("--{key} needs a value"),
            },
        };
        params.set(&key, coerce(&raw, &p.kind)?);
    }
    if files.is_empty() {
        bail!("no input files");
    }
    Ok((expand(files)?, params))
}

fn is_bool(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "1" | "0"
    )
}

fn coerce(raw: &str, kind: &ParamKind) -> Result<Value> {
    Ok(match kind {
        ParamKind::Int { min, max } => {
            let v: i64 = raw
                .parse()
                .map_err(|_| anyhow::anyhow!("`{raw}` is not a whole number"))?;
            if v < *min || v > *max {
                bail!("{v} is outside {min}–{max}");
            }
            v.into()
        }
        ParamKind::Float { min, max, .. } => {
            let v: f64 = raw
                .parse()
                .map_err(|_| anyhow::anyhow!("`{raw}` is not a number"))?;
            if v < *min || v > *max {
                bail!("{v} is outside {min}–{max}");
            }
            v.into()
        }
        ParamKind::Bool => Value::Bool(matches!(
            raw.to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        )),
        ParamKind::Choice { options } => {
            let lower = raw.to_ascii_lowercase();
            match options.iter().find(|o| o.to_ascii_lowercase() == lower) {
                Some(o) => Value::String(o.clone()),
                None => bail!("`{raw}` is not one of: {}", options.join(", ")),
            }
        }
        ParamKind::Size => {
            if morph_core::op::parse_size(raw).is_none() {
                bail!("`{raw}` is not a size (try 1MB, 500KB)");
            }
            Value::String(raw.into())
        }
        ParamKind::Json => serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.into())),
        _ => Value::String(raw.into()),
    })
}

/// Expand glob patterns ourselves: Windows shells pass `*.png` through literally.
pub fn expand(args: impl IntoIterator<Item = String>) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for a in args {
        if a.contains(['*', '?', '[']) && !std::path::Path::new(&a).exists() {
            let mut matched: Vec<PathBuf> = glob::glob(&a)?.filter_map(Result::ok).collect();
            if matched.is_empty() {
                bail!("no files match `{a}`");
            }
            matched.sort();
            out.extend(matched);
        } else {
            let p = PathBuf::from(&a);
            if !p.exists() {
                bail!("{a}: no such file or folder");
            }
            out.push(p);
        }
    }
    Ok(out)
}
