[logic]
use crate::modules::workspaces::{Pill, PillStyle, pill_view, pills};
use crate::shared::services::hyprland::{self, Snapshot};
use crate::shared::theme::NordTheme;

// Resolving the socket dir per click keeps the handler capture-free (it takes only the workspace id).
fn focus(id: i32) {
    if let Some(dir) = hyprland::socket_dir() {
        hyprland::focus_workspace(&dir, id);
    }
}

let env = crate::surface_env();
let config = env
    .as_ref()
    .map(|e| e.config.workspaces.clone())
    .unwrap_or_default();
let output = env.as_ref().and_then(|e| e.output.clone());

let occupied_background = config.occupied_background;

let list = signal(Vec::<Pill>::new());
let items = list.read_only();
// Subscribe to the single shared workspaces source; the consumer writes the signal on this surface's thread.
platform_layershell::watch(hyprland::subscribe, move |snap: Snapshot| {
    list.set(pills(&snap, &config, output.as_deref()));
});

let style = PillStyle {
    theme: use_theme::<NordTheme>(),
    // Pills round like the sibling chips instead of a fixed radius, so they follow the theme/`[shape]` radius.
    radius: crate::chip_radius(),
    // A stretched horizontal chip can't derive its width from its height, so size both sides to make a square.
    side: crate::bar_thickness(),
    vertical: crate::bar_is_vertical(),
    occupied_background,
};
let vertical = style.vertical;

[view]
if vertical
    col align:center
        for pill in $items key pill.key() gap:8
            build "pill_view(pill, style, focus)?"
else
    row align:center
        for pill in $items key pill.key() gap:8
            build "pill_view(pill, style, focus)?"
