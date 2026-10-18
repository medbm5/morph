//! Image metadata: read EXIF / XMP / IPTC, edit common fields, strip everything.
//!
//! Stripping is lossless for JPEG, PNG and WebP (container surgery, no re-encode):
//! the ICC colour profile is kept, and a non-default orientation survives as a
//! one-tag EXIF block so the picture doesn't turn sideways.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    /// `EXIF`, `GPS`, `XMP` or `IPTC`.
    pub group: String,
    pub key: String,
    pub value: String,
}

/// Editable fields (EXIF tags written by `image.metadata`).
pub const EDITABLE: &[(&str, &str)] = &[
    ("artist", "Author"),
    ("copyright", "Copyright"),
    ("description", "Description"),
    ("datetime", "Date taken"),
    ("make", "Camera make"),
    ("model", "Camera model"),
    ("software", "Software"),
];

/// Read every field we can find.
pub fn read(path: &Path) -> Result<Vec<Field>> {
    let data = std::fs::read(path)?;
    let mut out = Vec::new();
    if let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(&data)) {
        for f in exif.fields() {
            let group = if f.ifd_num == exif::In::PRIMARY && is_gps(f.tag) {
                "GPS"
            } else {
                "EXIF"
            };
            let value = f.display_value().with_unit(&exif).to_string();
            if value.len() > 200 {
                continue; // maker notes and thumbnails
            }
            out.push(Field {
                group: group.into(),
                key: f.tag.to_string(),
                value,
            });
        }
    }
    if out.is_empty() {
        out.extend(little_exif_fields(&data, path));
    }
    if let Some(xmp) = find_xmp(&data) {
        out.extend(parse_xmp(&xmp).into_iter().map(|(k, v)| Field {
            group: "XMP".into(),
            key: k,
            value: v,
        }));
    }
    if let Some(iptc) = jpeg_iptc(&data) {
        out.extend(iptc.into_iter().map(|(k, v)| Field {
            group: "IPTC".into(),
            key: k,
            value: v,
        }));
    }
    Ok(out)
}

/// Fallback for EXIF stored where kamadak-exif doesn't look (PNG text chunks, some WebP).
fn little_exif_fields(data: &[u8], path: &Path) -> Vec<Field> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = match ext.as_str() {
        "jpeg" => "jpg".to_string(),
        "tif" => "tiff".to_string(),
        e => e.to_string(),
    };
    let Ok(ft) = file_ext(&ext) else {
        return vec![];
    };
    let Ok(md) = little_exif::metadata::Metadata::new_from_vec(&data.to_vec(), ft) else {
        return vec![];
    };
    let mut out = Vec::new();
    for tag in &md {
        // Debug looks like `Artist("Someone")` or `GPSLatitude([uR64 { .. }])`.
        let dbg = format!("{tag:?}");
        let (key, value) = match dbg.split_once('(') {
            Some((k, v)) => (
                k.to_string(),
                v.trim_end_matches(')').trim_matches('"').to_string(),
            ),
            None => (dbg.clone(), String::new()),
        };
        if key.starts_with("Unknown")
            || value.len() > 200
            || key == "ExifOffset"
            || key == "GPSInfo"
        {
            continue;
        }
        let group = if key.starts_with("GPS") {
            "GPS"
        } else {
            "EXIF"
        };
        out.push(Field {
            group: group.into(),
            key,
            value,
        });
    }
    out
}

fn is_gps(tag: exif::Tag) -> bool {
    tag.context() == exif::Context::Gps
}

/// Does the file carry GPS coordinates?
pub fn has_gps(path: &Path) -> bool {
    read(path)
        .map(|f| {
            f.iter()
                .any(|f| f.group == "GPS" || f.key.contains("GPS") || f.key.contains("Location"))
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------- XMP

fn find_xmp(data: &[u8]) -> Option<String> {
    let start = find(data, b"<x:xmpmeta")?;
    let end = find(&data[start..], b"</x:xmpmeta>")? + start + b"</x:xmpmeta>".len();
    Some(String::from_utf8_lossy(&data[start..end]).into_owned())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Simple properties (`ns:Name="v"` attributes and `<ns:Name>v</ns:Name>` / `rdf:li` values).
fn parse_xmp(xml: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    // Attributes on rdf:Description.
    for desc in xml.split("<rdf:Description").skip(1) {
        let mut head = &desc[..desc.find('>').unwrap_or(desc.len())];
        // name="value" pairs; values may contain spaces.
        while let Some(eq) = head.find("=\"") {
            let name = head[..eq]
                .rsplit(|c: char| c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            let rest = &head[eq + 2..];
            let Some(end) = rest.find('"') else { break };
            if name.contains(':') && !name.starts_with("xmlns") && !name.starts_with("rdf:") {
                out.push((name, xml_unescape(&rest[..end])));
            }
            head = &rest[end + 1..];
        }
    }
    // Elements: take the text of the innermost element (or the first rdf:li).
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        if tag.starts_with('/')
            || tag.starts_with('?')
            || tag.starts_with("rdf:")
            || tag.starts_with("x:")
            || !tag.contains(':')
        {
            continue;
        }
        let name = tag
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        if name.is_empty() || name.starts_with("rdf") || tag.ends_with('/') {
            continue;
        }
        let close = format!("</{name}>");
        let Some(cend) = rest.find(&close) else {
            continue;
        };
        let inner = &rest[end + 1..cend];
        let text = match inner.find("<rdf:li") {
            Some(li) => {
                let s = &inner[li..];
                let a = s.find('>').map(|a| a + 1).unwrap_or(0);
                let b = s.find("</rdf:li>").unwrap_or(s.len());
                &s[a..b.max(a)]
            }
            None if !inner.contains('<') => inner,
            None => continue,
        };
        let text = xml_unescape(text.trim());
        if !text.is_empty() && !out.iter().any(|(k, _)| k == &name) {
            out.push((name, text));
        }
    }
    out
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// ---------------------------------------------------------------- IPTC (JPEG APP13)

fn jpeg_segments(data: &[u8]) -> Result<Vec<(u8, std::ops::Range<usize>)>> {
    if !data.starts_with(&[0xFF, 0xD8]) {
        bail!("not a JPEG");
    }
    let mut segs = Vec::new();
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            bail!("corrupt JPEG marker at {i}");
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            segs.push((marker, i..data.len())); // scan data and the rest
            break;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let end = (i + 2 + len).min(data.len());
        segs.push((marker, i..end));
        i = end;
    }
    Ok(segs)
}

fn jpeg_iptc(data: &[u8]) -> Option<Vec<(String, String)>> {
    let segs = jpeg_segments(data).ok()?;
    let (_, r) = segs
        .iter()
        .find(|(m, r)| *m == 0xED && data[r.start + 4..].starts_with(b"Photoshop 3.0\0"))?;
    let mut p = &data[r.start + 4 + 14..r.end];
    let mut out = Vec::new();
    // 8BIM resources.
    while p.len() >= 12 && p.starts_with(b"8BIM") {
        let id = u16::from_be_bytes([p[4], p[5]]);
        let name_len = p[6] as usize;
        let mut off = 7 + name_len;
        if off % 2 == 1 {
            off += 1;
        }
        if p.len() < off + 4 {
            break;
        }
        let size = u32::from_be_bytes([p[off], p[off + 1], p[off + 2], p[off + 3]]) as usize;
        let body = &p[(off + 4).min(p.len())..(off + 4 + size).min(p.len())];
        if id == 0x0404 {
            let mut b = body;
            while b.len() >= 5 && b[0] == 0x1C {
                let (rec, ds) = (b[1], b[2]);
                let len = u16::from_be_bytes([b[3], b[4]]) as usize;
                let v = &b[5..(5 + len).min(b.len())];
                if rec == 2 {
                    let name = match ds {
                        5 => "Object Name",
                        25 => "Keywords",
                        80 => "By-line",
                        90 => "City",
                        101 => "Country",
                        105 => "Headline",
                        110 => "Credit",
                        115 => "Source",
                        116 => "Copyright Notice",
                        120 => "Caption",
                        _ => "",
                    };
                    if !name.is_empty() {
                        out.push((name.to_string(), String::from_utf8_lossy(v).into_owned()));
                    }
                }
                b = &b[(5 + len).min(b.len())..];
            }
        }
        let mut next = off + 4 + size;
        if next % 2 == 1 {
            next += 1;
        }
        p = &p[next.min(p.len())..];
    }
    Some(out)
}

// ---------------------------------------------------------------- strip

/// EXIF orientation (1–8) of a file, if present.
pub fn orientation(data: &[u8]) -> Option<u32> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(data))
        .ok()?;
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
        .get_uint(0)
}

/// Minimal big-endian EXIF block holding only the orientation tag.
fn orientation_exif(o: u32) -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(b"MM\0\x2a\0\0\0\x08"); // TIFF header, IFD0 at 8
    t.extend_from_slice(&1u16.to_be_bytes()); // one entry
    t.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    t.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    t.extend_from_slice(&1u32.to_be_bytes());
    t.extend_from_slice(&(o as u16).to_be_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0u32.to_be_bytes()); // no next IFD
    t
}

/// Remove all metadata from an encoded image. Supported: JPEG, PNG, WebP.
pub fn strip(data: &[u8], ext: &str) -> Result<Vec<u8>> {
    let keep_orientation = orientation(data).filter(|&o| o > 1 && o <= 8);
    match ext {
        "jpg" => strip_jpeg(data, keep_orientation),
        "png" => strip_png(data, keep_orientation),
        "webp" => strip_webp(data, keep_orientation),
        other => bail!("can't strip metadata from .{other} without re-encoding"),
    }
}

fn strip_jpeg(data: &[u8], orient: Option<u32>) -> Result<Vec<u8>> {
    let segs = jpeg_segments(data)?;
    let mut out = vec![0xFF, 0xD8];
    let mut wrote_orientation = false;
    for (marker, r) in segs {
        let seg = &data[r.clone()];
        let payload = if seg.len() > 4 { &seg[4..] } else { &[][..] };
        let keep = match marker {
            0xE0 => payload.starts_with(b"JFIF\0") || payload.starts_with(b"JFXX\0"),
            0xE2 => payload.starts_with(b"ICC_PROFILE\0"),
            0xEE => payload.starts_with(b"Adobe"), // colour transform flag: affects decoding
            0xE1..=0xEF | 0xFE => false,           // EXIF, XMP, IPTC, comments, maker data
            _ => true,
        };
        if marker == 0xDB || marker == 0xC4 || (0xC0..=0xCF).contains(&marker) || marker == 0xDA {
            if let (Some(o), false) = (orient, wrote_orientation) {
                let mut app1 = b"Exif\0\0".to_vec();
                app1.extend(orientation_exif(o));
                out.extend_from_slice(&[0xFF, 0xE1]);
                out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
                out.extend(app1);
            }
            wrote_orientation = true;
        }
        if keep {
            out.extend_from_slice(seg);
        }
    }
    Ok(out)
}

fn png_crc(bytes: &[u8]) -> u32 {
    let mut h = flate2::Crc::new();
    h.update(bytes);
    h.sum()
}

fn strip_png(data: &[u8], mut orient: Option<u32>) -> Result<Vec<u8>> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !data.starts_with(SIG) {
        bail!("not a PNG");
    }
    let mut out = SIG.to_vec();
    let mut i = 8;
    while i + 12 <= data.len() {
        let len = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let kind = &data[i + 4..i + 8];
        let end = (i + 12 + len).min(data.len());
        let drop = matches!(kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME");
        if kind == b"IDAT" || kind == b"IEND" {
            if let Some(o) = orient.take() {
                let body = orientation_exif(o);
                let mut chunk = Vec::new();
                chunk.extend_from_slice(&(body.len() as u32).to_be_bytes());
                chunk.extend_from_slice(b"eXIf");
                chunk.extend_from_slice(&body);
                let crc = png_crc(&chunk[4..]);
                chunk.extend_from_slice(&crc.to_be_bytes());
                out.extend(chunk);
            }
        }
        if !drop {
            out.extend_from_slice(&data[i..end]);
        }
        i = end;
    }
    Ok(out)
}

fn strip_webp(data: &[u8], orient: Option<u32>) -> Result<Vec<u8>> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        bail!("not a WebP");
    }
    let mut chunks = Vec::new();
    let mut i = 12;
    while i + 8 <= data.len() {
        let fourcc = &data[i..i + 4];
        let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        let end = (i + 8 + len + (len & 1)).min(data.len());
        if fourcc != b"EXIF" && fourcc != b"XMP " {
            chunks.push(data[i..end].to_vec());
        }
        i = end;
    }
    if let Some(o) = orient {
        let body = orientation_exif(o);
        let mut c = b"EXIF".to_vec();
        c.extend_from_slice(&(body.len() as u32).to_le_bytes());
        c.extend(body);
        if c.len() % 2 == 1 {
            c.push(0);
        }
        chunks.push(c);
    }
    // VP8X flags: clear XMP (0x04); EXIF (0x08) only if we kept an orientation.
    for c in chunks.iter_mut() {
        if &c[..4] == b"VP8X" && c.len() > 8 {
            c[8] &= !0x04;
            if orient.is_none() {
                c[8] &= !0x08;
            }
        }
    }
    let body_len: usize = 4 + chunks.iter().map(Vec::len).sum::<usize>();
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body_len as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    for c in chunks {
        out.extend(c);
    }
    Ok(out)
}

// ---------------------------------------------------------------- edit

/// Changes requested by the metadata editor.
#[derive(Debug, Default, Clone)]
pub struct Edit {
    pub set: Vec<(String, String)>,
    pub remove_gps: bool,
    pub remove_xmp_iptc: bool,
}

fn file_ext(ext: &str) -> Result<little_exif::filetype::FileExtension> {
    use little_exif::filetype::FileExtension as F;
    Ok(match ext {
        "jpg" => F::JPEG,
        "png" => F::PNG {
            as_zTXt_chunk: false,
        },
        "webp" => F::WEBP,
        "tiff" => F::TIFF,
        other => bail!("can't edit metadata of .{other} files"),
    })
}

/// Apply `edit` to an encoded image, returning the new bytes.
pub fn apply(data: &[u8], ext: &str, edit: &Edit) -> Result<Vec<u8>> {
    use little_exif::exif_tag::ExifTag;
    use little_exif::ifd::ExifTagGroup;
    use little_exif::metadata::Metadata;
    let mut bytes = data.to_vec();
    if edit.remove_xmp_iptc && ext == "jpg" {
        let segs = jpeg_segments(&bytes)?;
        let mut out = vec![0xFF, 0xD8];
        for (m, r) in segs {
            let p = &bytes[r.clone()];
            let xmp =
                m == 0xE1 && p.len() > 4 && p[4..].starts_with(b"http://ns.adobe.com/xap/1.0/");
            if !(xmp || m == 0xED) {
                out.extend_from_slice(p);
            }
        }
        bytes = out;
    }
    let ft = file_ext(ext)?;
    let mut md = Metadata::new_from_vec(&bytes, ft).unwrap_or_else(|_| Metadata::new());
    for (k, v) in &edit.set {
        let tag = match k.as_str() {
            "artist" => ExifTag::Artist(v.clone()),
            "copyright" => ExifTag::Copyright(v.clone()),
            "description" => ExifTag::ImageDescription(v.clone()),
            "datetime" => ExifTag::DateTimeOriginal(v.clone()),
            "make" => ExifTag::Make(v.clone()),
            "model" => ExifTag::Model(v.clone()),
            "software" => ExifTag::Software(v.clone()),
            other => bail!("unknown metadata field `{other}`"),
        };
        if v.is_empty() {
            md.remove_tag(tag);
        } else {
            md.set_tag(tag);
        }
    }
    if edit.remove_gps {
        for hex in 0x0000..=0x001F {
            md.remove_tag_by_hex_group(hex, ExifTagGroup::GPS);
        }
    }
    md.write_to_vec(&mut bytes, ft).context("writing EXIF")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmp_simple_properties() {
        let x = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description xmp:CreatorTool="Camera 1.0" xmlns:dc="x">
          <dc:creator><rdf:Seq><rdf:li>Ada</rdf:li></rdf:Seq></dc:creator><dc:format>image/jpeg</dc:format>
          </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let p = parse_xmp(x);
        assert!(
            p.contains(&("xmp:CreatorTool".into(), "Camera 1.0".into())),
            "{p:?}"
        );
        assert!(p.contains(&("dc:creator".into(), "Ada".into())), "{p:?}");
        assert!(
            p.contains(&("dc:format".into(), "image/jpeg".into())),
            "{p:?}"
        );
    }

    #[test]
    fn orientation_block_parses() {
        let mut app = b"Exif\0\0".to_vec();
        app.extend(orientation_exif(6));
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend_from_slice(&((app.len() + 2) as u16).to_be_bytes());
        jpeg.extend(app);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        assert_eq!(orientation(&jpeg), Some(6));
    }
}
