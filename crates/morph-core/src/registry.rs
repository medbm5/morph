//! Operation registry, filtered by the dragged selection.

use crate::kind::{Family, Kind};
use crate::op::{Arity, OpSpec, Operation, Ring};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct Registry {
    ops: Vec<Arc<dyn Operation>>,
    index: HashMap<String, usize>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an operation. A later registration with the same id replaces the earlier one
    /// (user recipes can override built-ins).
    pub fn register(&mut self, op: impl Operation + 'static) {
        self.register_arc(Arc::new(op));
    }

    pub fn register_arc(&mut self, op: Arc<dyn Operation>) {
        let id = op.spec().id.clone();
        if let Some(&i) = self.index.get(&id) {
            self.ops[i] = op;
        } else {
            self.index.insert(id, self.ops.len());
            self.ops.push(op);
        }
    }

    /// Remove every operation whose id starts with `prefix` (used to reload recipes).
    pub fn remove_prefix(&mut self, prefix: &str) {
        self.ops.retain(|o| !o.spec().id.starts_with(prefix));
        self.index = self
            .ops
            .iter()
            .enumerate()
            .map(|(i, o)| (o.spec().id.clone(), i))
            .collect();
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Operation>> {
        self.index.get(id).map(|&i| &self.ops[i])
    }

    pub fn all(&self) -> impl Iterator<Item = &Arc<dyn Operation>> {
        self.ops.iter()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Operations of `ring` valid for the whole selection, in ring order.
    ///
    /// A per-file op must accept every file; a whole-selection op must accept every
    /// file too. Conversions to the selection's own format are left out
    /// (no `JPG` segment when dragging only JPGs).
    pub fn for_selection(&self, kinds: &[Kind], ring: Ring) -> Vec<&Arc<dyn Operation>> {
        if kinds.is_empty() {
            return vec![];
        }
        let mut out: Vec<_> = self
            .ops
            .iter()
            .filter(|op| {
                let s = op.spec();
                s.ring == ring
                    && kinds.iter().all(|k| s.accepts_kind(k))
                    && !(s.ring == Ring::Convert && is_identity(s, kinds))
                    && !(s.arity == Arity::All && needs_many(s) && kinds.len() < 2)
            })
            .collect();
        out.sort_by(|a, b| {
            let (a, b) = (a.spec(), b.spec());
            a.order.cmp(&b.order).then_with(|| a.label.cmp(&b.label))
        });
        out
    }

    /// The conversion to `ext` for this selection (`morph convert --to ext`).
    pub fn converter(&self, ext: &str, kinds: &[Kind]) -> Option<&Arc<dyn Operation>> {
        self.ops.iter().find(|op| {
            let s = op.spec();
            s.ring == Ring::Convert
                && matches!(&s.output, crate::op::OutputKind::Ext(e) if e == ext)
                && kinds.iter().all(|k| s.accepts_kind(k))
        })
    }

    /// Specs for a selection, for the UI and `morph list --for`.
    pub fn specs_for(&self, kinds: &[Kind], ring: Ring) -> Vec<OpSpec> {
        self.for_selection(kinds, ring)
            .into_iter()
            .map(|o| o.spec().clone())
            .collect()
    }
}

fn is_identity(spec: &OpSpec, kinds: &[Kind]) -> bool {
    match &spec.output {
        crate::op::OutputKind::Ext(ext) => kinds
            .iter()
            .all(|k| &k.ext == ext && k.family != Family::Folder),
        _ => false,
    }
}

/// Whole-selection ops that only make sense with two or more files.
fn needs_many(spec: &OpSpec) -> bool {
    spec.id.ends_with(".merge") || spec.id.ends_with(".join")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{OpContext, OutputKind};

    struct Fake(OpSpec);
    impl Operation for Fake {
        fn spec(&self) -> &OpSpec {
            &self.0
        }
        fn run(&self, _: &OpContext) -> anyhow::Result<Vec<std::path::PathBuf>> {
            Ok(vec![])
        }
    }

    fn kind(ext: &str, mime: &str, fam: Family) -> Kind {
        Kind::new(mime, ext, fam)
    }

    #[test]
    fn filters_by_type_ring_and_identity() {
        let mut r = Registry::new();
        r.register(Fake(OpSpec::convert("png", "PNG", &["image/*"]).order(2)));
        r.register(Fake(OpSpec::convert("jpg", "JPG", &["image/*"]).order(1)));
        r.register(Fake(OpSpec::convert("mp3", "MP3", &["video/*", "audio/*"])));
        r.register(Fake(OpSpec::tool(
            "image.crop",
            "Crop",
            "Cropped",
            &["image/*"],
            OutputKind::SameAsInput,
        )));
        r.register(Fake(
            OpSpec::tool(
                "pdf.merge",
                "Merge",
                "Merged",
                &["application/pdf"],
                OutputKind::Ext("pdf".into()),
            )
            .arity(Arity::All),
        ));

        let jpg = kind("jpg", "image/jpeg", Family::Image);
        let ids: Vec<_> = r
            .for_selection(std::slice::from_ref(&jpg), Ring::Convert)
            .iter()
            .map(|o| o.spec().id.clone())
            .collect();
        assert_eq!(ids, vec!["convert.png"]);
        assert_eq!(r.for_selection(&[jpg], Ring::Tools).len(), 1);

        let pdf = kind("pdf", "application/pdf", Family::Pdf);
        assert!(r
            .for_selection(std::slice::from_ref(&pdf), Ring::Tools)
            .is_empty());
        assert_eq!(r.for_selection(&[pdf.clone(), pdf], Ring::Tools).len(), 1);
    }

    #[test]
    fn mixed_selection_needs_common_support() {
        let mut r = Registry::new();
        r.register(Fake(OpSpec::convert("png", "PNG", &["image/*"])));
        let sel = [
            kind("jpg", "image/jpeg", Family::Image),
            kind("pdf", "application/pdf", Family::Pdf),
        ];
        assert!(r.for_selection(&sel, Ring::Convert).is_empty());
    }

    #[test]
    fn later_registration_replaces() {
        let mut r = Registry::new();
        r.register(Fake(OpSpec::convert("png", "PNG", &["image/*"])));
        r.register(Fake(OpSpec::convert("png", "PNG!", &["image/*"])));
        assert_eq!(r.len(), 1);
        assert_eq!(r.get("convert.png").unwrap().spec().label, "PNG!");
        r.remove_prefix("convert.");
        assert!(r.is_empty());
    }
}
