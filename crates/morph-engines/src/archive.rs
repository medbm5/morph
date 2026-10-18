//! Archives: create ZIP / TAR / TAR.GZ (GZIP for a single file) from a selection,
//! extract ZIP, TAR, GZIP and 7z in Rust, and RAR through libarchive's `bsdtar`
//! (built into Windows 10+ as `tar.exe`; `libarchive-tools` on Linux).
//!
//! Extraction never writes outside the new folder: entries with absolute paths,
//! `..` components or drive prefixes are rejected, and so are links that point out.

use anyhow::{bail, Context, Result};
use morph_core::{Arity, OpContext, OpSpec, Operation, OutputKind, Registry};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const ANY: &[&str] = &["*", "folder"];
const ARCHIVES: &[&str] = &["ext:zip", "ext:tar", "ext:gz", "ext:7z", "ext:rar"];

pub fn register(reg: &mut Registry) {
    for (id, label, order) in [
        ("archive.zip", "ZIP", 30),
        ("archive.tar", "TAR", 31),
        ("archive.targz", "TAR.GZ", 32),
    ] {
        let mut spec = OpSpec::tool(id, label, "", ANY, OutputKind::Ext("zip".into()))
            .arity(Arity::All)
            .order(order);
        spec.suffix = None;
        reg.register(Create { spec });
    }
    reg.register(Extract {
        spec: OpSpec::tool(
            "archive.extract",
            "Extract",
            "",
            ARCHIVES,
            OutputKind::Folder,
        )
        .order(1),
    });
}

/// Files under `root` (recursively for folders) with their archive names.
fn collect(inputs: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    for input in inputs {
        let base = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if input.is_dir() {
            let mut stack = vec![(input.clone(), base.clone())];
            while let Some((dir, prefix)) = stack.pop() {
                let mut entries: Vec<_> = fs::read_dir(&dir)?.flatten().collect();
                entries.sort_by_key(|e| e.file_name());
                for e in entries {
                    let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
                    let ft = e.file_type()?;
                    if ft.is_dir() {
                        stack.push((e.path(), name));
                    } else if ft.is_file() {
                        out.push((e.path(), name));
                    }
                }
            }
        } else {
            out.push((input.clone(), base));
        }
    }
    Ok(out)
}

struct Create {
    spec: OpSpec,
}

impl Operation for Create {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let files = collect(&ctx.inputs)?;
        let single = ctx.inputs.len() == 1;
        let stem = if single {
            morph_core::output::stem_of(ctx.input())
        } else {
            "Archive".into()
        };
        let dir = ctx.sink.dir_for(ctx.input());
        let kind = self.spec.id.as_str();
        // A single plain file to GZIP keeps its name: photo.png → photo.png.gz.
        let single_file_gz = kind == "archive.targz" && single && ctx.input().is_file();
        let (ext, stem) = match kind {
            "archive.zip" => ("zip", stem),
            "archive.tar" => ("tar", stem),
            _ if single_file_gz => (
                "gz",
                ctx.input()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            _ => ("tar.gz", stem),
        };
        let out = ctx.sink.file_named(&dir, &format!("{stem}.{ext}"))?;
        let total = files.len().max(1);
        let f = fs::File::create(out.path())?;
        match ext {
            "zip" => {
                let mut z = zip::ZipWriter::new(f);
                let opts = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .large_file(true);
                for (i, (path, name)) in files.iter().enumerate() {
                    ctx.progress.check()?;
                    z.start_file(name, opts)?;
                    std::io::copy(&mut fs::File::open(path)?, &mut z)?;
                    ctx.progress.set((i + 1) as f32 / total as f32);
                }
                z.finish()?;
            }
            "gz" => {
                let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
                std::io::copy(&mut fs::File::open(ctx.input())?, &mut gz)?;
                gz.finish()?;
            }
            _ => {
                let w: Box<dyn Write> = if ext == "tar.gz" {
                    Box::new(flate2::write::GzEncoder::new(
                        f,
                        flate2::Compression::default(),
                    ))
                } else {
                    Box::new(f)
                };
                let mut t = tar::Builder::new(w);
                for (i, (path, name)) in files.iter().enumerate() {
                    ctx.progress.check()?;
                    t.append_path_with_name(path, name)?;
                    ctx.progress.set((i + 1) as f32 / total as f32);
                }
                t.into_inner()?.flush()?;
            }
        }
        Ok(vec![out.commit()?])
    }
}

/// A safe relative path for an archive entry, or an error for traversal attempts.
pub fn safe_rel(name: &str) -> Result<PathBuf> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.contains(':') {
        bail!("refusing absolute path `{name}` in archive");
    }
    let mut out = PathBuf::new();
    for c in Path::new(&name).components() {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => bail!("refusing path `{name}` that escapes the folder"),
        }
    }
    if out.as_os_str().is_empty() {
        bail!("empty path in archive");
    }
    Ok(out)
}

fn write_entry(root: &Path, name: &str, mut r: impl Read) -> Result<()> {
    let rel = safe_rel(name)?;
    let dest = root.join(rel);
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p)?;
    }
    let mut f = fs::File::create(&dest)?;
    std::io::copy(&mut r, &mut f)?;
    Ok(())
}

struct Extract {
    spec: OpSpec,
}

impl Operation for Extract {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let name = input
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        // `report.tar.gz` → folder `report`.
        let stem = name
            .trim_end_matches(".gz")
            .trim_end_matches(".tgz")
            .trim_end_matches(".tar")
            .trim_end_matches(".zip")
            .trim_end_matches(".7z")
            .trim_end_matches(".rar")
            .to_string();
        let stem = input.file_name().unwrap().to_string_lossy()[..stem.len()].to_string();
        let dir = ctx.sink.folder_named(&ctx.sink.dir_for(input), &stem)?;
        let root = dir.path().to_path_buf();
        let ext = ctx.kind().ext.as_str();
        match ext {
            "zip" => {
                let mut z = zip::ZipArchive::new(fs::File::open(input)?)?;
                let n = z.len();
                for i in 0..n {
                    ctx.progress.check()?;
                    let e = z.by_index(i)?;
                    if e.is_dir() {
                        fs::create_dir_all(root.join(safe_rel(e.name())?))?;
                    } else if e.is_symlink() {
                        bail!("refusing symbolic link `{}` in archive", e.name());
                    } else {
                        let entry_name = e.name().to_string();
                        write_entry(&root, &entry_name, e)?;
                    }
                    ctx.progress.set((i + 1) as f32 / n.max(1) as f32);
                }
            }
            "tar" | "gz" => {
                let f = fs::File::open(input)?;
                let is_tar_gz = name.ends_with(".tar.gz")
                    || name.ends_with(".tgz")
                    || ext == "tar"
                    || gz_is_tar(input);
                if ext == "gz" && !is_tar_gz {
                    // Plain .gz of one file.
                    let inner = input.file_stem().unwrap().to_string_lossy().into_owned();
                    write_entry(&root, &inner, flate2::read::GzDecoder::new(f))?;
                } else {
                    let r: Box<dyn Read> = if ext == "gz" {
                        Box::new(flate2::read::GzDecoder::new(f))
                    } else {
                        Box::new(f)
                    };
                    let mut t = tar::Archive::new(r);
                    for e in t.entries()? {
                        ctx.progress.check()?;
                        let e = e?;
                        let path = e.path()?.to_string_lossy().into_owned();
                        match e.header().entry_type() {
                            tar::EntryType::Directory => {
                                fs::create_dir_all(root.join(safe_rel(&path)?))?;
                            }
                            tar::EntryType::Regular | tar::EntryType::Continuous => {
                                write_entry(&root, &path, e)?
                            }
                            tar::EntryType::Symlink | tar::EntryType::Link => {
                                bail!("refusing link `{path}` in archive")
                            }
                            _ => {} // devices, fifos: skip
                        }
                    }
                }
            }
            "7z" => {
                sevenz_rust2::decompress_with_extract_fn(
                    fs::File::open(input)?,
                    &root,
                    |entry, reader, _dest| {
                        if entry.name().trim_matches(['/', '\\', '.']).is_empty() {
                            // The archive's own root entry.
                        } else if entry.is_directory() {
                            let rel = safe_rel(entry.name()).map_err(|e| {
                                sevenz_rust2::Error::from(std::io::Error::other(e.to_string()))
                            })?;
                            fs::create_dir_all(root.join(rel))
                                .map_err(sevenz_rust2::Error::from)?;
                        } else {
                            write_entry(&root, entry.name(), reader).map_err(|e| {
                                sevenz_rust2::Error::from(std::io::Error::other(e.to_string()))
                            })?;
                        }
                        Ok(true)
                    },
                )
                .context("extracting 7z")?;
            }
            "rar" => bsdtar_extract(input, &root)?,
            other => bail!("can't extract .{other}"),
        }
        verify_inside(&root)?;
        Ok(vec![dir.commit()?])
    }
}

fn gz_is_tar(p: &Path) -> bool {
    let Ok(f) = fs::File::open(p) else {
        return false;
    };
    let mut buf = [0u8; 512];
    let mut r = flate2::read::GzDecoder::new(f);
    r.read_exact(&mut buf).is_ok() && &buf[257..262] == b"ustar"
}

/// libarchive's bsdtar: `tar.exe` on Windows 10+, `bsdtar` on Linux.
pub fn bsdtar() -> Option<PathBuf> {
    if cfg!(windows) {
        let sys = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let p = sys.join("System32").join("tar.exe");
        return p.is_file().then_some(p);
    }
    morph_core::components::find_on_path("bsdtar")
}

fn bsdtar_extract(input: &Path, root: &Path) -> Result<()> {
    let exe = bsdtar().context("RAR extraction needs bsdtar (install libarchive-tools)")?;
    // bsdtar refuses absolute and `..` paths unless -P is given; we never pass -P.
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = c
        .arg("-xf")
        .arg(input)
        .arg("-C")
        .arg(root)
        .stdin(Stdio::null())
        .output()
        .context("running bsdtar")?;
    if !out.status.success() {
        bail!("bsdtar: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Belt and braces: nothing extracted may resolve outside `root`.
fn verify_inside(root: &Path) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d)?.flatten() {
            let p = e.path();
            let meta = fs::symlink_metadata(&p)?;
            if meta.file_type().is_symlink() {
                let target = fs::canonicalize(&p).unwrap_or_default();
                if !target.starts_with(&root) {
                    let _ = fs::remove_file(&p);
                    bail!("removed link `{}` pointing outside the folder", p.display());
                }
            } else if meta.is_dir() {
                stack.push(p);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_rejected() {
        assert!(safe_rel("../evil.txt").is_err());
        assert!(safe_rel("a/../../evil.txt").is_err());
        assert!(safe_rel("/etc/passwd").is_err());
        assert!(safe_rel("C:/Windows/evil.dll").is_err());
        assert!(safe_rel("..\\evil.txt").is_err());
        assert_eq!(
            safe_rel("./a/b.txt").unwrap(),
            PathBuf::from("a").join("b.txt")
        );
    }
}
