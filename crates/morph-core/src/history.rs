//! Job history: what ran, on what, and what it saved — for Undo, "Run again",
//! "Show in folder" and turning drag-chains into recipes.

use crate::chain::Step;
use crate::op::Params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const KEEP: usize = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub op: String,
    pub label: String,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
    #[serde(default)]
    pub params: Params,
    /// Unix seconds.
    pub at: u64,
    /// The job whose output was this job's input (a drag-chain).
    #[serde(default)]
    pub parent: Option<u64>,
    #[serde(default)]
    pub undone: bool,
}

pub struct History {
    path: PathBuf,
}

impl Default for History {
    fn default() -> Self {
        Self::at(crate::components::data_dir().join("history.json"))
    }
}

impl History {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Vec<Entry> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, entries: &[Entry]) -> std::io::Result<()> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(entries)?)?;
        std::fs::rename(tmp, &self.path)
    }

    /// Record a finished job; links it to the job that produced its input, if any.
    pub fn record(
        &self,
        op: &str,
        label: &str,
        inputs: &[PathBuf],
        outputs: &[PathBuf],
        params: &Params,
    ) -> std::io::Result<Entry> {
        let mut all = self.load();
        let id = all.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let parent = inputs.first().and_then(|i| {
            all.iter()
                .rev()
                .find(|e| !e.undone && e.outputs.iter().any(|o| o == i || i.starts_with(o)))
                .map(|e| e.id)
        });
        let entry = Entry {
            id,
            op: op.into(),
            label: label.into(),
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
            params: params.clone(),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            parent,
            undone: false,
        };
        all.push(entry.clone());
        if all.len() > KEEP {
            let drop = all.len() - KEEP;
            all.drain(..drop);
        }
        self.save(&all)?;
        Ok(entry)
    }

    /// The most recent job that can still be undone.
    pub fn last_undoable(&self) -> Option<Entry> {
        self.load()
            .into_iter()
            .rev()
            .find(|e| !e.undone && !e.outputs.is_empty())
    }

    pub fn mark_undone(&self, id: u64) -> std::io::Result<()> {
        let mut all = self.load();
        if let Some(e) = all.iter_mut().find(|e| e.id == id) {
            e.undone = true;
        }
        self.save(&all)
    }

    /// The chain of steps that led to job `id` (oldest first).
    pub fn chain(&self, id: u64) -> Vec<Step> {
        let all = self.load();
        let mut steps = Vec::new();
        let mut cur = all.iter().find(|e| e.id == id);
        while let Some(e) = cur {
            steps.push(Step {
                op: e.op.clone(),
                params: e.params.clone(),
            });
            cur = e.parent.and_then(|p| all.iter().find(|x| x.id == p));
        }
        steps.reverse();
        steps
    }
}

/// Paths that still exist among `paths` (undo only trashes what's there).
pub fn existing(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| Path::new(p).exists())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_links_and_undoes() {
        let d = tempfile::tempdir().unwrap();
        let h = History::at(d.path().join("h.json"));
        let a = h
            .record(
                "pdf.to-jpg",
                "JPG",
                &["r.pdf".into()],
                &["r.jpg".into()],
                &Params::new(),
            )
            .unwrap();
        let b = h
            .record(
                "image.crop",
                "Crop",
                &["r.jpg".into()],
                &["r Cropped.jpg".into()],
                &Params::new().with("ratio", "1:1"),
            )
            .unwrap();
        assert_eq!(b.parent, Some(a.id));
        let chain = h.chain(b.id);
        assert_eq!(
            chain.iter().map(|s| s.op.as_str()).collect::<Vec<_>>(),
            vec!["pdf.to-jpg", "image.crop"]
        );
        assert_eq!(h.last_undoable().unwrap().id, b.id);
        h.mark_undone(b.id).unwrap();
        assert_eq!(h.last_undoable().unwrap().id, a.id);
    }
}
