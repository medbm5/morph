//! S14: archive round trips, and extraction rejects path-traversal entries.

use morph_core::{Job, Runner};
use std::io::Write;
use std::path::{Path, PathBuf};

fn tree(dir: &Path) -> Vec<PathBuf> {
    let a = dir.join("photo.txt");
    std::fs::write(&a, b"pixels").unwrap();
    let folder = dir.join("notes");
    std::fs::create_dir_all(folder.join("deep")).unwrap();
    std::fs::write(folder.join("a.md"), b"# A").unwrap();
    std::fs::write(folder.join("deep").join("b.md"), b"# B").unwrap();
    vec![a, folder]
}

fn run(op: &str, inputs: Vec<PathBuf>) -> Result<PathBuf, String> {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(&Job::new(op, inputs), |_| {})
        .map_err(|e| e.to_string())?;
    if let Some((_, e)) = r.errors.first() {
        return Err(e.clone());
    }
    Ok(r.outputs[0].clone())
}

fn check_tree(root: &Path) {
    assert_eq!(std::fs::read(root.join("photo.txt")).unwrap(), b"pixels");
    assert_eq!(std::fs::read(root.join("notes/a.md")).unwrap(), b"# A");
    assert_eq!(std::fs::read(root.join("notes/deep/b.md")).unwrap(), b"# B");
}

#[test]
fn round_trips() {
    let d = tempfile::tempdir().unwrap();
    let inputs = tree(d.path());
    for op in ["archive.zip", "archive.tar", "archive.targz"] {
        let ar = run(op, inputs.clone()).unwrap();
        assert!(
            ar.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Archive."),
            "{}",
            ar.display()
        );
        let out = run("archive.extract", vec![ar.clone()]).unwrap();
        assert!(out.is_dir());
        check_tree(&out);
        std::fs::remove_dir_all(out).unwrap();
        std::fs::remove_file(ar).unwrap();
    }
    // One file to GZIP keeps its name.
    let gz = run("archive.targz", vec![inputs[0].clone()]).unwrap();
    assert_eq!(gz.file_name().unwrap(), "photo.txt.gz");
    let out = run("archive.extract", vec![gz]).unwrap();
    assert_eq!(std::fs::read(out.join("photo.txt")).unwrap(), b"pixels");
}

#[test]
fn extracts_7z() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("src");
    std::fs::create_dir_all(src.join("deep")).unwrap();
    std::fs::write(src.join("a.txt"), b"seven").unwrap();
    std::fs::write(src.join("deep/b.txt"), b"zip").unwrap();
    let ar = d.path().join("bundle.7z");
    sevenz_rust2::compress_to_path(&src, &ar).unwrap();
    let out = run("archive.extract", vec![ar]).unwrap();
    assert_eq!(out.file_name().unwrap(), "bundle");
    let a = walk(&out)
        .into_iter()
        .find(|p| p.ends_with("a.txt"))
        .unwrap();
    assert_eq!(std::fs::read(a).unwrap(), b"seven");
}

fn walk(d: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(d).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}

#[test]
fn zip_slip_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let inner = d.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let ar = inner.join("evil.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&ar).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    z.start_file("ok.txt", o).unwrap();
    z.write_all(b"fine").unwrap();
    z.start_file("../escaped.txt", o).unwrap();
    z.write_all(b"pwned").unwrap();
    z.finish().unwrap();
    let err = run("archive.extract", vec![ar]).unwrap_err();
    assert!(err.contains("escapes"), "{err}");
    assert!(
        !d.path().join("escaped.txt").exists(),
        "nothing written outside"
    );
    // The half-extracted folder is discarded, not left behind.
    assert_eq!(std::fs::read_dir(&inner).unwrap().count(), 1);
}

#[test]
fn tar_traversal_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let inner = d.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let ar = inner.join("evil.tar");
    let mut b = tar::Builder::new(std::fs::File::create(&ar).unwrap());
    let mut h = tar::Header::new_old();
    let name = b"../../escaped.txt";
    h.as_old_mut().name[..name.len()].copy_from_slice(name);
    h.set_size(5);
    h.set_mode(0o644);
    h.set_cksum();
    b.append(&h, &b"pwned"[..]).unwrap();
    b.finish().unwrap();
    drop(b);
    let err = run("archive.extract", vec![ar]).unwrap_err();
    assert!(err.contains("escapes"), "{err}");
    assert!(!d.path().join("escaped.txt").exists());
}
