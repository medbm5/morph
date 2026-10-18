//! The `Operation` trait and its declarative spec.
//!
//! An operation declares which kinds it accepts, what it produces and which
//! parameters it takes. The ring UI, the editors and the CLI flags are all built
//! from this spec, so a new engine operation needs no UI code.

use crate::kind::Kind;
use crate::output::OutputSink;
use crate::progress::Progress;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Which ring an operation appears in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ring {
    Convert,
    Tools,
    Actions,
}

impl Ring {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "convert" => Some(Ring::Convert),
            "tools" | "tool" | "command" => Some(Ring::Tools),
            "actions" | "action" => Some(Ring::Actions),
            _ => None,
        }
    }
}

/// How an operation consumes a multi-file selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arity {
    /// Run once per selected file (conversions, most tools).
    Each,
    /// Run once over the whole selection (merge, images → PDF, collage, archive).
    All,
}

/// What an operation writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "value")]
pub enum OutputKind {
    /// A file with this extension.
    Ext(String),
    /// Same extension as the input.
    SameAsInput,
    /// A folder of files (multi-page renders, extraction, split).
    Folder,
    /// Nothing on disk (Copy path, Share, Open in terminal).
    None,
}

/// A parameter type, rendered as a control in the editor and as a CLI flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum ParamKind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
    },
    Text,
    /// One of a fixed set of values.
    Choice {
        options: Vec<String>,
    },
    /// A byte size such as `1MB`, `500KB`.
    Size,
    /// `#RRGGBB` or `#RRGGBBAA`.
    Color,
    /// An aspect ratio such as `1:1`, `16:9`, or `free`.
    Ratio,
    /// A timecode or seconds (`00:01:02.5`, `62.5`).
    Time,
    /// Free-form JSON (editor state such as annotation shapes or redaction boxes).
    Json,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Key in the params map; also the CLI flag (`--target`, `--dpi`).
    pub key: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: ParamKind,
    pub default: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl ParamSpec {
    pub fn new(key: &str, label: &str, kind: ParamKind, default: Value) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            default,
            help: None,
        }
    }
    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Declarative description of an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpSpec {
    /// Stable id, e.g. `convert.png`, `image.compress`, `pdf.merge`.
    pub id: String,
    /// Segment label, e.g. `PNG`, `Compress`.
    pub label: String,
    pub ring: Ring,
    /// Patterns accepted for each input (see [`Kind::matches`]).
    pub accepts: Vec<String>,
    pub output: OutputKind,
    /// Word appended to tool outputs: `Page 001 Cropped.jpg`. `None` for conversions.
    pub suffix: Option<String>,
    pub arity: Arity,
    pub params: Vec<ParamSpec>,
    /// Editor id the app opens before running, if the op needs one (`crop`, `annotate`…).
    pub editor: Option<String>,
    /// Optional component needed at runtime (`heif`, `libreoffice`, `ml`, `ffmpeg`…).
    pub component: Option<String>,
    /// Lower sorts first in the ring.
    pub order: i32,
}

impl OpSpec {
    /// A conversion to `ext` for inputs matching `accepts`.
    pub fn convert(ext: &str, label: &str, accepts: &[&str]) -> Self {
        Self {
            id: format!("convert.{ext}"),
            label: label.into(),
            ring: Ring::Convert,
            accepts: accepts.iter().map(|s| s.to_string()).collect(),
            output: OutputKind::Ext(ext.into()),
            suffix: None,
            arity: Arity::Each,
            params: vec![],
            editor: None,
            component: None,
            order: 100,
        }
    }

    /// A tool that writes `{stem} {suffix}.{ext}`.
    pub fn tool(id: &str, label: &str, suffix: &str, accepts: &[&str], output: OutputKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ring: Ring::Tools,
            accepts: accepts.iter().map(|s| s.to_string()).collect(),
            output,
            suffix: Some(suffix.into()),
            arity: Arity::Each,
            params: vec![],
            editor: None,
            component: None,
            order: 100,
        }
    }

    pub fn action(id: &str, label: &str, accepts: &[&str]) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ring: Ring::Actions,
            accepts: accepts.iter().map(|s| s.to_string()).collect(),
            output: OutputKind::None,
            suffix: None,
            arity: Arity::All,
            params: vec![],
            editor: None,
            component: None,
            order: 100,
        }
    }

    pub fn arity(mut self, a: Arity) -> Self {
        self.arity = a;
        self
    }
    pub fn param(mut self, p: ParamSpec) -> Self {
        self.params.push(p);
        self
    }
    pub fn editor(mut self, e: &str) -> Self {
        self.editor = Some(e.into());
        self
    }
    pub fn component(mut self, c: &str) -> Self {
        self.component = Some(c.into());
        self
    }
    pub fn order(mut self, o: i32) -> Self {
        self.order = o;
        self
    }
    pub fn label(mut self, l: &str) -> Self {
        self.label = l.into();
        self
    }

    /// Does this op accept a file of this kind?
    pub fn accepts_kind(&self, kind: &Kind) -> bool {
        self.accepts.iter().any(|p| kind.matches(p))
    }

    /// Default values for every parameter.
    pub fn default_params(&self) -> Params {
        let mut m = Map::new();
        for p in &self.params {
            m.insert(p.key.clone(), p.default.clone());
        }
        Params(m)
    }
}

/// Parameter values for one run. Missing keys fall back to the spec defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Params(pub Map<String, Value>);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        self.0.insert(key.into(), value.into());
    }
    /// Fill missing keys from `defaults`.
    pub fn merged_over(mut self, defaults: &Params) -> Self {
        for (k, v) in &defaults.0 {
            self.0.entry(k.clone()).or_insert_with(|| v.clone());
        }
        self
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key).filter(|v| !v.is_null())
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }
    pub fn f64(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    pub fn i64(&self, key: &str) -> Option<i64> {
        self.f64(key).map(|f| f.round() as i64)
    }
    pub fn u32(&self, key: &str) -> Option<u32> {
        self.i64(key).and_then(|v| u32::try_from(v).ok())
    }
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Value::Bool(b) => Some(*b),
            Value::String(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Some(true),
                "false" | "no" | "0" | "off" => Some(false),
                _ => None,
            },
            Value::Number(n) => n.as_f64().map(|f| f != 0.0),
            _ => None,
        }
    }
    /// A byte size: number of bytes, or a string like `1MB`, `500 KB`, `1.5mb`.
    pub fn size(&self, key: &str) -> Option<u64> {
        match self.get(key)? {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => parse_size(s),
            _ => None,
        }
    }
}

/// Parse `1MB`, `500 KB`, `1.5mb`, `2048` into bytes (1 KB = 1000 B, KiB = 1024 B).
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase().replace(' ', "");
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().ok()?;
    let mult = match unit {
        "" | "b" => 1.0,
        "k" | "kb" => 1e3,
        "kib" => 1024.0,
        "m" | "mb" => 1e6,
        "mib" => 1024.0 * 1024.0,
        "g" | "gb" => 1e9,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult).round() as u64)
}

/// Everything an operation gets for one run.
pub struct OpContext<'a> {
    /// One file for [`Arity::Each`], the whole selection for [`Arity::All`].
    pub inputs: Vec<PathBuf>,
    pub kinds: Vec<Kind>,
    pub params: Params,
    /// Where outputs go: allocates collision-free names, writes atomically.
    pub sink: &'a OutputSink,
    pub progress: &'a Progress,
}

impl OpContext<'_> {
    pub fn input(&self) -> &PathBuf {
        &self.inputs[0]
    }
    pub fn kind(&self) -> &Kind {
        &self.kinds[0]
    }
}

/// An operation an engine provides.
pub trait Operation: Send + Sync {
    fn spec(&self) -> &OpSpec;

    /// Is the operation usable right now (component installed, sidecar found)?
    /// Unavailable ops still show in the ring, as `INSTALL COMPONENT`.
    fn available(&self) -> bool {
        true
    }

    /// Run the operation and return the paths it saved.
    /// Implementations must never open an input for writing.
    fn run(&self, ctx: &OpContext) -> anyhow::Result<Vec<PathBuf>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1MB"), Some(1_000_000));
        assert_eq!(parse_size("500 KB"), Some(500_000));
        assert_eq!(parse_size("1.5mb"), Some(1_500_000));
        assert_eq!(parse_size("2KiB"), Some(2048));
        assert_eq!(parse_size("12"), Some(12));
        assert_eq!(parse_size("abc"), None);
    }

    #[test]
    fn params_merge_and_coerce() {
        let spec = OpSpec::convert("png", "PNG", &["image/*"]).param(ParamSpec::new(
            "dpi",
            "DPI",
            ParamKind::Int { min: 72, max: 600 },
            300.into(),
        ));
        let p = Params::new()
            .with("quality", "85")
            .merged_over(&spec.default_params());
        assert_eq!(p.u32("dpi"), Some(300));
        assert_eq!(p.u32("quality"), Some(85));
    }
}
