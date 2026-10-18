//! Recipes: saved chains (TOML) that appear as ring segments and run from the CLI.
//!
//! ```toml
//! name    = "Square post"
//! ring    = "tools"
//! accepts = ["application/pdf", "image/*"]
//! steps   = ["convert:jpg", "crop:1:1", "bg:preset=soft"]
//!
//! # or explicit steps:
//! [[step]]
//! op = "image.compress"
//! params = { target = "1MB" }
//! ```

use crate::chain::Step;
use crate::job::Runner;
use crate::op::{Arity, OpContext, OpSpec, Operation, OutputKind, Params, Ring};
use crate::registry::Registry;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeFile {
    pub name: String,
    #[serde(default = "default_ring")]
    pub ring: String,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Shorthand steps: `convert:jpg`, `crop:1:1`, `bg:preset=soft`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
    /// Explicit steps.
    #[serde(default, rename = "step", skip_serializing_if = "Vec::is_empty")]
    pub step: Vec<StepFile>,
    /// Command recipes (S19): `run = ["magick", "{input}", …, "{output}"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub run: Vec<String>,
    /// Output template for command recipes: `{stem} Thumb.webp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Parameters of a command recipe (`[[param]]`), shown in the editor and
    /// substituted into `run` as `{key}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param: Vec<crate::op::ParamSpec>,
}

fn default_ring() -> String {
    "tools".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFile {
    pub op: String,
    #[serde(default)]
    pub params: toml::Table,
}

impl RecipeFile {
    pub fn parse(text: &str) -> Result<Self> {
        let r: RecipeFile = toml::from_str(text).context("reading recipe TOML")?;
        if r.name.trim().is_empty() {
            bail!("recipe needs a name");
        }
        if r.steps.is_empty() && r.step.is_empty() && r.run.is_empty() {
            bail!("recipe `{}` has no steps", r.name);
        }
        Ok(r)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Steps as runnable (possibly symbolic) chain steps.
    pub fn chain(&self) -> Result<Vec<Step>> {
        let mut out: Vec<Step> = self
            .steps
            .iter()
            .map(|s| parse_step(s))
            .collect::<Result<_>>()?;
        for s in &self.step {
            let json = serde_json::to_value(&s.params)?;
            let params = match json {
                Value::Object(m) => Params(m),
                _ => Params::new(),
            };
            out.push(Step {
                op: s.op.clone(),
                params,
            });
        }
        Ok(out)
    }

    /// Stable id from the name: `recipe.square-post`.
    pub fn id(&self) -> String {
        let slug: String = self
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        format!("recipe.{slug}")
    }
}

/// Parse `convert:jpg`, `crop:1:1`, `bg:preset=soft`, `compress:target=1MB,format=webp`
/// or a plain operation id (`image.strip`).
pub fn parse_step(s: &str) -> Result<Step> {
    let s = s.trim();
    let Some((head, rest)) = s.split_once(':') else {
        return Ok(Step::new(s));
    };
    if head == "convert" {
        return Ok(Step::new(&format!(
            "convert:{}",
            rest.trim_start_matches('.').to_ascii_lowercase()
        )));
    }
    let mut params = Params::new();
    if !rest.is_empty() {
        if rest.contains('=') {
            for kv in rest.split(',') {
                let (k, v) = kv
                    .split_once('=')
                    .with_context(|| format!("`{kv}` should be key=value"))?;
                params.set(k.trim(), parse_value(v.trim()));
            }
        } else {
            // One positional value for the tool's first parameter (crop:1:1 → ratio).
            params.set("_", rest);
        }
    }
    Ok(Step {
        op: format!("tool:{head}"),
        params,
    })
}

fn parse_value(v: &str) -> Value {
    if let Ok(i) = v.parse::<i64>() {
        return i.into();
    }
    if let Ok(f) = v.parse::<f64>() {
        return f.into();
    }
    match v {
        "true" => true.into(),
        "false" => false.into(),
        _ => Value::String(v.into()),
    }
}

/// A recipe registered as an operation.
pub struct RecipeOp {
    spec: OpSpec,
    steps: Vec<Step>,
    base: Arc<Registry>,
    pub source: Option<PathBuf>,
}

impl RecipeOp {
    /// `base` holds the engine operations the steps call.
    pub fn new(file: &RecipeFile, base: Arc<Registry>, source: Option<PathBuf>) -> Result<Self> {
        let steps = file.chain()?;
        let ring = Ring::parse(&file.ring).unwrap_or(Ring::Tools);
        let accepts = if file.accepts.is_empty() {
            first_step_accepts(&steps[0], &base)
        } else {
            file.accepts.clone()
        };
        let mut spec = OpSpec::tool(&file.id(), &file.name, &file.name, &[], OutputKind::Folder)
            .arity(Arity::Each)
            .order(50);
        spec.ring = ring;
        spec.accepts = accepts;
        spec.output = OutputKind::None; // the chain decides
        Ok(Self {
            spec,
            steps,
            base,
            source,
        })
    }
}

fn first_step_accepts(step: &Step, reg: &Registry) -> Vec<String> {
    if let Some(op) = reg.get(&step.op) {
        return op.spec().accepts.clone();
    }
    // Symbolic first step: accept whatever some matching op accepts.
    let (kind, name) = step.op.split_once(':').unwrap_or(("", ""));
    let mut acc: Vec<String> = reg
        .all()
        .filter(|o| {
            let s = o.spec();
            match kind {
                "convert" => {
                    s.ring == Ring::Convert && matches!(&s.output, OutputKind::Ext(e) if e == name)
                }
                "tool" => crate::registry::tool_matches(s, name),
                _ => false,
            }
        })
        .flat_map(|o| o.spec().accepts.clone())
        .collect();
    acc.sort();
    acc.dedup();
    acc
}

impl Operation for RecipeOp {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> anyhow::Result<Vec<PathBuf>> {
        let mut runner = Runner::new(&self.base);
        runner.cancel = ctx.progress.cancel_flag();
        let progress = ctx.progress.clone();
        let report = runner.run_chain(&self.steps, &ctx.inputs, move |e| {
            if let crate::JobEvent::Progress {
                fraction, message, ..
            } = e
            {
                progress.message(fraction, message.as_deref().unwrap_or(""));
            }
        })?;
        if let Some((_, e)) = report.errors.first() {
            bail!("{e}");
        }
        Ok(report.outputs)
    }
}

/// Load every `*.toml` recipe in `dir` (bad files are reported, not fatal).
pub fn load_dir(dir: &Path) -> Vec<(PathBuf, Result<RecipeFile>)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        paths.sort();
        for p in paths {
            let r = std::fs::read_to_string(&p)
                .map_err(anyhow::Error::from)
                .and_then(|t| RecipeFile::parse(&t));
            out.push((p, r));
        }
    }
    out
}

/// The user's recipe folder (`~/.config/morph/recipes`, `%APPDATA%\morph\recipes`).
pub fn user_dir() -> PathBuf {
    crate::components::config_dir().join("recipes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_steps() {
        assert_eq!(parse_step("convert:JPG").unwrap().op, "convert:jpg");
        let crop = parse_step("crop:1:1").unwrap();
        assert_eq!(crop.op, "tool:crop");
        assert_eq!(crop.params.str("_"), Some("1:1"));
        let bg = parse_step("bg:preset=soft,padding=12").unwrap();
        assert_eq!(bg.params.str("preset"), Some("soft"));
        assert_eq!(bg.params.i64("padding"), Some(12));
        assert_eq!(parse_step("image.strip").unwrap().op, "image.strip");
    }

    #[test]
    fn recipe_round_trip() {
        let r = RecipeFile::parse(
            r#"
name = "Square post"
ring = "tools"
steps = ["convert:jpg", "crop:1:1"]

[[step]]
op = "image.compress"
params = { target = "1MB" }
"#,
        )
        .unwrap();
        assert_eq!(r.id(), "recipe.square-post");
        let chain = r.chain().unwrap();
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[2].params.str("target"), Some("1MB"));
        let again = RecipeFile::parse(&r.to_toml()).unwrap();
        assert_eq!(again.chain().unwrap(), chain);
    }

    #[test]
    fn rejects_empty_recipes() {
        assert!(RecipeFile::parse("name = \"x\"").is_err());
        assert!(RecipeFile::parse("steps = [\"convert:jpg\"]").is_err());
    }
}
