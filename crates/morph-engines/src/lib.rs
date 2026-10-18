//! Conversion and editing engines for Metamorphosis.
//!
//! Each module registers its operations with a [`morph_core::Registry`].

#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "media")]
pub mod media;
#[cfg(feature = "ml")]
pub mod ml;
#[cfg(feature = "office")]
pub mod office;
#[cfg(feature = "pdf")]
pub mod pdf;

use morph_core::Registry;

/// Register every engine compiled into this build.
pub fn register_all(reg: &mut Registry) {
    #[cfg(feature = "image")]
    image::register(reg);
    #[cfg(feature = "pdf")]
    pdf::register(reg);
    #[cfg(feature = "ml")]
    ml::ops::register(reg);
    #[cfg(feature = "media")]
    media::register(reg);
    #[cfg(feature = "office")]
    office::register(reg);
}

/// A registry with every built-in operation.
pub fn registry() -> Registry {
    let mut r = Registry::new();
    register_all(&mut r);
    r
}

/// Recipes shipped with Metamorphosis (`recipes/` in the repository).
pub const BUILTIN_RECIPES: &[(&str, &str)] = &[
    (
        "square-post.toml",
        include_str!("../../../recipes/square-post.toml"),
    ),
    (
        "web-ready.toml",
        include_str!("../../../recipes/web-ready.toml"),
    ),
];

/// Engines plus built-in and user recipes. Returns recipe files that failed to load.
pub fn registry_with_recipes() -> (Registry, Vec<(std::path::PathBuf, String)>) {
    use morph_core::recipe::{self, RecipeFile, RecipeOp};
    let base = std::sync::Arc::new(registry());
    let mut reg = (*base).clone();
    let mut errors = Vec::new();
    for (name, text) in BUILTIN_RECIPES {
        match RecipeFile::parse(text).and_then(|f| RecipeOp::new(&f, base.clone(), None)) {
            Ok(op) => reg.register(op),
            Err(e) => errors.push((std::path::PathBuf::from(name), format!("{e:#}"))),
        }
    }
    for (path, file) in recipe::load_dir(&recipe::user_dir()) {
        match file.and_then(|f| recipe_op(&f, base.clone(), &path)) {
            Ok(op) => reg.register_arc(op),
            Err(e) => errors.push((path, format!("{e:#}"))),
        }
    }
    (reg, errors)
}

/// Build the operation for a user recipe file (chain recipe or, from S19, command recipe).
pub fn recipe_op(
    f: &morph_core::recipe::RecipeFile,
    base: std::sync::Arc<Registry>,
    path: &std::path::Path,
) -> anyhow::Result<std::sync::Arc<dyn morph_core::Operation>> {
    Ok(std::sync::Arc::new(morph_core::recipe::RecipeOp::new(
        f,
        base,
        Some(path.to_path_buf()),
    )?))
}
