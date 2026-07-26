[logic]
use crate::shared::glyph;
use crate::shared::services::network::{self, Network};

let state = signal(network::read());
let read = state.read_only();
let fg = crate::module_fg();
platform_layershell::watch(network::subscribe, move |net: Network| state.set(net));

let icon = crate::icon_view(
    move || glyph::network(read.get()).to_string(),
    move || fg.get(),
    icon_px(),
)?;

[view]
widget "icon"
