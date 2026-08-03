//! The settings application's preview, registered in Rust because its page is built by [`crate::panel`] rather
//! than by a `.rsx` component — see `docs/telar-rsx.md`, T-8.3, for the migration that would change that.

use telar::PreviewEntry;

pub fn entries() -> Vec<PreviewEntry> {
    vec![PreviewEntry {
        component_name: "settings",
        preview_name: "Settings panel",
        build: crate::panel::settings_panel,
    }]
}
