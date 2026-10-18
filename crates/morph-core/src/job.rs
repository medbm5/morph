//! Job runner: runs one operation over a selection and reports progress.

use crate::kind::{detect, Kind};
use crate::naming::Naming;
use crate::op::{Arity, OpContext, Params};
use crate::output::OutputSink;
use crate::progress::Progress;
use crate::registry::Registry;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A request to run one operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub op: String,
    pub inputs: Vec<PathBuf>,
    #[serde(default)]
    pub params: Params,
    /// Write outputs here instead of beside each source.
    #[serde(default)]
    pub out_dir: Option<PathBuf>,
}

impl Job {
    pub fn new(op: &str, inputs: Vec<PathBuf>) -> Self {
        Self {
            op: op.into(),
            inputs,
            params: Params::new(),
            out_dir: None,
        }
    }
    pub fn params(mut self, p: Params) -> Self {
        self.params = p;
        self
    }
}

/// Events emitted while a job runs (the app turns them into the toast / progress card).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum JobEvent {
    /// `total` units of work (files for per-file ops, 1 for whole-selection ops).
    Started {
        op: String,
        total: usize,
    },
    /// Overall fraction 0–1 and the item being processed.
    Progress {
        fraction: f32,
        item: usize,
        total: usize,
        message: Option<String>,
    },
    Saved {
        path: PathBuf,
    },
    Failed {
        input: Option<PathBuf>,
        error: String,
    },
    Finished {
        saved: usize,
        failed: usize,
        cancelled: bool,
    },
}

/// Outcome of a job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobReport {
    pub op: String,
    pub outputs: Vec<PathBuf>,
    pub errors: Vec<(Option<PathBuf>, String)>,
    pub cancelled: bool,
}

impl JobReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty() && !self.cancelled
    }
}

/// Runs jobs against a registry.
pub struct Runner<'r> {
    pub registry: &'r Registry,
    pub naming: Naming,
    pub cancel: Arc<AtomicBool>,
}

impl<'r> Runner<'r> {
    pub fn new(registry: &'r Registry) -> Self {
        Self {
            registry,
            naming: Naming::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn naming(mut self, naming: Naming) -> Self {
        self.naming = naming;
        self
    }

    /// Run `job`, calling `on_event` from this thread as work progresses.
    pub fn run(
        &self,
        job: &Job,
        on_event: impl Fn(JobEvent) + Send + Sync + 'static,
    ) -> Result<JobReport, Error> {
        let op = self
            .registry
            .get(&job.op)
            .ok_or_else(|| Error::UnknownOp(job.op.clone()))?;
        let spec = op.spec();
        if job.inputs.is_empty() {
            return Err(Error::NoInputs);
        }
        if !op.available() {
            return Err(Error::ComponentMissing(
                spec.component.clone().unwrap_or_else(|| spec.id.clone()),
            ));
        }

        let mut kinds = Vec::with_capacity(job.inputs.len());
        for p in &job.inputs {
            let k = detect(p).map_err(|e| Error::Io(p.clone(), e))?;
            if !spec.accepts_kind(&k) {
                return Err(Error::Unsupported {
                    op: spec.id.clone(),
                    path: p.clone(),
                    kind: k.label(),
                });
            }
            kinds.push(k);
        }

        let params = job.params.clone().merged_over(&spec.default_params());
        let sink = OutputSink::new(
            self.naming.clone(),
            spec.suffix.clone(),
            job.out_dir.clone(),
        );
        let groups: Vec<(Vec<PathBuf>, Vec<Kind>)> = match spec.arity {
            Arity::Each => job
                .inputs
                .iter()
                .cloned()
                .zip(kinds)
                .map(|(p, k)| (vec![p], vec![k]))
                .collect(),
            Arity::All => vec![(job.inputs.clone(), kinds)],
        };
        let total = groups.len();
        let on_event = Arc::new(on_event);
        on_event(JobEvent::Started {
            op: spec.id.clone(),
            total,
        });

        let mut report = JobReport {
            op: spec.id.clone(),
            ..Default::default()
        };
        for (i, (inputs, kinds)) in groups.into_iter().enumerate() {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            let ev = on_event.clone();
            let progress = Progress::new(self.cancel.clone(), move |f, msg| {
                ev(JobEvent::Progress {
                    fraction: (i as f32 + f) / total as f32,
                    item: i,
                    total,
                    message: msg.map(str::to_string),
                })
            });
            let first = inputs.first().cloned();
            let before = sink.saved().len();
            let ctx = OpContext {
                inputs,
                kinds,
                params: params.clone(),
                sink: &sink,
                progress: &progress,
            };
            match op.run(&ctx) {
                Ok(paths) => {
                    // Prefer what the op returned; fall back to what it committed.
                    let paths = if paths.is_empty() {
                        sink.saved()[before..].to_vec()
                    } else {
                        paths
                    };
                    for p in paths {
                        on_event(JobEvent::Saved { path: p.clone() });
                        report.outputs.push(p);
                    }
                }
                Err(e)
                    if e.downcast_ref::<Error>()
                        .is_some_and(|e| matches!(e, Error::Cancelled)) =>
                {
                    report.cancelled = true;
                    break;
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    on_event(JobEvent::Failed {
                        input: first.clone(),
                        error: msg.clone(),
                    });
                    report.errors.push((first, msg));
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
