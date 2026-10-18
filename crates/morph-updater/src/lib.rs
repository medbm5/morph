//! Optional components and app updates — the only Metamorphosis code that may use
//! the network (enforced by `scripts/offline_guard.py`).
//!
//! Installing a component downloads each pinned asset, checks its size and SHA-256,
//! unpacks only the listed files into `components/<name>/` (never outside it) and
//! finally writes the `.installed` marker that engines look for.

pub mod catalog;

use anyhow::{bail, Context, Result};
use catalog::{Asset, Component, Unpack};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub use catalog::{find, CATALOG};

/// Progress callback: (bytes done, bytes total, current file).
pub type ProgressFn<'a> = &'a dyn Fn(u64, u64, &str);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub name: String,
    pub title: String,
    pub description: String,
    pub license: String,
    pub installed: bool,
    pub download_size: u64,
    pub requires: Vec<String>,
    pub system_hint: Option<String>,
}

pub fn status() -> Vec<Status> {
    CATALOG
        .iter()
        .map(|c| Status {
            name: c.name.into(),
            title: c.title.into(),
            description: c.description.into(),
            license: c.license.into(),
            installed: morph_core::components::is_installed(c.name),
            download_size: c.download_size(),
            requires: c.requires.iter().map(|s| s.to_string()).collect(),
            system_hint: c.system_hint.map(str::to_string),
        })
        .collect()
}

/// Install `name` (and anything it requires).
pub fn install(name: &str, progress: ProgressFn) -> Result<()> {
    let c = find(name).with_context(|| format!("unknown component `{name}`"))?;
    for dep in c.requires {
        if !morph_core::components::is_installed(dep) {
            install(dep, progress)?;
        }
    }
    if c.assets().is_empty() {
        bail!(
            "{}",
            c.system_hint.unwrap_or("no download for this platform")
        );
    }
    let dir = morph_core::components::component_dir(c.name);
    let staging = dir.with_extension("partial");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let total = c.download_size();
    let mut done = 0u64;
    for asset in c.assets() {
        let data = download(asset, &|n| progress(done + n, total, asset.url))?;
        done += asset.size;
        unpack(asset, &data, &staging)?;
    }
    fs::write(
        staging.join(".installed"),
        serde_json::to_vec_pretty(&marker(c))?,
    )?;
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&staging, &dir).context("activating component")?;
    progress(total, total, "done");
    Ok(())
}

fn marker(c: &Component) -> serde_json::Value {
    serde_json::json!({
        "name": c.name,
        "assets": c.assets().iter().map(|a| serde_json::json!({ "url": a.url, "sha256": a.sha256 })).collect::<Vec<_>>(),
        "installed_by": format!("Metamorphosis {}", morph_core::VERSION),
    })
}

pub fn remove(name: &str) -> Result<()> {
    let dir = morph_core::components::component_dir(name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Download into memory, verifying size and SHA-256.
fn download(asset: &Asset, progress: &dyn Fn(u64)) -> Result<Vec<u8>> {
    log::info!("downloading {}", asset.url);
    let resp = ureq::get(asset.url)
        .header(
            "User-Agent",
            &format!("Metamorphosis/{}", morph_core::VERSION),
        )
        .call()
        .with_context(|| format!("downloading {}", asset.url))?;
    let mut reader = resp.into_body().into_reader();
    let mut data = Vec::with_capacity(asset.size as usize);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        progress(data.len() as u64);
        if data.len() as u64 > asset.size + 1024 {
            bail!("{} is larger than expected", asset.url);
        }
    }
    verify(asset, &data)?;
    Ok(data)
}

pub fn verify(asset: &Asset, data: &[u8]) -> Result<()> {
    if data.len() as u64 != asset.size {
        bail!(
            "{}: size {} ≠ expected {}",
            asset.url,
            data.len(),
            asset.size
        );
    }
    let got = hex::encode(Sha256::digest(data));
    if got != asset.sha256 {
        bail!(
            "{}: checksum mismatch (got {got}, want {})",
            asset.url,
            asset.sha256
        );
    }
    Ok(())
}

/// Write `data` to `dir/name`, refusing names that escape `dir`.
fn write_inside(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf> {
    let rel = Path::new(name);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        bail!("refusing to write `{name}` outside the component folder");
    }
    let out = dir.join(rel);
    if let Some(p) = out.parent() {
        fs::create_dir_all(p)?;
    }
    fs::File::create(&out)?.write_all(data)?;
    Ok(out)
}

fn unpack(asset: &Asset, data: &[u8], dir: &Path) -> Result<()> {
    match asset.unpack {
        Unpack::File => {
            write_inside(dir, asset.files[0].1, data)?;
        }
        Unpack::Zip => {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))?;
            for (from, to) in asset.files {
                let mut f = zip
                    .by_name(from)
                    .with_context(|| format!("{from} missing from archive"))?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)?;
                write_inside(dir, to, &buf)?;
            }
        }
        Unpack::TarGz | Unpack::TarXz => {
            let reader: Box<dyn Read> = if asset.unpack == Unpack::TarGz {
                Box::new(flate2::read::GzDecoder::new(data))
            } else {
                Box::new(xz2::read::XzDecoder::new(data))
            };
            let mut ar = tar::Archive::new(reader);
            let mut wanted: Vec<_> = asset.files.to_vec();
            for entry in ar.entries()? {
                let mut e = entry?;
                let path = e.path()?.to_string_lossy().replace('\\', "/");
                if let Some(i) = wanted.iter().position(|(from, _)| *from == path) {
                    let mut buf = Vec::new();
                    e.read_to_end(&mut buf)?;
                    let out = write_inside(dir, wanted[i].1, &buf)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(mode) = e.header().mode() {
                            fs::set_permissions(&out, fs::Permissions::from_mode(mode | 0o600))?;
                        }
                    }
                    let _ = out;
                    wanted.remove(i);
                }
            }
            if let Some((missing, _)) = wanted.first() {
                bail!("{missing} missing from archive");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_path_traversal() {
        let d = tempfile::tempdir().unwrap();
        assert!(write_inside(d.path(), "../evil.dll", b"x").is_err());
        assert!(write_inside(d.path(), "/abs", b"x").is_err());
        assert!(write_inside(d.path(), "ok/file.bin", b"x").is_ok());
    }

    #[test]
    fn checksum_and_size_are_enforced() {
        let a = Asset {
            url: "x",
            sha256: "00",
            size: 3,
            unpack: Unpack::File,
            files: &[("", "f")],
        };
        assert!(verify(&a, b"abcd").is_err());
        assert!(verify(&a, b"abc").is_err());
        let good = Asset {
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ..a
        };
        assert!(verify(&good, b"abc").is_ok());
    }

    #[test]
    fn catalog_is_complete() {
        for c in CATALOG {
            for a in c.windows.iter().chain(c.linux) {
                assert_eq!(a.sha256.len(), 64, "{}", a.url);
                assert!(a.url.starts_with("https://"));
                assert!(!a.files.is_empty());
            }
            for r in c.requires {
                assert!(find(r).is_some(), "{} requires unknown {r}", c.name);
            }
        }
    }
}
