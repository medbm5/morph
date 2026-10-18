//! Reconstruct reading order from PDF characters: lines → blocks (headings,
//! paragraphs, list items) and images, for TXT/Markdown/DOCX output.

use pdfium_render::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    ListItem {
        ordered: bool,
        text: String,
    },
    /// PNG bytes plus size in points.
    Image {
        png: Vec<u8>,
        width_pt: f32,
        height_pt: f32,
    },
    PageBreak,
}

#[derive(Debug, Clone)]
struct Line {
    text: String,
    size: f32,
    bold: f32,
    left: f32,
    top: f32,
    bottom: f32,
}

/// Text lines of one page, top to bottom.
fn lines(page: &PdfPage) -> Vec<Line> {
    let Ok(text) = page.text() else { return vec![] };
    let mut lines: Vec<Line> = Vec::new();
    let mut cur: Option<(Line, f32, usize, usize)> = None; // (line, last right edge, chars, bold chars)
    let mut last_y = f32::NAN;

    let flush = |cur: &mut Option<(Line, f32, usize, usize)>, lines: &mut Vec<Line>| {
        if let Some((mut l, _, n, b)) = cur.take() {
            l.text = l.text.trim().to_string();
            l.bold = if n > 0 { b as f32 / n as f32 } else { 0.0 };
            if !l.text.is_empty() {
                lines.push(l);
            }
        }
    };

    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        if c == '\r' || c == '\n' {
            continue; // we rebuild lines from geometry
        }
        let Ok(b) = ch.loose_bounds() else { continue };
        let (left, right, top, bottom) = (
            b.left().value,
            b.right().value,
            b.top().value,
            b.bottom().value,
        );
        let size = ch.scaled_font_size().value;
        let y = bottom;
        let new_line = match &cur {
            None => true,
            Some((l, last_right, _, _)) => {
                let h = (l.top - l.bottom).max(size).max(1.0);
                (y - last_y).abs() > h * 0.5 || left + h * 4.0 < *last_right
            }
        };
        if new_line {
            flush(&mut cur, &mut lines);
            cur = Some((
                Line {
                    text: String::new(),
                    size,
                    bold: 0.0,
                    left,
                    top,
                    bottom,
                },
                left,
                0,
                0,
            ));
        }
        let (l, last_right, n, nb) = cur.as_mut().unwrap();
        // Insert a space where a gap is wider than ~a quarter em and the PDF didn't encode one.
        if !l.text.is_empty()
            && !l.text.ends_with(' ')
            && c != ' '
            && left - *last_right > size * 0.25
        {
            l.text.push(' ');
        }
        l.text.push(c);
        if c != ' ' {
            *n += 1;
            if ch.font_weight().is_some_and(|w| {
                matches!(
                    w,
                    PdfFontWeight::Weight600
                        | PdfFontWeight::Weight700Bold
                        | PdfFontWeight::Weight800
                        | PdfFontWeight::Weight900
                )
            }) || ch.font_name().to_ascii_lowercase().contains("bold")
            {
                *nb += 1;
            }
            l.size = l.size.max(size);
        }
        l.top = l.top.max(top);
        l.bottom = l.bottom.min(bottom);
        *last_right = right.max(*last_right);
        last_y = y;
    }
    flush(&mut cur, &mut lines);
    lines
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 12.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn list_marker(s: &str) -> Option<(bool, String)> {
    let t = s.trim_start();
    for m in ['•', '◦', '▪', '■', '●', '○', '–', '-', '*', '·', '‣', '⁃'] {
        if let Some(rest) = t.strip_prefix(m) {
            if rest.starts_with(' ') || rest.starts_with('\t') {
                return Some((false, rest.trim().to_string()));
            }
        }
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let rest = &t[digits.len()..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((true, r.trim().to_string()));
        }
    }
    None
}

/// Turn a document into blocks. `with_images` extracts embedded images (DOCX).
pub fn blocks(doc: &PdfDocument, with_images: bool) -> Vec<Block> {
    let pages: Vec<PdfPage> = doc.pages().iter().collect();
    let per_page: Vec<Vec<Line>> = pages.iter().map(lines).collect();
    let body = median(per_page.iter().flatten().map(|l| l.size).collect());
    // Distinct heading sizes, largest first → H1, H2, H3.
    let mut heading_sizes: Vec<f32> = per_page
        .iter()
        .flatten()
        .filter(|l| l.size >= body * 1.18)
        .map(|l| (l.size * 2.0).round() / 2.0)
        .collect();
    heading_sizes.sort_by(|a, b| b.partial_cmp(a).unwrap());
    heading_sizes.dedup();

    let mut out = Vec::new();
    for (pi, (page, lines)) in pages.iter().zip(per_page).enumerate() {
        if pi > 0 {
            out.push(Block::PageBreak);
        }
        // Images with their top edge, merged into the text flow by position.
        let mut images: Vec<(f32, Block)> = Vec::new();
        if with_images {
            for obj in page.objects().iter() {
                if let Some(img) = obj.as_image_object() {
                    let Ok(q) = obj.bounds() else { continue };
                    let (w, h) = (q.width().value, q.height().value);
                    if w < 24.0 || h < 24.0 {
                        continue; // decorations, bullets
                    }
                    if let Ok(raw) = img.get_raw_image() {
                        let mut png = Vec::new();
                        if raw
                            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                            .is_ok()
                        {
                            images.push((
                                q.top().value,
                                Block::Image {
                                    png,
                                    width_pt: w,
                                    height_pt: h,
                                },
                            ));
                        }
                    }
                }
            }
            images.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        }

        let mut para: Option<(String, f32, f32)> = None; // (text, bottom of last line, line height)
        let mut img_iter = images.into_iter().peekable();
        let flush = |para: &mut Option<(String, f32, f32)>, out: &mut Vec<Block>| {
            if let Some((t, _, _)) = para.take() {
                out.push(Block::Paragraph(t));
            }
        };
        for line in lines {
            while img_iter.peek().is_some_and(|(top, _)| *top > line.top) {
                flush(&mut para, &mut out);
                out.push(img_iter.next().unwrap().1);
            }
            let size_key = (line.size * 2.0).round() / 2.0;
            let short = line.text.chars().count() < 90;
            if line.size >= body * 1.18 && short {
                flush(&mut para, &mut out);
                let level = heading_sizes
                    .iter()
                    .position(|s| (*s - size_key).abs() < 0.6)
                    .unwrap_or(2)
                    .min(2) as u8
                    + 1;
                out.push(Block::Heading {
                    level,
                    text: line.text,
                });
                continue;
            }
            if line.bold > 0.8
                && short
                && !line.text.ends_with('.')
                && line.text.chars().count() < 70
            {
                flush(&mut para, &mut out);
                out.push(Block::Heading {
                    level: (heading_sizes.len().min(2) + 1) as u8,
                    text: line.text,
                });
                continue;
            }
            if let Some((ordered, text)) = list_marker(&line.text) {
                flush(&mut para, &mut out);
                out.push(Block::ListItem { ordered, text });
                continue;
            }
            let h = (line.top - line.bottom).max(1.0);
            match para.as_mut() {
                Some((t, last_bottom, lh)) if (*last_bottom - line.top) < lh.max(h) * 0.9 => {
                    // Same paragraph: join, mending hyphenated line breaks.
                    if t.ends_with('-') && !t.ends_with(" -") {
                        t.pop();
                    } else {
                        t.push(' ');
                    }
                    t.push_str(&line.text);
                    *last_bottom = line.bottom;
                    *lh = h;
                }
                _ => {
                    flush(&mut para, &mut out);
                    // A list item continues on the next indented line.
                    if let Some(Block::ListItem { text, .. }) = out.last_mut() {
                        if line.left > 0.0
                            && !line.text.is_empty()
                            && line.text.chars().next().is_some_and(|c| c.is_lowercase())
                        {
                            text.push(' ');
                            text.push_str(&line.text);
                            continue;
                        }
                    }
                    para = Some((line.text, line.bottom, h));
                }
            }
        }
        flush(&mut para, &mut out);
        out.extend(img_iter.map(|(_, b)| b));
    }
    out
}

pub fn to_markdown(blocks: &[Block]) -> String {
    let mut s = String::new();
    let mut n = 0;
    for b in blocks {
        match b {
            Block::Heading { level, text } => {
                s.push_str(&format!("{} {}\n\n", "#".repeat(*level as usize), text));
                n = 0;
            }
            Block::Paragraph(t) => {
                s.push_str(&escape_md(t));
                s.push_str("\n\n");
                n = 0;
            }
            Block::ListItem { ordered, text } => {
                if *ordered {
                    n += 1;
                    s.push_str(&format!("{n}. {}\n", escape_md(text)));
                } else {
                    s.push_str(&format!("- {}\n", escape_md(text)));
                }
            }
            Block::Image { .. } => {}
            Block::PageBreak => {
                if !s.ends_with("\n\n") {
                    s.push('\n');
                }
                n = 0;
            }
        }
    }
    s.trim_end().to_string() + "\n"
}

fn escape_md(t: &str) -> String {
    // Lines that would otherwise turn into headings or lists.
    if t.starts_with('#') || t.starts_with("- ") || t.starts_with("* ") || t.starts_with('>') {
        format!("\\{t}")
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers() {
        assert_eq!(list_marker("• Apples"), Some((false, "Apples".into())));
        assert_eq!(list_marker("12. Step"), Some((true, "Step".into())));
        assert_eq!(list_marker("3) Step"), Some((true, "Step".into())));
        assert_eq!(list_marker("2024 was a year"), None);
        assert_eq!(list_marker("-5 degrees"), None);
    }

    #[test]
    fn markdown() {
        let md = to_markdown(&[
            Block::Heading {
                level: 1,
                text: "Title".into(),
            },
            Block::Paragraph("Body text.".into()),
            Block::ListItem {
                ordered: false,
                text: "a".into(),
            },
            Block::ListItem {
                ordered: true,
                text: "b".into(),
            },
            Block::ListItem {
                ordered: true,
                text: "c".into(),
            },
        ]);
        assert_eq!(md, "# Title\n\nBody text.\n\n- a\n1. b\n2. c\n");
    }
}
