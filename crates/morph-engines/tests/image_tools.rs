//! S5: Compress, Resize, Crop, Rotate, Metadata.

use image::GenericImageView;
use morph_core::{Job, Params, Runner};
use std::path::{Path, PathBuf};

/// A photo-like JPEG with fine noise (hard to compress), about `mb` megabytes.
fn big_jpeg(path: &Path, w: u32, h: u32) {
    let mut s = 0x1234_5678u32;
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        let n = (s & 0x3f) as u8;
        image::Rgb([
            ((x / 8) % 190) as u8 + n,
            ((y / 8) % 190) as u8 + n / 2,
            120 + n,
        ])
    });
    let mut f = std::fs::File::create(path).unwrap();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 100)
        .encode_image(&img)
        .unwrap();
}

fn run(op: &str, input: &Path, params: Params) -> Vec<PathBuf> {
    let reg = morph_engines::registry();
    let r = Runner::new(&reg)
        .run(
            &Job::new(op, vec![input.to_path_buf()]).params(params),
            |_| {},
        )
        .unwrap();
    assert!(r.ok(), "{op}: {:?}", r.errors);
    r.outputs
}

#[test]
fn six_megabyte_jpeg_under_one_megabyte() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("photo.jpg");
    big_jpeg(&src, 3000, 2250);
    let size = std::fs::metadata(&src).unwrap().len();
    assert!(size > 5_000_000, "fixture is {size} bytes");
    let out = run("image.compress", &src, Params::new().with("target", "1MB"));
    let got = std::fs::metadata(&out[0]).unwrap().len();
    assert!(got <= 1_000_000, "{got} bytes");
    assert_eq!(out[0].file_name().unwrap(), "photo Compressed.jpg");
    // Still a full-size photo, not a thumbnail.
    let (w, _) = image::image_dimensions(&out[0]).unwrap();
    assert!(w >= 1500, "shrunk to {w}px");
}

#[test]
fn percent_and_presets() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("photo.jpg");
    big_jpeg(&src, 800, 600);
    let size = std::fs::metadata(&src).unwrap().len();
    let out = run("image.compress", &src, Params::new().with("percent", 50));
    assert!(std::fs::metadata(&out[0]).unwrap().len() <= size / 2);
    let out = run(
        "image.compress",
        &src,
        Params::new().with("preset", "small").with("max_width", 400),
    );
    assert_eq!(image::image_dimensions(&out[0]).unwrap(), (400, 300));
}

#[test]
fn crop_is_pixel_exact() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("photo.png");
    image::RgbImage::from_fn(4000, 3000, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 0])
    })
    .save(&src)
    .unwrap();
    let out = run(
        "image.crop",
        &src,
        Params::new()
            .with("ratio", "1:1")
            .with("width", 1080)
            .with("height", 1080),
    );
    assert_eq!(image::image_dimensions(&out[0]).unwrap(), (1080, 1080));
    assert_eq!(out[0].file_name().unwrap(), "photo Cropped.png");

    // Exact rectangle, no resize: pixels match the source region.
    let out = run(
        "image.crop",
        &src,
        Params::new()
            .with("x", 100)
            .with("y", 50)
            .with("w", 300)
            .with("h", 200),
    );
    let img = image::open(&out[0]).unwrap();
    assert_eq!(img.dimensions(), (300, 200));
    assert_eq!(img.to_rgb8().get_pixel(0, 0).0, [100, 50, 0]);
}

/// JPEG with author, copyright and GPS coordinates.
fn tagged_jpeg(path: &Path) {
    use little_exif::exif_tag::ExifTag;
    use little_exif::metadata::Metadata;
    use little_exif::rational::uR64;
    big_jpeg(path, 64, 48);
    let mut md = Metadata::new();
    md.set_tag(ExifTag::Artist("Ada Lovelace".into()));
    md.set_tag(ExifTag::Copyright("(c) Ada".into()));
    md.set_tag(ExifTag::Orientation(vec![6]));
    md.set_tag(ExifTag::GPSLatitudeRef("N".into()));
    md.set_tag(ExifTag::GPSLatitude(vec![
        uR64 {
            nominator: 48,
            denominator: 1,
        },
        uR64 {
            nominator: 51,
            denominator: 1,
        },
        uR64 {
            nominator: 30,
            denominator: 1,
        },
    ]));
    md.set_tag(ExifTag::GPSLongitudeRef("E".into()));
    md.set_tag(ExifTag::GPSLongitude(vec![
        uR64 {
            nominator: 2,
            denominator: 1,
        },
        uR64 {
            nominator: 17,
            denominator: 1,
        },
        uR64 {
            nominator: 40,
            denominator: 1,
        },
    ]));
    md.write_to_file(path).unwrap();
}

fn exiftool_gps(path: &Path) -> Option<String> {
    let out = std::process::Command::new("exiftool")
        .args(["-gps:all", "-s"])
        .arg(path)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn strip_removes_gps_keeps_orientation() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("trip.jpg");
    tagged_jpeg(&src);
    assert!(
        morph_engines::image::metadata::has_gps(&src),
        "fixture has GPS"
    );

    let out = run("image.strip", &src, Params::new());
    assert_eq!(out[0].file_name().unwrap(), "trip Clean.jpg");
    let fields = morph_engines::image::metadata::read(&out[0]).unwrap();
    assert!(!fields.iter().any(|f| f.group == "GPS"), "{fields:?}");
    assert!(!fields.iter().any(|f| f.key == "Artist"), "{fields:?}");
    // Orientation kept so the photo doesn't turn sideways.
    assert_eq!(
        morph_engines::image::metadata::orientation(&std::fs::read(&out[0]).unwrap()),
        Some(6)
    );
    // Still decodes.
    image::open(&out[0]).unwrap();
    if let Some(gps) = exiftool_gps(&out[0]) {
        assert!(gps.trim().is_empty(), "exiftool still sees: {gps}");
        assert!(
            !exiftool_gps(&src).unwrap().trim().is_empty(),
            "exiftool sees GPS in the fixture"
        );
    }
}

#[test]
fn edit_fields_and_remove_location() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("trip.jpg");
    tagged_jpeg(&src);
    let out = run(
        "image.metadata",
        &src,
        Params::new()
            .with(
                "set",
                serde_json::json!({ "artist": "Grace Hopper", "description": "Harbour" }),
            )
            .with("remove_gps", true),
    );
    let fields = morph_engines::image::metadata::read(&out[0]).unwrap();
    let get = |k: &str| {
        fields
            .iter()
            .find(|f| f.key == k)
            .map(|f| f.value.clone())
            .unwrap_or_default()
    };
    assert!(get("Artist").contains("Grace Hopper"), "{fields:?}");
    assert!(get("ImageDescription").contains("Harbour"));
    assert!(get("Copyright").contains("Ada"), "untouched fields stay");
    assert!(!fields.iter().any(|f| f.group == "GPS"), "{fields:?}");
    // Source untouched.
    assert!(morph_engines::image::metadata::has_gps(&src));
}

#[test]
fn strip_png_and_webp() {
    let d = tempfile::tempdir().unwrap();
    for ext in ["png", "webp"] {
        let src = d.path().join(format!("pic.{ext}"));
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(32, 32));
        morph_engines::image::encode::encode(&img, ext, &src, &Default::default()).unwrap();
        let mut md = little_exif::metadata::Metadata::new();
        md.set_tag(little_exif::exif_tag::ExifTag::Artist("Someone".into()));
        md.write_to_file(&src).unwrap();
        assert!(
            morph_engines::image::metadata::read(&src)
                .unwrap()
                .iter()
                .any(|f| f.key == "Artist"),
            "{ext} fixture"
        );
        let out = run("image.strip", &src, Params::new());
        assert!(
            morph_engines::image::metadata::read(&out[0])
                .unwrap()
                .is_empty(),
            "{ext}"
        );
        image::open(&out[0]).unwrap();
    }
}

#[test]
fn resize_by_percent_and_box() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("photo.jpg");
    big_jpeg(&src, 800, 600);
    let half = run("image.resize", &src, Params::new());
    assert_eq!(half[0].file_name().unwrap(), "photo Resized.jpg");
    assert_eq!(image::image_dimensions(&half[0]).unwrap(), (400, 300));
    let w = run("image.resize", &src, Params::new().with("width", 1600));
    assert_eq!(
        image::image_dimensions(&w[0]).unwrap(),
        (1600, 1200),
        "upscales"
    );
    let fit = run(
        "image.resize",
        &src,
        Params::new().with("width", 200).with("height", 200),
    );
    assert_eq!(
        image::image_dimensions(&fit[0]).unwrap(),
        (200, 150),
        "fits the box"
    );
}

#[test]
fn rotate_and_flip() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("mark.png");
    // Red top-left pixel on a 4×2 image.
    let mut img = image::RgbaImage::from_pixel(4, 2, image::Rgba([0, 0, 255, 255]));
    img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    img.save(&src).unwrap();
    let red = |p: &Path| {
        let i = image::open(p).unwrap().to_rgba8();
        let pos = i
            .enumerate_pixels()
            .find(|(_, _, px)| px[0] == 255)
            .unwrap();
        (i.dimensions(), (pos.0, pos.1))
    };
    let cases = [
        ("right", ((2, 4), (1, 0))),
        ("left", ((2, 4), (0, 3))),
        ("180", ((4, 2), (3, 1))),
        ("flip-horizontal", ((4, 2), (3, 0))),
        ("flip-vertical", ((4, 2), (0, 1))),
    ];
    for (turn, want) in cases {
        let out = run("image.rotate", &src, Params::new().with("turn", turn));
        assert_eq!(red(&out[0]), want, "{turn}");
        std::fs::remove_file(&out[0]).unwrap();
    }
}
