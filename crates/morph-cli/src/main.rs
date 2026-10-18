//! `morph` — every ring operation, from a shell.

mod args;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use morph_core::{detect, Job, JobEvent, Kind, Registry, Ring, Runner};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "morph",
    version = morph_core::VERSION,
    about = "Metamorphosis — convert and edit files without touching the original",
    after_help = "Examples:\n  morph convert --to jpg file.pdf\n  morph convert --to webp *.png\n  morph list --for clip.mov"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert files to another format, saved beside each original.
    #[command(disable_help_flag = true)]
    Convert {
        /// Target format (jpg, png, webp, pdf, mp3…).
        #[arg(long)]
        to: String,
        /// Write outputs to this folder instead of beside each source.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Files, then any format options (`--quality 80`, `--dpi 300`). Run
        /// `morph list --for <file>` to see the options.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        rest: Vec<String>,
    },
    /// Run a tool (compress, crop, metadata, strip…), saved beside each original.
    #[command(disable_help_flag = true)]
    Tool {
        /// Tool name, e.g. compress, crop, strip.
        name: String,
        /// Write outputs to this folder instead of beside each source.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Files and tool options (`--target 1MB`, `--ratio 1:1`).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        rest: Vec<String>,
    },
    /// Run a recipe (built-in or from your recipes folder) on files.
    Run {
        /// Recipe name or id, e.g. "Square post" or square-post.
        recipe: String,
        #[arg(required = true, num_args = 1..)]
        files: Vec<String>,
    },
    /// Chain operations: `morph chain report.pdf "convert:jpg" "crop:1:1" "bg:preset=soft"`.
    Chain {
        /// The file to start from.
        file: String,
        /// Steps: convert:<format>, <tool>:<value> or <tool>:key=value,…
        #[arg(required = true, num_args = 1..)]
        steps: Vec<String>,
    },
    /// Manage optional components (local AI, OCR, FFmpeg, pandoc…).
    Component {
        #[command(subcommand)]
        action: ComponentAction,
    },
    /// List the operations available for some files.
    List {
        /// Files to check.
        #[arg(long = "for", required = true, num_args = 1..)]
        files: Vec<PathBuf>,
        /// Only this ring (convert, tools, actions).
        #[arg(long)]
        ring: Option<String>,
        /// Show parameters.
        #[arg(long, short)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
enum ComponentAction {
    /// Show every component and whether it is installed.
    List,
    /// Download, verify (SHA-256) and install a component.
    Install { name: String },
    /// Remove an installed component.
    Remove { name: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (registry, recipe_errors) = morph_engines::registry_with_recipes();
    for (path, e) in recipe_errors {
        eprintln!("morph: skipping recipe {}: {e}", path.display());
    }
    match run(cli, &registry) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("morph: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli, registry: &Registry) -> Result<bool> {
    match cli.command {
        Command::Convert { to, out_dir, rest } => {
            let to = canonical_ext(&to);
            // Options depend on the converter, which depends on the input type.
            let (_files, kinds) = input_files(&rest)?;
            let op = registry.converter(&to, &kinds).with_context(|| {
                let labels: Vec<_> = kinds.iter().map(Kind::label).collect();
                format!("can't convert {} to .{to}", labels.join(", "))
            })?;
            let (files, params) = args::split(&rest, op.spec())?;
            run_job(
                registry,
                Job {
                    op: op.spec().id.clone(),
                    inputs: files,
                    params,
                    out_dir,
                },
            )
        }
        Command::Run { recipe, files } => {
            let want = recipe.to_lowercase();
            let op = registry
                .all()
                .find(|o| {
                    let s = o.spec();
                    s.id.starts_with("recipe.")
                        && (s.label.to_lowercase() == want
                            || s.id == format!("recipe.{want}")
                            || s.id == want)
                })
                .with_context(|| format!("no recipe `{recipe}` (see `morph list --for <file>`)"))?;
            let files = args::expand(files)?;
            run_job(registry, Job::new(&op.spec().id.clone(), files))
        }
        Command::Chain { file, steps } => {
            let files = args::expand([file])?;
            let steps: Vec<morph_core::Step> = steps
                .iter()
                .map(|s| morph_core::recipe::parse_step(s))
                .collect::<Result<_>>()?;
            let runner = Runner::new(registry);
            let report = runner.run_chain(&steps, &files, |ev| match ev {
                JobEvent::Saved { path } => println!("✓ SAVED · {}", path.display()),
                JobEvent::Failed { error, .. } => eprintln!("✗ {error}"),
                _ => {}
            })?;
            Ok(report.ok())
        }
        Command::Component { action } => {
            component(action)?;
            Ok(true)
        }
        Command::Tool {
            name,
            out_dir,
            rest,
        } => {
            let (_files, kinds) = input_files(&rest)?;
            let op = find_tool(registry, &name, &kinds)?;
            let (files, params) = args::split(&rest, op.spec())?;
            run_job(
                registry,
                Job {
                    op: op.spec().id.clone(),
                    inputs: files,
                    params,
                    out_dir,
                },
            )
        }
        Command::List {
            files,
            ring,
            verbose,
        } => {
            list(registry, &files, ring.as_deref(), verbose)?;
            Ok(true)
        }
    }
}

fn component(action: ComponentAction) -> Result<()> {
    match action {
        ComponentAction::List => {
            for c in morph_updater::status() {
                let mark = if c.installed { "installed" } else { "-" };
                println!(
                    "{:<10} {:<10} {:>8}  {}",
                    c.name,
                    mark,
                    format!("{:.1} MB", c.download_size as f64 / 1e6),
                    c.title
                );
                if let Some(h) = c.system_hint {
                    println!("{:<21}{h}", "");
                }
            }
            println!("\nfolder: {}", morph_core::components::root().display());
        }
        ComponentAction::Install { name } => {
            let fancy = std::io::stderr().is_terminal();
            morph_updater::install(&name, &move |done, total, _| {
                if fancy {
                    eprint!(
                        "\r{:>5.1} / {:.1} MB",
                        done as f64 / 1e6,
                        total as f64 / 1e6
                    );
                }
            })?;
            if fancy {
                eprintln!();
            }
            println!("✓ installed {name} (checksums verified)");
        }
        ComponentAction::Remove { name } => {
            morph_updater::remove(&name)?;
            println!("removed {name}");
        }
    }
    Ok(())
}

/// Input files among trailing arguments: those that exist or are globs.
fn input_files(rest: &[String]) -> Result<(Vec<PathBuf>, Vec<Kind>)> {
    let files = args::expand(
        rest.iter()
            .filter(|a| {
                !a.starts_with("--") && (std::path::Path::new(a).exists() || a.contains(['*', '?']))
            })
            .cloned(),
    )?;
    if files.is_empty() {
        anyhow::bail!("no input files");
    }
    let kinds = files
        .iter()
        .map(|f| detect(f).with_context(|| format!("reading {}", f.display())))
        .collect::<Result<_>>()?;
    Ok((files, kinds))
}

/// Resolve a tool name (`crop`, `compress`, `strip`) for these inputs.
fn find_tool<'r>(
    registry: &'r Registry,
    name: &str,
    kinds: &[Kind],
) -> Result<&'r std::sync::Arc<dyn morph_core::Operation>> {
    registry.find_tool(name, kinds).with_context(|| {
        let labels: Vec<_> = kinds.iter().map(Kind::label).collect();
        format!(
            "no tool `{name}` for {} files (see `morph list --for <file>`)",
            labels.join(", ")
        )
    })
}

/// Map common aliases to our canonical extensions.
pub fn canonical_ext(s: &str) -> String {
    let s = s.trim_start_matches('.').to_ascii_lowercase();
    match s.as_str() {
        "jpeg" | "jpe" => "jpg".into(),
        "tif" => "tiff".into(),
        "markdown" => "md".into(),
        "htm" => "html".into(),
        "text" => "txt".into(),
        "aif" => "aiff".into(),
        _ => s,
    }
}

pub fn run_job(registry: &Registry, job: Job) -> Result<bool> {
    let fancy = std::io::stderr().is_terminal();
    let runner = Runner::new(registry);
    let report = runner.run(&job, move |ev| match ev {
        JobEvent::Saved { path } => println!("✓ SAVED · {}", path.display()),
        JobEvent::Failed { input, error } => match input {
            Some(p) => eprintln!("✗ {}: {error}", p.display()),
            None => eprintln!("✗ {error}"),
        },
        JobEvent::Progress {
            fraction,
            item,
            total,
            ..
        } if fancy => {
            eprint!(
                "\r{:>3}%  {}/{}  ",
                (fraction * 100.0) as u32,
                (item + 1).min(total),
                total
            );
        }
        JobEvent::Finished { .. } if fancy => eprint!("\r                    \r"),
        _ => {}
    })?;
    if report.cancelled {
        bail!("cancelled");
    }
    Ok(report.errors.is_empty())
}

fn list(registry: &Registry, files: &[PathBuf], ring: Option<&str>, verbose: bool) -> Result<()> {
    let files = args::expand(files.iter().map(|p| p.to_string_lossy().into_owned()))?;
    let kinds: Vec<Kind> = files
        .iter()
        .map(|f| detect(f).with_context(|| format!("reading {}", f.display())))
        .collect::<Result<_>>()?;
    let labels: Vec<_> = kinds.iter().map(Kind::label).collect();
    println!("{} file(s): {}", files.len(), labels.join(", "));
    let rings = match ring {
        Some(r) => vec![Ring::parse(r).with_context(|| format!("unknown ring `{r}`"))?],
        None => vec![Ring::Convert, Ring::Tools, Ring::Actions],
    };
    for ring in rings {
        let ops = registry.for_selection(&kinds, ring);
        if ops.is_empty() {
            continue;
        }
        println!("\n{}", format!("{ring:?}").to_uppercase());
        for op in ops {
            let s = op.spec();
            let note = if op.available() {
                ""
            } else {
                "  (INSTALL COMPONENT)"
            };
            println!("  {:<22} {}{note}", s.id, s.label);
            if verbose {
                for p in &s.params {
                    println!("      --{:<16} {} [default: {}]", p.key, p.label, p.default);
                }
            }
        }
    }
    Ok(())
}
