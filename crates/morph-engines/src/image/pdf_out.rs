//! Combine images into one PDF, one page per image.
//!
//! JPEGs are embedded byte-for-byte (DCTDecode) so they lose nothing; other
//! images are stored losslessly with Flate, with a soft mask for transparency.

use super::decode::decode;
use anyhow::{Context, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use image::GenericImageView;
use lopdf::content::{Content, Operation as PdfOp};
use lopdf::{dictionary, Document, Object, Stream};
use morph_core::{Kind, Progress};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Page sizing for images placed in a PDF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// Page matches the image at `dpi`.
    Fit { dpi: f32 },
    /// A4 portrait/landscape (follows the image), image centred with a margin in points.
    A4 { margin: f32 },
    /// US Letter, same rules.
    Letter { margin: f32 },
}

pub fn images_to_pdf(
    inputs: &[(PathBuf, Kind)],
    out: &Path,
    size: PageSize,
    progress: &Progress,
) -> Result<()> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();

    for (i, (path, kind)) in inputs.iter().enumerate() {
        progress.check()?;
        let (xobj, w, h) = image_xobject(&mut doc, path, kind)
            .with_context(|| format!("adding {}", path.display()))?;

        let (page_w, page_h, x, y, dw, dh) = layout(w as f32, h as f32, size);
        let content = Content {
            operations: vec![
                PdfOp::new("q", vec![]),
                PdfOp::new(
                    "cm",
                    vec![dw.into(), 0.into(), 0.into(), dh.into(), x.into(), y.into()],
                ),
                PdfOp::new("Do", vec!["Im0".into()]),
                PdfOp::new("Q", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), page_w.into(), page_h.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => xobj } },
        });
        kids.push(Object::Reference(page_id));
        progress.set((i + 1) as f32 / inputs.len() as f32);
    }

    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let info_id = doc.add_object(dictionary! {
        "Producer" => Object::string_literal(format!("Metamorphosis {}", morph_core::VERSION)),
    });
    doc.trailer.set("Info", info_id);
    doc.compress();
    doc.save(out).context("writing PDF")?;
    Ok(())
}

/// Page size, image origin and drawn size, all in points.
fn layout(w: f32, h: f32, size: PageSize) -> (f32, f32, f32, f32, f32, f32) {
    match size {
        PageSize::Fit { dpi } => {
            let (pw, ph) = (w * 72.0 / dpi, h * 72.0 / dpi);
            (pw, ph, 0.0, 0.0, pw, ph)
        }
        PageSize::A4 { margin } | PageSize::Letter { margin } => {
            let (a, b) = if matches!(size, PageSize::A4 { .. }) {
                (595.28, 841.89)
            } else {
                (612.0, 792.0)
            };
            let (pw, ph) = if w > h { (b, a) } else { (a, b) };
            let s = ((pw - 2.0 * margin) / w).min((ph - 2.0 * margin) / h);
            let (dw, dh) = (w * s, h * s);
            (pw, ph, (pw - dw) / 2.0, (ph - dh) / 2.0, dw, dh)
        }
    }
}

fn image_xobject(
    doc: &mut Document,
    path: &Path,
    kind: &Kind,
) -> Result<(lopdf::ObjectId, u32, u32)> {
    if kind.ext == "jpg" {
        if let Some(r) = jpeg_passthrough(doc, path)? {
            return Ok(r);
        }
    }
    let img = decode(path, kind)?;
    let (w, h) = img.dimensions();
    let has_alpha = img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p[3] < 255);

    let rgb = img.to_rgb8();
    let mut dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image",
        "Width" => w as i64, "Height" => h as i64,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        "Filter" => "FlateDecode",
    };
    if has_alpha {
        let alpha: Vec<u8> = img.to_rgba8().pixels().map(|p| p[3]).collect();
        let smask = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
            },
            zlib(&alpha)?,
        );
        let smask_id = doc.add_object(smask);
        dict.set("SMask", smask_id);
    }
    let mut stream = Stream::new(dict, zlib(rgb.as_raw())?);
    stream.allows_compression = false;
    Ok((doc.add_object(stream), w, h))
}

/// Embed a baseline/progressive JPEG as-is when it needs no rotation.
fn jpeg_passthrough(
    doc: &mut Document,
    path: &Path,
) -> Result<Option<(lopdf::ObjectId, u32, u32)>> {
    use image::ImageDecoder;
    let bytes = std::fs::read(path)?;
    let mut dec = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&bytes))?;
    let orientation = dec
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    if orientation != image::metadata::Orientation::NoTransforms {
        return Ok(None);
    }
    let (w, h) = dec.dimensions();
    let cs = match dec.color_type() {
        image::ColorType::L8 => "DeviceGray",
        image::ColorType::Rgb8 => "DeviceRGB",
        _ => return Ok(None), // CMYK and friends: re-encode
    };
    let mut stream = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => w as i64, "Height" => h as i64,
            "ColorSpace" => cs, "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        bytes,
    );
    stream.allows_compression = false;
    Ok(Some((doc.add_object(stream), w, h)))
}

fn zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data)?;
    Ok(e.finish()?)
}
