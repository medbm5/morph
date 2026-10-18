//! Command recipes: user TOML files that wrap a local command.
//!
//! ```toml
//! name    = "Thumbnail 512"
//! ring    = "tools"
//! accepts = ["image/*"]
//! output  = "{stem} Thumb.webp"
//! run     = ["magick", "{input}", "-thumbnail", "{size}x{size}", "{output}"]
//!
//! [[param]]
//! key = "size"
//! label = "Size"
//! type = "int"
//! min = 16
//! max = 4096
//! default = 512
//! ```
//!
//! The command runs in a fresh staging folder next to the output, with `{output}`
//! pointing inside it, and is only allowed to write there: a low-integrity token on
//! Windows, Landlock on Linux. When it exits successfully the output is committed
//! like any other result (collision-free name, never replacing a file). Anything
//! else it wrote is discarded. A failure reports the command's stderr.

use anyhow::{anyhow, bail, Context, Result};
use morph_core::naming::sanitize;
use morph_core::op::{Arity, OpContext, OpSpec, Operation, OutputKind, Ring};
use morph_core::recipe::RecipeFile;
use std::path::{Path, PathBuf};

mod sandbox;

/// Whether recipe commands are confined to their output on this system.
pub fn confined() -> bool {
    sandbox::confined()
}

/// A recipe that runs a local command.
pub struct CommandOp {
    spec: OpSpec,
    run: Vec<String>,
    output: String,
}

impl CommandOp {
    pub fn new(file: &RecipeFile) -> Result<Self> {
        if file.run.is_empty() {
            bail!("recipe `{}` has no `run` command", file.name);
        }
        let output = file
            .output
            .clone()
            .unwrap_or_else(|| "{stem} {op}.{ext}".into());
        if output.contains('/') || output.contains('\\') || output.contains("..") {
            bail!("`output` must be a file name, not a path: `{output}`");
        }
        if !file.run.iter().any(|a| a.contains("{output}")) {
            bail!("`run` must write to {{output}}");
        }
        let ext = Path::new(&output)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| !e.contains('{'))
            .map(|e| e.to_ascii_lowercase());
        let kind = match ext {
            Some(e) => OutputKind::Ext(e),
            None => OutputKind::SameAsInput,
        };
        let accepts: Vec<String> = if file.accepts.is_empty() {
            vec!["*".into()]
        } else {
            file.accepts.clone()
        };
        let accepts: Vec<&str> = accepts.iter().map(String::as_str).collect();
        let mut spec = OpSpec::tool(&file.id(), &file.name, &file.name, &accepts, kind)
            .arity(Arity::Each)
            .order(60);
        spec.ring = Ring::parse(&file.ring).unwrap_or(Ring::Tools);
        for p in &file.param {
            spec = spec.param(p.clone());
        }
        Ok(Self {
            spec,
            run: file.run.clone(),
            output,
        })
    }
}

/// Fill `{input}`, `{output}`, `{stem}`, `{ext}`, `{dir}`, `{date}` and parameters.
fn fill(template: &str, vars: &[(String, String)]) -> String {
    let mut s = template.to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

impl Operation for CommandOp {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }

    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let stem = morph_core::output::stem_of(input);
        let ext = ctx.kind().ext.clone();
        let mut defaults = morph_core::Params::new();
        for p in &self.spec.params {
            defaults.set(&p.key, p.default.clone());
        }
        let params = ctx.params.clone().merged_over(&defaults);
        let mut vars: Vec<(String, String)> = vec![
            ("stem".into(), stem.clone()),
            ("ext".into(), ext),
            ("op".into(), self.spec.label.clone()),
            (
                "date".into(),
                morph_core::naming::timestamp()[..10].to_string(),
            ),
        ];
        for p in &self.spec.params {
            let v = match params.get(&p.key) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => String::new(),
            };
            vars.push((p.key.clone(), v));
        }
        let name = sanitize(&fill(&self.output, &vars));

        let dir = ctx.sink.dir_for(input);
        std::fs::create_dir_all(&dir)?;
        let stage = tempdir_in(&dir)?;
        let staged = stage.path().join(&name);
        vars.push(("input".into(), input.display().to_string()));
        vars.push(("output".into(), staged.display().to_string()));
        vars.push((
            "dir".into(),
            input
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        ));
        let argv: Vec<String> = self.run.iter().map(|a| fill(a, &vars)).collect();
        log::info!("recipe `{}`: {:?}", self.spec.id, argv);

        ctx.progress.message(0.05, &format!("running {}", argv[0]));
        let out = sandbox::run(&argv, stage.path(), &ctx.progress.cancel_flag())
            .with_context(|| format!("starting `{}`", argv[0]))?;
        if ctx.progress.is_cancelled() {
            bail!(morph_core::Error::Cancelled);
        }
        if out.code != Some(0) {
            let tail = tail_lines(&out.stderr, 20);
            bail!(
                "`{}` failed ({}){}",
                argv[0],
                out.code
                    .map_or("killed".to_string(), |c| format!("exit code {c}")),
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(":\n{tail}")
                }
            );
        }
        if !staged.is_file() {
            let tail = tail_lines(&out.stderr, 20);
            bail!(
                "`{}` didn't write {{output}} ({name}){}",
                argv[0],
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(":\n{tail}")
                }
            );
        }
        let pending = ctx.sink.file_named(&dir, &name)?;
        std::fs::rename(&staged, pending.path())?;
        let saved = pending.commit()?;
        Ok(vec![saved])
    }
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// A hidden staging folder in `dir`, removed on drop.
struct Stage(PathBuf);

impl Stage {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tempdir_in(dir: &Path) -> Result<Stage> {
    for n in 0..100u32 {
        let p = dir.join(format!(
            ".morph-run-{}-{}",
            std::process::id(),
            n + rand_seed()
        ));
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(Stage(p)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(anyhow!(
                    "creating a staging folder in {}: {e}",
                    dir.display()
                ))
            }
        }
    }
    bail!("no free staging folder name in {}", dir.display())
}

fn rand_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
}

/// Build the operation for a recipe file: a chain, or a command when it has `run`.
pub fn recipe_op(
    f: &RecipeFile,
    base: std::sync::Arc<morph_core::Registry>,
    path: &Path,
) -> Result<std::sync::Arc<dyn Operation>> {
    if !f.run.is_empty() {
        return Ok(std::sync::Arc::new(CommandOp::new(f)?));
    }
    Ok(std::sync::Arc::new(morph_core::recipe::RecipeOp::new(
        f,
        base,
        Some(path.to_path_buf()),
    )?))
}
