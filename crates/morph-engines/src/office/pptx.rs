//! PDF → PPTX: one slide per page, the page rendered as a picture.

use anyhow::{Context, Result};
use morph_core::{OpContext, OpSpec, Operation, Registry};
use std::io::Write;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

pub fn register(reg: &mut Registry) {
    let mut spec = OpSpec::convert("pptx", "PPTX", &["application/pdf"]).order(6);
    spec.id = "pdf.to-pptx".into();
    reg.register(PdfToPptx { spec });
}

struct PdfToPptx {
    spec: OpSpec,
}

impl Operation for PdfToPptx {
    fn spec(&self) -> &OpSpec {
        &self.spec
    }
    fn available(&self) -> bool {
        crate::pdf::lib::available()
    }
    fn run(&self, ctx: &OpContext) -> Result<Vec<PathBuf>> {
        use pdfium_render::prelude::*;
        let pdfium = crate::pdf::lib::pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(ctx.input(), None)
            .context("opening PDF")?;
        let mut slides = Vec::new();
        let n = doc.pages().len();
        for (i, page) in doc.pages().iter().enumerate() {
            ctx.progress.check()?;
            let img = page
                .render_with_config(&PdfRenderConfig::new().set_target_width(1920))?
                .as_image()?;
            let img = image::DynamicImage::ImageRgb8(img.to_rgb8());
            let jpeg = crate::image::encode::encode_to_vec(
                &img,
                "jpg",
                &crate::image::encode::EncodeOptions {
                    quality: 90,
                    ..Default::default()
                },
            )?;
            slides.push((img.width(), img.height(), jpeg));
            ctx.progress.set((i + 1) as f32 / n as f32 * 0.9);
        }
        let out = ctx.sink.file(ctx.input(), "pptx")?;
        write_pptx(&slides, out.path())?;
        Ok(vec![out.commit()?])
    }
}

const EMU_W: u64 = 12_192_000; // 13.333 in

/// Write a minimal, valid PPTX (opens in PowerPoint, LibreOffice and Keynote).
pub fn write_pptx(slides: &[(u32, u32, Vec<u8>)], path: &std::path::Path) -> Result<()> {
    let (w0, h0) = slides.first().map(|s| (s.0, s.1)).unwrap_or((16, 9));
    let (cx, cy) = (EMU_W, EMU_W * h0 as u64 / w0.max(1) as u64);
    let f = std::fs::File::create(path)?;
    let mut z = zip::ZipWriter::new(f);
    let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, data: &[u8]| -> Result<()> {
        z.start_file(name, o)?;
        z.write_all(data)?;
        Ok(())
    };
    let n = slides.len();
    let mut ct = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
    );
    for i in 1..=n {
        ct.push_str(&format!(r#"<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#));
    }
    ct.push_str("</Types>");
    put("[Content_Types].xml", ct.as_bytes())?;
    put("_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#)?;
    put("docProps/app.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Metamorphosis {}</Application><Slides>{n}</Slides></Properties>"#, morph_core::VERSION).as_bytes())?;
    put("docProps/core.xml", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:creator>Metamorphosis</dc:creator></cp:coreProperties>"#)?;

    let mut pres = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>"#);
    let mut pres_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>"#,
    );
    for i in 1..=n {
        pres.push_str(&format!(
            r#"<p:sldId id="{}" r:id="rId{}"/>"#,
            255 + i,
            i + 2
        ));
        pres_rels.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{i}.xml"/>"#, i + 2));
    }
    pres.push_str(&format!(r#"</p:sldIdLst><p:sldSz cx="{cx}" cy="{cy}"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#));
    pres_rels.push_str("</Relationships>");
    put("ppt/presentation.xml", pres.as_bytes())?;
    put("ppt/_rels/presentation.xml.rels", pres_rels.as_bytes())?;

    let tree = r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>"#;
    put("ppt/slideMasters/slideMaster1.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">{tree}<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#).as_bytes())?;
    put("ppt/slideMasters/_rels/slideMaster1.xml.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#)?;
    put("ppt/slideLayouts/slideLayout1.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank">{tree}</p:sldLayout>"#).as_bytes())?;
    put("ppt/slideLayouts/_rels/slideLayout1.xml.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#)?;
    put("ppt/theme/theme1.xml", THEME.as_bytes())?;

    for (i, (w, h, jpeg)) in slides.iter().enumerate() {
        let k = i + 1;
        // Fit the page inside the slide, centred.
        let s = (cx as f64 / *w as f64).min(cy as f64 / *h as f64);
        let (pw, ph) = ((*w as f64 * s) as u64, (*h as f64 * s) as u64);
        let (px, py) = ((cx - pw) / 2, (cy - ph) / 2);
        put(&format!("ppt/media/image{k}.jpeg"), jpeg)?;
        put(&format!("ppt/slides/slide{k}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:pic><p:nvPicPr><p:cNvPr id="2" name="Page {k}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{px}" y="{py}"/><a:ext cx="{pw}" cy="{ph}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:spTree></p:cSld></p:sld>"#).as_bytes())?;
        put(&format!("ppt/slides/_rels/slide{k}.xml.rels"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image{k}.jpeg"/></Relationships>"#).as_bytes())?;
    }
    z.finish()?;
    Ok(())
}

const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Metamorphosis"><a:themeElements><a:clrScheme name="Metamorphosis"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="04070C"/></a:dk2><a:lt2><a:srgbClr val="BDF4FF"/></a:lt2><a:accent1><a:srgbClr val="4FE3FF"/></a:accent1><a:accent2><a:srgbClr val="FFB347"/></a:accent2><a:accent3><a:srgbClr val="FF5EA8"/></a:accent3><a:accent4><a:srgbClr val="9CFF6E"/></a:accent4><a:accent5><a:srgbClr val="5856D6"/></a:accent5><a:accent6><a:srgbClr val="6FB6C6"/></a:accent6><a:hlink><a:srgbClr val="4FE3FF"/></a:hlink><a:folHlink><a:srgbClr val="6FB6C6"/></a:folHlink></a:clrScheme><a:fontScheme name="Metamorphosis"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Metamorphosis"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="38100"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;
