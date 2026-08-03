[logic]
use ::services::network::{self, Network};
use ::ui::glyph;

let state = signal(network::read());
let read = state.read_only();
let fg = ui::module::module_fg();
platform_layershell::watch(network::subscribe, move |net: Network| state.set(net));

let icon = ui::icon::icon_view(
    move || glyph::network(read.get()).to_string(),
    move || fg.get(),
    ui::module::icon_px(),
)?;

[view]
widget "icon"

[preview "Network"]
network
