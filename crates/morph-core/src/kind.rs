//! File type detection: magic bytes first, extension as a fallback.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Broad family of a file, used to build rings and pick engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Image,
    Pdf,
    Video,
    Audio,
    Document,
    Slides,
    Spreadsheet,
    Subtitle,
    Archive,
    Folder,
    Other,
}

/// A detected file kind: MIME type, canonical extension and family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kind {
    pub mime: String,
    /// Canonical lower-case extension without the dot (`jpg`, `png`, `pdf`…).
    pub ext: String,
    pub family: Family,
}

impl Kind {
    pub fn new(mime: &str, ext: &str, family: Family) -> Self {
        Self {
            mime: mime.into(),
            ext: ext.into(),
            family,
        }
    }

    pub fn folder() -> Self {
        Self::new("inode/directory", "", Family::Folder)
    }

    /// Does this kind match a pattern such as `image/*`, `application/pdf`, `ext:srt`,
    /// `family:video`, `*` or `folder`?
    pub fn matches(&self, pattern: &str) -> bool {
        if pattern == "*" || pattern == "*/*" {
            return true;
        }
        if pattern == "folder" {
            return self.family == Family::Folder;
        }
        if let Some(ext) = pattern.strip_prefix("ext:") {
            return self.ext.eq_ignore_ascii_case(ext);
        }
        if let Some(fam) = pattern.strip_prefix("family:") {
            return serde_json::to_value(self.family)
                .ok()
                .and_then(|v| v.as_str().map(|s| s == fam))
                .unwrap_or(false);
        }
        if let Some(prefix) = pattern.strip_suffix("/*") {
            return self.mime.split('/').next() == Some(prefix);
        }
        self.mime.eq_ignore_ascii_case(pattern)
    }

    /// Short upper-case label for the HUD readout (`JPG`, `PDF`, `FOLDER`).
    pub fn label(&self) -> String {
        if self.family == Family::Folder {
            "FOLDER".into()
        } else if self.ext.is_empty() {
            "FILE".into()
        } else {
            self.ext.to_uppercase()
        }
    }
}

/// Canonical (mime, ext, family) for extensions we know, including the ones
/// magic bytes can't tell apart (text formats, OOXML containers).
fn by_extension(ext: &str) -> Option<Kind> {
    use Family::*;
    let (mime, canon, family) = match ext {
        "jpg" | "jpeg" | "jpe" | "jfif" => ("image/jpeg", "jpg", Image),
        "png" => ("image/png", "png", Image),
        "webp" => ("image/webp", "webp", Image),
        "tif" | "tiff" => ("image/tiff", "tiff", Image),
        "avif" => ("image/avif", "avif", Image),
        "bmp" => ("image/bmp", "bmp", Image),
        "gif" => ("image/gif", "gif", Image),
        "heic" | "heif" => ("image/heic", "heic", Image),
        "svg" => ("image/svg+xml", "svg", Image),
        "ico" => ("image/x-icon", "ico", Image),
        "pdf" => ("application/pdf", "pdf", Pdf),
        "mp4" | "m4v" => ("video/mp4", "mp4", Video),
        "mov" => ("video/quicktime", "mov", Video),
        "mkv" => ("video/x-matroska", "mkv", Video),
        "webm" => ("video/webm", "webm", Video),
        "avi" => ("video/x-msvideo", "avi", Video),
        "wmv" => ("video/x-ms-wmv", "wmv", Video),
        "mp3" => ("audio/mpeg", "mp3", Audio),
        "m4a" => ("audio/mp4", "m4a", Audio),
        "wav" => ("audio/wav", "wav", Audio),
        "flac" => ("audio/flac", "flac", Audio),
        "ogg" | "oga" => ("audio/ogg", "ogg", Audio),
        "opus" => ("audio/opus", "opus", Audio),
        "aif" | "aiff" => ("audio/aiff", "aiff", Audio),
        "wma" => ("audio/x-ms-wma", "wma", Audio),
        "docx" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
            Document,
        ),
        "doc" => ("application/msword", "doc", Document),
        "odt" => ("application/vnd.oasis.opendocument.text", "odt", Document),
        "rtf" => ("application/rtf", "rtf", Document),
        "md" | "markdown" => ("text/markdown", "md", Document),
        "txt" | "text" => ("text/plain", "txt", Document),
        "html" | "htm" => ("text/html", "html", Document),
        "epub" => ("application/epub+zip", "epub", Document),
        "pages" => ("application/vnd.apple.pages", "pages", Document),
        "pptx" => (
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
            Slides,
        ),
        "ppt" => ("application/vnd.ms-powerpoint", "ppt", Slides),
        "odp" => (
            "application/vnd.oasis.opendocument.presentation",
            "odp",
            Slides,
        ),
        "key" => ("application/vnd.apple.keynote", "key", Slides),
        "xlsx" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
            Spreadsheet,
        ),
        "xls" => ("application/vnd.ms-excel", "xls", Spreadsheet),
        "ods" => (
            "application/vnd.oasis.opendocument.spreadsheet",
            "ods",
            Spreadsheet,
        ),
        "csv" => ("text/csv", "csv", Spreadsheet),
        "tsv" => ("text/tab-separated-values", "tsv", Spreadsheet),
        "srt" => ("application/x-subrip", "srt", Subtitle),
        "vtt" => ("text/vtt", "vtt", Subtitle),
        "zip" => ("application/zip", "zip", Archive),
        "tar" => ("application/x-tar", "tar", Archive),
        "gz" | "tgz" => ("application/gzip", "gz", Archive),
        "7z" => ("application/x-7z-compressed", "7z", Archive),
        "rar" => ("application/vnd.rar", "rar", Archive),
        _ => return None,
    };
    Some(Kind::new(mime, canon, family))
}

/// Map a magic-byte MIME to our canonical kind.
fn by_mime(mime: &str) -> Option<Kind> {
    let ext = match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/tiff" => "tiff",
        "image/avif" => "avif",
        "image/bmp" => "bmp",
        "image/gif" => "gif",
        "image/heif" | "image/heic" => "heic",
        "image/vnd.microsoft.icon" | "image/x-icon" => "ico",
        "application/pdf" => "pdf",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        "video/webm" => "webm",
        "video/x-msvideo" => "avi",
        "video/x-ms-wmv" | "video/x-ms-asf" => "wmv",
        "audio/mpeg" => "mp3",
        "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/x-wav" | "audio/wav" => "wav",
        "audio/x-flac" | "audio/flac" => "flac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/x-aiff" | "audio/aiff" => "aiff",
        "application/rtf" => "rtf",
        "application/epub+zip" => "epub",
        "application/x-tar" => "tar",
        "application/gzip" => "gz",
        "application/x-7z-compressed" => "7z",
        "application/vnd.rar" | "application/x-rar-compressed" => "rar",
        "application/msword" => "doc",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.ms-powerpoint" => "ppt",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "application/vnd.oasis.opendocument.text" => "odt",
        "application/vnd.oasis.opendocument.spreadsheet" => "ods",
        "application/vnd.oasis.opendocument.presentation" => "odp",
        _ => return None,
    };
    by_extension(ext)
}

/// Detect a file's kind from its first bytes, falling back to the extension.
///
/// Containers that magic bytes can't distinguish (ZIP-based OOXML/EPUB/Pages, plain
/// text formats, MP4-family audio) are refined by the extension.
pub fn detect(path: &Path) -> std::io::Result<Kind> {
    if path.is_dir() {
        return Ok(Kind::folder());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let by_ext = by_extension(&ext);

    let mut head = [0u8; 8192];
    let n = {
        let mut f = File::open(path)?;
        let mut read = 0;
        while read < head.len() {
            match f.read(&mut head[read..])? {
                0 => break,
                k => read += k,
            }
        }
        read
    };
    let magic = infer::get(&head[..n]).and_then(|t| by_mime(t.mime_type()));

    Ok(match (magic, by_ext) {
        // ZIP containers: trust a more specific extension (docx, epub, pages, key…).
        (Some(m), Some(e)) if m.ext == "zip" && e.ext != "zip" => e,
        // MP4 family: .m4a is audio even though the container sniffs as video/mp4.
        (Some(m), Some(e)) if m.ext == "mp4" && e.family == Family::Audio => e,
        // HEIF brand sniffing may say heic for an .avif, and vice versa.
        (Some(m), Some(e)) if m.ext == "heic" && e.ext == "avif" => e,
        (Some(m), _) => m,
        (None, Some(e)) => e,
        (None, None) => {
            if looks_like_svg(&head[..n]) {
                by_extension("svg").unwrap()
            } else {
                let mime = mime_guess::from_ext(&ext).first_or_octet_stream();
                Kind::new(mime.essence_str(), &ext, Family::Other)
            }
        }
    })
}

fn looks_like_svg(head: &[u8]) -> bool {
    let s = String::from_utf8_lossy(head);
    let s = s.trim_start_matches('\u{feff}').trim_start();
    (s.starts_with("<?xml") || s.starts_with("<svg") || s.starts_with("<!--")) && s.contains("<svg")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        File::create(&p).unwrap().write_all(bytes).unwrap();
        p
    }

    #[test]
    fn magic_beats_wrong_extension() {
        let d = tempfile::tempdir().unwrap();
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        let k = detect(&write(d.path(), "really-a-png.jpg", png)).unwrap();
        assert_eq!(k.ext, "png");
        assert_eq!(k.family, Family::Image);
    }

    #[test]
    fn extension_fallback_for_text_formats() {
        let d = tempfile::tempdir().unwrap();
        let k = detect(&write(
            d.path(),
            "subs.srt",
            b"1\n00:00:01,000 --> 00:00:02,000\nHi\n",
        ))
        .unwrap();
        assert_eq!(k.family, Family::Subtitle);
        let k = detect(&write(d.path(), "notes.md", b"# Title\n")).unwrap();
        assert_eq!(k.ext, "md");
    }

    #[test]
    fn svg_without_extension() {
        let d = tempfile::tempdir().unwrap();
        let k = detect(&write(
            d.path(),
            "logo",
            b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        ))
        .unwrap();
        assert_eq!(k.ext, "svg");
    }

    #[test]
    fn patterns() {
        let jpg = by_extension("jpg").unwrap();
        assert!(jpg.matches("image/*"));
        assert!(jpg.matches("ext:JPG"));
        assert!(jpg.matches("family:image"));
        assert!(!jpg.matches("video/*"));
        assert!(Kind::folder().matches("folder"));
    }
}
