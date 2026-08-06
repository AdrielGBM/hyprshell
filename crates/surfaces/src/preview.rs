//! What this crate's previews need before they build, and the ones it registers in Rust.
//!
//! Each of these is a *surface*, so each declares the size the compositor would give it and is rendered under
//! the same root the runner mounts — see [`telar::PreviewSurface`]. What is still the surface host's alone is
//! the clear colour behind the tree and the window's own transparency.

use std::sync::Arc;

use telar::{PreviewEntry, PreviewSurface};

use config::{Config, DrawerConfig};

/// The previews this crate registers by hand, for the surfaces whose content is still built by a Rust function.
/// The drawer is not among them: its panel is `drawer_panel.rsx`, so its preview is a `[preview]` block there.
pub fn entries() -> Vec<PreviewEntry> {
    let config = config::config().unwrap_or_else(|| Arc::new(Config::starter()));
    let popouts = config.popouts;
    vec![
        PreviewEntry {
            component_name: "bar",
            preview_name: "Configured bar",
            build: crate::bar::preview,
            // A screen's width would not fit the page; what a bar has to be given exactly is its thickness,
            // which is the axis every chip on it sizes itself against.
            surface: Some(PreviewSurface::new(
                940.0,
                config.bars.get(ui::module::bar_edge()).size as f32,
            )),
        },
        PreviewEntry {
            component_name: "float",
            preview_name: "Window frame",
            build: crate::float::frame_preview,
            // The one preview that animates: a float fades in from transparent, and a transition that never
            // settles is a window the user cannot see while every other check passes.
            surface: Some(PreviewSurface::new(360.0, 240.0).animated()),
        },
        PreviewEntry {
            component_name: "wallpaper",
            preview_name: "Desktop",
            build: crate::wallpaper::preview,
            surface: Some(PreviewSurface::new(880.0, 495.0)),
        },
        PreviewEntry {
            component_name: "popout",
            preview_name: "Hover card",
            build: crate::popout::preview,
            surface: Some(PreviewSurface::new(
                popouts.card_width(),
                popouts.card_height(),
            )),
        },
    ]
}

/// Which module the drawer is showing, which is decided by the chip that opened it and so has to be put in
/// scope before the panel builds. `clock` because it needs nothing from the machine to draw.
pub fn drawer() {
    ui::preview::bar_chip();
    crate::drawer::set_drawer_ctx("clock".to_string(), DrawerConfig::default());
}
