//! The settings application's previews, registered in Rust because its page is built by [`crate::panel`]
//! rather than by a `.rsx` component — see `docs/telar-rsx.md`, T-8.3, for the migration that would change that.

use telar::{LayoutError, LayoutItem, PreviewEntry, PreviewSurface};

/// The float this application opens in (`ui::placement::window`), so both previews are laid out at the size the
/// user actually reads them at.
const FLOAT: PreviewSurface = PreviewSurface {
    width: 920.0,
    height: 680.0,
    animate: false,
};

pub fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry {
            component_name: "settings",
            preview_name: "Settings panel",
            build: crate::panel::settings_panel,
            surface: Some(FLOAT),
        },
        PreviewEntry {
            component_name: "settings",
            preview_name: "A page of switches",
            build: switches_page,
            surface: Some(FLOAT),
        },
    ]
}

/// The panel opened on a page of toggles. The application's first page is text fields and colour swatches, so
/// previewing only that leaves every switch in the settings unseen — and they are the rows the catalogue's
/// `toggle` draws.
fn switches_page() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let at = crate::pages::PAGES
        .iter()
        .position(|page| page.label == "notifications")
        .unwrap_or(0);
    util::state::kept("settings.page", || telar::signal(0usize)).set(at);
    crate::panel::settings_panel()
}
