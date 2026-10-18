//! S8: a saved recipe runs from one segment and via `morph chain` with identical output.

use morph_core::{recipe::parse_step, Job, Runner};
use sha2::{Digest, Sha256};

fn hash(p: &std::path::Path) -> String {
    hex::encode(Sha256::digest(std::fs::read(p).unwrap()))
}

#[test]
fn square_post_recipe_matches_the_chain() {
    if !morph_engines::pdf::lib::available() {
        eprintln!("PDFium not found — skipping");
        return;
    }
    let (reg, errors) = morph_engines::registry_with_recipes();
    assert!(errors.iter().all(|(p, _)| !p.starts_with("square")), "{errors:?}");
    let d = tempfile::tempdir().unwrap();
    let make = |dir: &str| {
        let sub = d.path().join(dir);
        std::fs::create_dir_all(&sub).unwrap();
        let png = sub.join("page.png");
        image::RgbImage::from_fn(600, 400, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])).save(&png).unwrap();
        let r = Runner::new(&reg).run(&Job::new("convert.pdf", vec![png.clone()]), |_| {}).unwrap();
        std::fs::remove_file(png).unwrap();
        r.outputs[0].clone()
    };
    let (a, b) = (make("a"), make("b"));

    let seg = Runner::new(&reg).run(&Job::new("recipe.square-post", vec![a.clone()]), |_| {}).unwrap();
    assert!(seg.ok(), "{:?}", seg.errors);
    let steps: Vec<_> = ["convert:jpg", "crop:1:1", "bg:preset=soft,size=1080x1080,padding=6"]
        .iter()
        .map(|s| parse_step(s).unwrap())
        .collect();
    let chain = Runner::new(&reg).run_chain(&steps, &[b.clone()], |_| {}).unwrap();
    assert!(chain.ok(), "{:?}", chain.errors);

    assert_eq!(seg.outputs.len(), 1);
    assert_eq!(image::image_dimensions(&seg.outputs[0]).unwrap(), (1080, 1080));
    assert_eq!(hash(&seg.outputs[0]), hash(&chain.outputs[0]), "segment and chain differ");
    // Only the source and the final result beside it.
    assert_eq!(std::fs::read_dir(a.parent().unwrap()).unwrap().count(), 2);
}
