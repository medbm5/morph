//! X11 without a compositor: ARGB windows aren't translucent, so the overlay
//! becomes a circular window (XShape via GDK) with an opaque backdrop.

use gtk::prelude::*;

pub fn apply(w: &tauri::WebviewWindow, size: i32) {
    if morph_platform::has_compositor() {
        return;
    }
    let Ok(gw) = w.gtk_window() else { return };
    let Some(gdk_window) = gw.window() else {
        return;
    };
    let scale = gdk_window.scale_factor().max(1);
    let logical = size / scale;
    let r = (logical as f64) * 588.0 / 600.0 / 2.0;
    let c = logical as f64 / 2.0;
    // Build the circle from one rectangle per scanline.
    let region = cairo::Region::create();
    for y in 0..logical {
        let dy = y as f64 + 0.5 - c;
        if dy.abs() > r {
            continue;
        }
        let half = (r * r - dy * dy).sqrt();
        let x0 = (c - half).floor() as i32;
        let rect = cairo::RectangleInt::new(x0, y, ((c + half).ceil() as i32 - x0).max(1), 1);
        let _ = region.union_rectangle(&rect);
    }
    gdk_window.shape_combine_region(Some(&region), 0, 0);
    gdk_window.input_shape_combine_region(&region, 0, 0);
}
