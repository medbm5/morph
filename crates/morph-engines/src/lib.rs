//! Conversion and editing engines for Metamorphosis.
//!
//! Each module registers its operations with a [`morph_core::Registry`].

#[cfg(feature = "image")]
pub mod image;

use morph_core::Registry;

/// Register every engine compiled into this build.
pub fn register_all(reg: &mut Registry) {
    #[cfg(feature = "image")]
    image::register(reg);
}

/// A registry with every built-in operation.
pub fn registry() -> Registry {
    let mut r = Registry::new();
    register_all(&mut r);
    r
}
