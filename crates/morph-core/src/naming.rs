//! Output naming templates.
//!
//! Conversions keep the name and change the extension (`Page 001.pdf` →
//! `Page 001.jpg`); tools append the operation (`Page 001 Cropped.jpg`).
//! Collisions get a number (`Page 001 Cropped 2.jpg`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Naming {
    /// Template for conversions. Variables: `{stem}`, `{ext}`, `{op}`, `{date}`.
    pub convert: String,
    /// Template for tools.
    pub tool: String,
    /// How a collision number is attached: `{name} {n}` → `photo 2`.
    pub collision: String,
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            convert: "{stem}.{ext}".into(),
            tool: "{stem} {op}.{ext}".into(),
            collision: "{name} {n}".into(),
        }
    }
}

impl Naming {
    /// File name for an output, before collision numbering.
    ///
    /// `suffix` is the tool word (`Cropped`) or `None` for a conversion.
    /// An empty `ext` produces a folder-style name without an extension.
    pub fn render(&self, stem: &str, suffix: Option<&str>, ext: &str) -> String {
        let template = match suffix {
            Some(_) => &self.tool,
            None => &self.convert,
        };
        let date = today();
        let mut out = template
            .replace("{stem}", stem)
            .replace("{op}", suffix.unwrap_or(""))
            .replace("{date}", &date);
        if ext.is_empty() {
            out = out.replace(".{ext}", "").replace("{ext}", "");
        } else {
            out = out.replace("{ext}", ext);
        }
        sanitize(out.trim())
    }

    /// Apply collision number `n` (n ≥ 2) to a rendered name, keeping its extension.
    pub fn numbered(&self, name: &str, n: u32) -> String {
        if n <= 1 {
            return name.to_string();
        }
        let (base, ext) = split_ext(name);
        let numbered = self.collision.replace("{name}", base).replace("{n}", &n.to_string());
        match ext {
            Some(e) => format!("{numbered}.{e}"),
            None => numbered,
        }
    }
}

/// Split `a.b.jpg` into (`a.b`, Some(`jpg`)). Dotfiles keep their dot.
pub fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() && !name[i + 1..].contains(' ') => {
            (&name[..i], Some(&name[i + 1..]))
        }
        _ => (name, None),
    }
}

/// Replace characters that are invalid in file names on Windows or Linux.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed
    }
}

fn today() -> String {
    // Days since the epoch → civil date (Howard Hinnant's algorithm); avoids a chrono dependency.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_and_tool_names() {
        let n = Naming::default();
        assert_eq!(n.render("Page 001", None, "jpg"), "Page 001.jpg");
        assert_eq!(n.render("Page 001", Some("Cropped"), "jpg"), "Page 001 Cropped.jpg");
        assert_eq!(n.render("Page 001", Some("With BG"), "png"), "Page 001 With BG.png");
        assert_eq!(n.render("report", None, ""), "report");
    }

    #[test]
    fn collisions() {
        let n = Naming::default();
        assert_eq!(n.numbered("Page 001 Cropped.jpg", 2), "Page 001 Cropped 2.jpg");
        assert_eq!(n.numbered("photo.png", 1), "photo.png");
        assert_eq!(n.numbered("folder", 3), "folder 3");
        assert_eq!(n.numbered("archive.tar.gz", 2), "archive.tar 2.gz");
    }

    #[test]
    fn custom_template_and_sanitize() {
        let n = Naming { tool: "{op} - {stem}.{ext}".into(), ..Naming::default() };
        assert_eq!(n.render("a:b", Some("Edited"), "jpg"), "Edited - a_b.jpg");
        assert_eq!(today().len(), 10);
    }
}
