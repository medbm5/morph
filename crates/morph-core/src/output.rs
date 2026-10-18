//! Non-destructive, atomic output.
//!
//! Every result is written to a hidden temp file in the destination folder, then
//! committed under a fresh name. Commit never replaces an existing file: it
//! hard-links the temp file to the candidate name (which fails if the name is
//! taken) and moves on to `name 2`, `name 3`… The source is never opened for
//! writing and can never be the destination.

use crate::naming::Naming;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_name(dir: &Path, hint: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    dir.join(format!(".{hint}.morph-{pid}-{nanos:x}-{n}.tmp"))
}

/// Allocates output names and commits files atomically for one operation.
#[derive(Debug)]
pub struct OutputSink {
    naming: Naming,
    /// Tool word (`Cropped`); `None` for conversions.
    suffix: Option<String>,
    /// Write here instead of beside the source.
    out_dir: Option<PathBuf>,
    saved: Mutex<Vec<PathBuf>>,
}

impl OutputSink {
    pub fn new(naming: Naming, suffix: Option<String>, out_dir: Option<PathBuf>) -> Self {
        Self { naming, suffix, out_dir, saved: Mutex::new(Vec::new()) }
    }

    /// Paths committed so far.
    pub fn saved(&self) -> Vec<PathBuf> {
        self.saved.lock().unwrap().clone()
    }

    pub fn naming(&self) -> &Naming {
        &self.naming
    }

    /// Destination folder for outputs derived from `input`.
    pub fn dir_for(&self, input: &Path) -> PathBuf {
        if let Some(d) = &self.out_dir {
            return d.clone();
        }
        match input.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        }
    }

    /// A pending output file derived from `input` with extension `ext`
    /// (`photo.heic` → `photo.png`, or `photo Cropped.png` for a tool).
    pub fn file(&self, input: &Path, ext: &str) -> io::Result<PendingFile<'_>> {
        let stem = stem_of(input);
        self.file_in(&self.dir_for(input), &stem, ext)
    }

    /// A pending output file with an explicit folder and stem.
    pub fn file_in(&self, dir: &Path, stem: &str, ext: &str) -> io::Result<PendingFile<'_>> {
        let name = self.naming.render(stem, self.suffix.as_deref(), ext);
        self.file_named(dir, &name)
    }

    /// A pending output file with an exact name (collision numbering still applies).
    pub fn file_named(&self, dir: &Path, name: &str) -> io::Result<PendingFile<'_>> {
        fs::create_dir_all(dir)?;
        let tmp = temp_name(dir, "out");
        fs::File::create(&tmp)?;
        Ok(PendingFile { sink: self, tmp, dir: dir.to_path_buf(), name: name.to_string(), done: false })
    }

    /// A pending output folder derived from `input` (`report.pdf` → `report/`).
    pub fn folder(&self, input: &Path) -> io::Result<PendingDir<'_>> {
        let stem = stem_of(input);
        let name = self.naming.render(&stem, self.suffix.as_deref(), "");
        self.folder_named(&self.dir_for(input), &name)
    }

    pub fn folder_named(&self, dir: &Path, name: &str) -> io::Result<PendingDir<'_>> {
        fs::create_dir_all(dir)?;
        let tmp = temp_name(dir, "dir");
        fs::create_dir(&tmp)?;
        Ok(PendingDir { sink: self, tmp, dir: dir.to_path_buf(), name: name.to_string(), done: false })
    }

    fn record(&self, p: &Path) {
        self.saved.lock().unwrap().push(p.to_path_buf());
    }
}

/// File stem without the last extension; `Untitled` if there is none.
pub fn stem_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".into())
}

/// A temp file waiting to be committed. Dropped without commit → deleted.
pub struct PendingFile<'a> {
    sink: &'a OutputSink,
    tmp: PathBuf,
    dir: PathBuf,
    name: String,
    done: bool,
}

impl PendingFile<'_> {
    /// Where to write the content.
    pub fn path(&self) -> &Path {
        &self.tmp
    }

    /// The name this file will get if nothing collides.
    pub fn intended_name(&self) -> &str {
        &self.name
    }

    /// Give the file its final, collision-free name. Returns the final path.
    pub fn commit(mut self) -> io::Result<PathBuf> {
        let final_path = commit_file(&self.tmp, &self.dir, &self.name, self.sink.naming())?;
        self.done = true;
        self.sink.record(&final_path);
        Ok(final_path)
    }
}

impl Drop for PendingFile<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_file(&self.tmp);
        }
    }
}

/// A temp folder waiting to be committed. Dropped without commit → deleted.
pub struct PendingDir<'a> {
    sink: &'a OutputSink,
    tmp: PathBuf,
    dir: PathBuf,
    name: String,
    done: bool,
}

impl PendingDir<'_> {
    pub fn path(&self) -> &Path {
        &self.tmp
    }

    pub fn commit(mut self) -> io::Result<PathBuf> {
        let naming = self.sink.naming();
        for n in 1..10_000 {
            let cand = self.dir.join(naming.numbered(&self.name, n));
            if cand.symlink_metadata().is_ok() {
                continue;
            }
            match fs::rename(&self.tmp, &cand) {
                Ok(()) => {
                    self.done = true;
                    self.sink.record(&cand);
                    return Ok(cand);
                }
                Err(e) if cand.exists() => {
                    log::debug!("folder name taken during rename: {e}");
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("no free output folder name"))
    }
}

impl Drop for PendingDir<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_dir_all(&self.tmp);
        }
    }
}

/// Commit `tmp` as `dir/name`, or `name 2`, `name 3`… without ever replacing a file.
pub fn commit_file(tmp: &Path, dir: &Path, name: &str, naming: &Naming) -> io::Result<PathBuf> {
    for n in 1..10_000 {
        let cand = dir.join(naming.numbered(name, n));
        match fs::hard_link(tmp, &cand) {
            Ok(()) => {
                fs::remove_file(tmp)?;
                return Ok(cand);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                // File systems without hard links (FAT32, exFAT, some network shares):
                // fall back to check-then-rename.
                if cand.symlink_metadata().is_ok() {
                    continue;
                }
                log::debug!("hard link unavailable ({e}); falling back to rename");
                fs::rename(tmp, &cand)?;
                return Ok(cand);
            }
        }
    }
    Err(io::Error::other("no free output name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn never_overwrites_and_numbers_collisions() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("photo.heic");
        fs::write(&src, b"source").unwrap();
        fs::write(d.path().join("photo.png"), b"existing").unwrap();

        let sink = OutputSink::new(Naming::default(), None, None);
        let f = sink.file(&src, "png").unwrap();
        fs::File::create(f.path()).unwrap().write_all(b"new").unwrap();
        let out = f.commit().unwrap();

        assert_eq!(out.file_name().unwrap(), "photo 2.png");
        assert_eq!(fs::read(d.path().join("photo.png")).unwrap(), b"existing");
        assert_eq!(fs::read(&out).unwrap(), b"new");
        assert_eq!(fs::read(&src).unwrap(), b"source");
        assert_eq!(sink.saved(), vec![out]);
    }

    #[test]
    fn same_extension_never_hits_the_source() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("a.jpg");
        fs::write(&src, b"source").unwrap();
        let sink = OutputSink::new(Naming::default(), None, None);
        let f = sink.file(&src, "jpg").unwrap();
        fs::write(f.path(), b"x").unwrap();
        assert_eq!(f.commit().unwrap().file_name().unwrap(), "a 2.jpg");
        assert_eq!(fs::read(&src).unwrap(), b"source");
    }

    #[test]
    fn dropped_pending_leaves_nothing() {
        let d = tempfile::tempdir().unwrap();
        let sink = OutputSink::new(Naming::default(), Some("Cropped".into()), None);
        {
            let f = sink.file(&d.path().join("x.jpg"), "jpg").unwrap();
            fs::write(f.path(), b"half").unwrap();
            let _dir = sink.folder(&d.path().join("x.pdf")).unwrap();
        }
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[test]
    fn folders_commit_with_numbers() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("report")).unwrap();
        let sink = OutputSink::new(Naming::default(), None, None);
        let p = sink.folder(&d.path().join("report.pdf")).unwrap();
        fs::write(p.path().join("Page 001.png"), b"x").unwrap();
        let out = p.commit().unwrap();
        assert_eq!(out.file_name().unwrap(), "report 2");
        assert!(out.join("Page 001.png").exists());
    }
}
