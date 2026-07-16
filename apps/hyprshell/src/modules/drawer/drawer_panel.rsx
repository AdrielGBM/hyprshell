[logic]
use crate::modules::drawer::{current_drawer_config, current_drawer_module, module_panel};

let drawer = current_drawer_config();
let dw = drawer.width;
let dmh = drawer.max_height;
let module = current_drawer_module();
// The module's panel content, dispatched in Rust and embedded below with `widget`.
let content = module_panel(&module)?;

[view]
box width:dw pad:16 fill:surface radius:14
    scroll width:100% height:dmh
        widget "content"
