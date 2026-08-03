//! What this crate's previews need before they build, and the ones it registers in Rust.
//!
//! A surface is not a component: what the compositor mounts is an `App` — a clear colour, a window config and a
//! scaffold around a tree. A `[preview]` renders the tree, which is the half a person looks at; the chrome
//! around it belongs to the surface host and no preview can show it.

use telar::PreviewEntry;

use config::DrawerConfig;

/// The previews this crate registers by hand, for the surfaces whose content is still built by a Rust function.
/// The drawer is not among them: its panel is `drawer_panel.rsx`, so its preview is a `[preview]` block there.
pub fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry {
            component_name: "bar",
            preview_name: "Configured bar",
            build: crate::bar::preview,
        },
        PreviewEntry {
            component_name: "float",
            preview_name: "Window frame",
            build: crate::float::frame_preview,
        },
        PreviewEntry {
            component_name: "wallpaper",
            preview_name: "Desktop",
            build: crate::wallpaper::preview,
        },
        PreviewEntry {
            component_name: "popout",
            preview_name: "Hover card",
            build: crate::popout::preview,
        },
    ]
}

/// Which module the drawer is showing, which is decided by the chip that opened it and so has to be put in
/// scope before the panel builds. `clock` because it needs nothing from the machine to draw.
pub fn drawer() {
    ui::preview::bar_chip();
    crate::drawer::set_drawer_ctx("clock".to_string(), DrawerConfig::default(), 14.0);
}
