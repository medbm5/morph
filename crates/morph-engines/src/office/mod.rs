//! Documents, slides, spreadsheets and subtitles (S13).
//!
//! - pandoc (separate GPL process): DOCX ↔ Markdown ↔ TXT ↔ EPUB, HTML export, RTF input.
//! - LibreOffice (optional, the user's installation): slides and office files to PDF and
//!   images, Pages input. Segments that need it show INSTALL COMPONENT when it's missing.
//! - Pure Rust: spreadsheets (XLSX/ODS/CSV/TSV, XLS input), subtitles (SRT ↔ VTT, TXT),
//!   PDF → PPTX (page images), PDF → EPUB, PDF tables → XLSX (experimental).

pub mod epub;
pub mod pptx;
pub mod sheets;
pub mod subtitles;
pub mod tables;

use anyhow::{bail, Context, Result};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const DOCS: &[&str] = &[
    "ext:docx", "ext:odt", "ext:md", "ext:txt", "ext:html", "ext:epub", "ext:rtf",
];
pub const SLIDES: &[&str] = &["ext:pptx", "ext:ppt", "ext:odp", "ext:key"];
pub const OFFICE: &[&str] = &[
    "ext:docx",
    "ext:doc",
    "ext:odt",
    "ext:rtf",
    "ext:pages",
    "ext:pptx",
    "ext:ppt",
    "ext:odp",
    "ext:key",
    "ext:xlsx",
    "ext:xls",
    "ext:ods",
    "ext:csv",
    "ext:tsv",
];

pub fn register(reg: &mut Registry) {
    for (ext, label, order) in [
        ("docx", "DOCX", 1),
        ("md", "Markdown", 2),
        ("txt", "TXT", 3),
        ("epub", "EPUB", 4),
        ("html", "HTML", 5),
    ] {
        let mut spec = OpSpec::convert(ext, label, DOCS)
            .order(order)
            .component("pandoc");
        spec.id = format!("doc.to-{ext}");
        reg.register(Pandoc { spec, ext });
    }
    let mut pdf = OpSpec::convert("pdf", "PDF", OFFICE)
        .order(6)
        .component("libreoffice");
    pdf.id = "office.to-pdf".into();
    reg.register(Office {
        spec: pdf,
        kind: OfficeKind::Pdf,
    });
    for (ext, label, order) in [("jpg", "JPG", 7), ("png", "PNG", 8)] {
        let mut spec = OpSpec::convert(ext, label, SLIDES)
            .order(order)
            .component("libreoffice");
        spec.id = format!("slides.to-{ext}");
        reg.register(Office {
            spec,
            kind: OfficeKind::Images(ext),
        });
    }
    let mut pages = OpSpec::convert("docx", "DOCX", &["ext:pages"])
        .order(1)
        .component("libreoffice");
    pages.id = "pages.to-docx".into();
    reg.register(Office {
        spec: pages,
        kind: OfficeKind::Docx,
    });
    pptx::register(reg);
    epub::register(reg);
    tables::register(reg);
    sheets::register(reg);
    subtitles::register(reg);
}

// ---------------------------------------------------------------- pandoc

pub fn pandoc() -> Option<PathBuf> {
    morph_core::components::find_tool("pandoc", "pandoc")
}

fn quiet(c: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.stdin(Stdio::null())
}

fn pandoc_format(ext: &str) -> &'static str {
    match ext {
        "docx" => "docx",
        "odt" => "odt",
        "md" => "gfm",
        "txt" => "plain",
        "html" => "html",
        "epub" => "epub3",
        "rtf" => "rtf",
        _ => "markdown",
    }
}

fn reader_format(ext: &str) -> &'static str {
    match ext {
        "txt" => "markdown", // plain text reads cleanly as Markdown
        "epub" => "epub",
        other => pandoc_format(other),
    }
}

struct Pandoc {
    spec: OpSpec,
    ext: &'static str,
}

impl Operation for Pandoc {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        pandoc().is_some()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let exe = pandoc()
            .ok_or_else(|| anyhow::anyhow!(morph_core::Error::ComponentMissing("pandoc".into())))?;
        let input = ctx.input();
        let out = ctx.sink.file(input, self.ext)?;
        let mut c = Command::new(exe);
        quiet(&mut c)
            .arg("--from")
            .arg(reader_format(&ctx.kind().ext))
            .arg("--to")
            .arg(pandoc_format(self.ext))
            .arg("--sandbox") // never fetch or read anything but the input
            .arg("--standalone")
            .arg("--output")
            .arg(out.path())
            .arg(input);
        if self.ext == "html" {
            c.arg("--embed-resources");
        }
        if matches!(self.ext, "epub" | "html") {
            c.arg("--metadata")
                .arg(format!("title={}", morph_core::output::stem_of(input)));
        }
        let o = c.output().context("running pandoc")?;
        if !o.status.success() {
            bail!("pandoc: {}", String::from_utf8_lossy(&o.stderr).trim());
        }
        Ok(vec![out.commit()?])
    }
}

// ---------------------------------------------------------------- LibreOffice

pub fn soffice() -> Option<PathBuf> {
    morph_core::components::system_tool("libreoffice")
}

/// Convert with headless LibreOffice into `outdir`; returns the produced file.
pub fn libreoffice_convert(
    input: &Path,
    to: &str,
    outdir: &Path,
    progress: &morph_core::Progress,
) -> Result<PathBuf> {
    let exe = soffice().ok_or_else(|| {
        anyhow::anyhow!(morph_core::Error::ComponentMissing("libreoffice".into()))
    })?;
    // A private profile: no clash with a running LibreOffice, no first-run dialogs.
    // One per conversion: two LibreOffice instances can't share a profile, and the
    // app runs jobs in parallel.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let profile = std::env::temp_dir().join(format!(
        "morph-lo-profile-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(profile.clone());
    let profile_url = url_from_path(&profile);
    let mut c = Command::new(exe);
    quiet(&mut c)
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args([
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--nodefault",
            "--convert-to",
            to,
            "--outdir",
        ])
        .arg(outdir)
        .arg(input)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = c.spawn().context("starting LibreOffice")?;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut err);
                }
                bail!(
                    "LibreOffice failed ({status}){}",
                    if err.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", err.trim())
                    }
                );
            }
            break;
        }
        if progress.is_cancelled() {
            let _ = child.kill();
            bail!(morph_core::Error::Cancelled);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let want_ext = to.split(':').next().unwrap_or(to);
    let produced = outdir.join(format!("{}.{want_ext}", morph_core::output::stem_of(input)));
    if !produced.exists() {
        bail!("LibreOffice produced nothing (unsupported or damaged file?)");
    }
    Ok(produced)
}

fn url_from_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = if s.starts_with('/') {
        s
    } else {
        format!("/{s}")
    };
    format!("file://{}", s.replace(' ', "%20"))
}

fn scratch() -> Result<PathBuf> {
    Ok(morph_core::output::private_tempdir("morph-office")?)
}

enum OfficeKind {
    Pdf,
    Docx,
    Images(&'static str),
}

struct Office {
    spec: OpSpec,
    kind: OfficeKind,
}

impl Operation for Office {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        soffice().is_some()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        let input = ctx.input();
        let tmp = scratch()?;
        let result = (|| -> Result<Vec<PathBuf>> {
            match self.kind {
                OfficeKind::Pdf | OfficeKind::Docx => {
                    let (to, ext) = match self.kind {
                        OfficeKind::Pdf => ("pdf", "pdf"),
                        _ => ("docx:MS Word 2007 XML", "docx"),
                    };
                    let produced = libreoffice_convert(input, to, &tmp, ctx.progress)?;
                    let out = ctx.sink.file(input, ext)?;
                    std::fs::copy(&produced, out.path())?;
                    Ok(vec![out.commit()?])
                }
                OfficeKind::Images(ext) => {
                    // Slides → PDF → pages rendered with PDFium.
                    let pdf = libreoffice_convert(input, "pdf", &tmp, ctx.progress)?;
                    ctx.progress.set(0.5);
                    let pdfium = crate::pdf::lib::pdfium()?;
                    let doc = pdfium.load_pdf_from_file(&pdf, None)?;
                    use pdfium_render::prelude::*;
                    let cfg = PdfRenderConfig::new().set_target_width(1920);
                    let render = |p: &PdfPage| -> Result<image::DynamicImage> {
                        Ok(image::DynamicImage::ImageRgb8(
                            p.render_with_config(&cfg)?.as_image()?.to_rgb8(),
                        ))
                    };
                    let opts = crate::image::encode::EncodeOptions {
                        quality: 92,
                        ..Default::default()
                    };
                    let pages: Vec<_> = doc.pages().iter().collect();
                    if pages.len() == 1 {
                        let out = ctx.sink.file(input, ext)?;
                        crate::image::encode::encode(&render(&pages[0])?, ext, out.path(), &opts)?;
                        return Ok(vec![out.commit()?]);
                    }
                    let dir = ctx.sink.folder(input)?;
                    for (i, p) in pages.iter().enumerate() {
                        crate::image::encode::encode(
                            &render(p)?,
                            ext,
                            &dir.path().join(format!("Slide {:03}.{ext}", i + 1)),
                            &opts,
                        )?;
                        ctx.progress
                            .set(0.5 + 0.5 * (i + 1) as f32 / pages.len() as f32);
                    }
                    Ok(vec![dir.commit()?])
                }
            }
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        result
    }
}
