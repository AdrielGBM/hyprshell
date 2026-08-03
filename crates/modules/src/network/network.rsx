[logic]
use ::services::network::{self, Network};
use ::ui::glyph;

let state = signal(network::read());
let read = state.read_only();
let fg = ui::module::module_fg();
platform_layershell::watch(network::subscribe, move |net: Network| state.set(net));

[view]
icon_glyph name(move || glyph::network(read.get()).to_string()) tint(move || fg.get()) size:(ui::module::icon_px())

[preview "Network" fixture:ui::preview::bar_chip]
network
