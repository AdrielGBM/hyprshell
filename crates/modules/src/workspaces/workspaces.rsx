[logic]
use crate::workspaces::{Pill, PillStyle, grid, pills};
use ::config::theme::NordTheme;
use ::services::hyprland::{self, Snapshot};

// Resolving the socket dir per click keeps the handler capture-free (it takes only the workspace id).
fn focus(id: i32) {
    if let Some(dir) = hyprland::socket_dir() {
        hyprland::focus_workspace(&dir, id);
    }
}

let env = ui::module::surface_env();
let config = env
    .as_ref()
    .map(|e| e.config.workspaces.clone())
    .unwrap_or_default();
let output = env.as_ref().and_then(|e| e.output.clone());

let occupied_background = config.occupied_background;
let indicator = config.indicator;

let list = signal(Vec::<Pill>::new());
let items = list.read_only();
// Subscribe to the single shared workspaces source; the consumer writes the signal on this surface's thread.
platform_layershell::watch(hyprland::subscribe, move |snap: Snapshot| {
    list.set(pills(&snap, &config, output.as_deref()));
});

let style = PillStyle {
    theme: use_theme::<NordTheme>(),
    // Pills round like the sibling chips instead of a fixed radius, so they follow the theme/`[shape]` radius.
    radius: ui::module::chip_radius(),
    // A stretched horizontal chip can't derive its width from its height, so size both sides to make a square.
    side: ui::module::bar_thickness(),
    vertical: ui::module::bar_is_vertical(),
    occupied_background,
    indicator,
};
// Built in Rust: the indicator has to read the active pill's laid-out rect and paint itself from it, and the
// view DSL reaches neither layout nodes nor a canvas.
let row = grid(items, style, focus)?;

[view]
widget "row"

[preview "Workspaces"]
workspaces
