//! Job graph for chained operations (PDF → JPG → Crop → BG).
//!
//! Intermediate results live in a private temp folder; only the last step writes
//! beside the source. When a step produces a folder (multi-page renders), the next
//! step runs on every file inside it and the final files land in one new folder.

use crate::job::{Job, JobEvent, JobReport, Runner};
use crate::op::Params;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub op: String,
    #[serde(default)]
    pub params: Params,
}

impl Step {
    pub fn new(op: &str) -> Self {
        Self {
            op: op.into(),
            params: Params::new(),
        }
    }
    pub fn params(mut self, p: Params) -> Self {
        self.params = p;
        self
    }
}

/// Files of a step's outputs, expanding folders (sorted, hidden files skipped).
fn expand(outputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in outputs {
        if p.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(p)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|f| {
                    f.is_file()
                        && !f
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                })
                .collect();
            files.sort();
            out.extend(files);
        } else {
            out.push(p.clone());
        }
    }
    out
}

impl Runner<'_> {
    /// Run `steps` over `inputs`, one chain per input file.
    pub fn run_chain(
        &self,
        steps: &[Step],
        inputs: &[PathBuf],
        on_event: impl Fn(JobEvent) + Send + Sync + 'static,
    ) -> Result<JobReport, Error> {
        if steps.is_empty() {
            return Err(Error::UnknownOp("empty chain".into()));
        }
        let on_event = Arc::new(on_event);
        let label = steps
            .iter()
            .map(|s| s.op.as_str())
            .collect::<Vec<_>>()
            .join(" → ");
        on_event(JobEvent::Started {
            op: label.clone(),
            total: inputs.len() * steps.len(),
        });
        let mut report = JobReport {
            op: label,
            ..Default::default()
        };
        let silent = |_e: JobEvent| {};

        for (n, input) in inputs.iter().enumerate() {
            let work = tempdir().map_err(|e| Error::Io(std::env::temp_dir(), e))?;
            let mut current = vec![input.clone()];
            let mut multi = false;
            let mut failed = false;
            for (i, step) in steps.iter().enumerate() {
                let last = i + 1 == steps.len();
                let files = expand(&current);
                multi |= files.len() > 1 || current.iter().any(|p| p.is_dir());
                let out_dir = if last {
                    if multi {
                        Some(
                            final_folder(input, &self.naming)
                                .map_err(|e| Error::Io(input.clone(), e))?,
                        )
                    } else {
                        None // beside the source
                    }
                } else {
                    let d = work.join(format!("step{i}"));
                    std::fs::create_dir_all(&d).map_err(|e| Error::Io(d.clone(), e))?;
                    Some(d)
                };
                // The last step writes beside the original, not beside the temp file.
                let out_dir = out_dir.or_else(|| input.parent().map(Path::to_path_buf));
                let kinds: Vec<crate::Kind> =
                    files.iter().filter_map(|f| crate::detect(f).ok()).collect();
                let op = self.registry.resolve(&step.op, &kinds).ok_or_else(|| {
                    Error::UnknownOp(format!(
                        "{} for {}",
                        step.op,
                        kinds.first().map(|k| k.label()).unwrap_or_default()
                    ))
                })?;
                let mut params = step.params.clone();
                // Positional value (`crop:1:1`) goes to the tool's first parameter.
                if let Some(v) = params.0.remove("_") {
                    if let Some(p) = op
                        .spec()
                        .params
                        .iter()
                        .find(|p| !matches!(p.kind, crate::ParamKind::Json))
                    {
                        params.0.insert(p.key.clone(), v);
                    }
                }
                let job = Job {
                    op: op.spec().id.clone(),
                    inputs: files,
                    params,
                    out_dir,
                };
                let r = self.run(&job, silent)?;
                on_event(JobEvent::Progress {
                    fraction: ((n * steps.len() + i + 1) as f32)
                        / (inputs.len() * steps.len()) as f32,
                    item: n,
                    total: inputs.len(),
                    message: Some(step.op.clone()),
                });
                if !r.errors.is_empty() || r.cancelled {
                    for (p, e) in r.errors {
                        on_event(JobEvent::Failed {
                            input: p.clone(),
                            error: format!("{}: {e}", step.op),
                        });
                        report.errors.push((p, e));
                    }
                    report.cancelled |= r.cancelled;
                    failed = true;
                    break;
                }
                current = r.outputs;
            }
            let _ = std::fs::remove_dir_all(&work);
            if failed {
                if report.cancelled {
                    break;
                }
                continue;
            }
            if multi {
                // Report the folder, not every page.
                if let Some(dir) = current.first().and_then(|p| p.parent()) {
                    on_event(JobEvent::Saved {
                        path: dir.to_path_buf(),
                    });
                    report.outputs.push(dir.to_path_buf());
                }
            } else {
                for p in current {
                    on_event(JobEvent::Saved { path: p.clone() });
                    report.outputs.push(p);
                }
            }
        }
        on_event(JobEvent::Finished {
            saved: report.outputs.len(),
            failed: report.errors.len(),
            cancelled: report.cancelled,
        });
        Ok(report)
    }
}

/// A fresh, collision-free folder beside `input` for a chain's multi-file result.
fn final_folder(input: &Path, naming: &crate::Naming) -> std::io::Result<PathBuf> {
    let dir = input
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = crate::output::stem_of(input);
    for n in 1..10_000 {
        let cand = dir.join(naming.numbered(&stem, n));
        if std::fs::create_dir(&cand).is_ok() {
            return Ok(cand);
        }
    }
    Err(std::io::Error::other("no free folder name"))
}

fn tempdir() -> std::io::Result<PathBuf> {
    crate::output::private_tempdir("morph-chain")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{OpContext, OpSpec, Operation, OutputKind};
    use crate::registry::Registry;

    /// Appends a line to a text file; `.split` variant writes a folder of two files.
    struct Append(OpSpec, &'static str, bool);
    impl Operation for Append {
        fn spec(&self) -> &OpSpec {
            &self.0
        }
        fn run(&self, ctx: &OpContext) -> anyhow::Result<Vec<PathBuf>> {
            let text = std::fs::read_to_string(ctx.input())? + self.1;
            if self.2 {
                let d = ctx.sink.folder(ctx.input())?;
                std::fs::write(d.path().join("a.txt"), &text)?;
                std::fs::write(d.path().join("b.txt"), &text)?;
                return Ok(vec![d.commit()?]);
            }
            let f = ctx.sink.file(ctx.input(), "txt")?;
            std::fs::write(f.path(), text)?;
            Ok(vec![f.commit()?])
        }
    }

    fn reg() -> Registry {
        let mut r = Registry::new();
        let t = |id: &str, suf: &str, out| OpSpec::tool(id, id, suf, &["ext:txt"], out);
        r.register(Append(t("t.a", "A", OutputKind::SameAsInput), "a", false));
        r.register(Append(t("t.b", "B", OutputKind::SameAsInput), "b", false));
        r.register(Append(t("t.split", "Pages", OutputKind::Folder), "s", true));
        r
    }

    #[test]
    fn chain_writes_only_the_final_result_beside_the_source() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("doc.txt");
        std::fs::write(&src, "x").unwrap();
        let reg = reg();
        let r = Runner::new(&reg)
            .run_chain(
                &[Step::new("t.a"), Step::new("t.b")],
                std::slice::from_ref(&src),
                |_| {},
            )
            .unwrap();
        assert!(r.ok(), "{:?}", r.errors);
        assert_eq!(r.outputs.len(), 1);
        assert_eq!(r.outputs[0].file_name().unwrap(), "doc A B.txt");
        assert_eq!(std::fs::read_to_string(&r.outputs[0]).unwrap(), "xab");
        let names: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "only source + final: {names:?}");
        assert_eq!(std::fs::read_to_string(&src).unwrap(), "x");
    }

    #[test]
    fn folder_steps_fan_out_into_one_final_folder() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("doc.txt");
        std::fs::write(&src, "x").unwrap();
        let reg = reg();
        let r = Runner::new(&reg)
            .run_chain(&[Step::new("t.split"), Step::new("t.a")], &[src], |_| {})
            .unwrap();
        assert!(r.ok(), "{:?}", r.errors);
        let folder = &r.outputs[0];
        assert!(folder.is_dir());
        let mut files: Vec<_> = std::fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(files, vec!["a A.txt", "b A.txt"]);
    }
}
