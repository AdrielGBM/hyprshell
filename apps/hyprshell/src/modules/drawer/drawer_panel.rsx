[logic]
use crate::modules::drawer::{
    content_radius, current_drawer_config, current_drawer_module, module_panel, panel_fill,
};

let drawer = current_drawer_config();
let dw = drawer.width;
let dmh = drawer.max_height;
let rad = content_radius();
let module = current_drawer_module();
// The module's panel content, dispatched in Rust and embedded below with `widget`.
let content = module_panel(&module)?;
// The box below fills with `panel_fill()` rather than the `surface` token: it is that token at the configured `[panels] opacity`, so a compositor `layer_rule = blur, hyprshell-drawer` has something to show through.
[view]
box width:dw pad:16 fill:panel_fill() radius:rad
    scroll width:100% height:dmh
        widget "content"
