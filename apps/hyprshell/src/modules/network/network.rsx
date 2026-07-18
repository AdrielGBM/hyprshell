[logic]
use std::time::Duration;

use crate::shared::services::network::{self, Network, NetworkKind};

// The single glyph reads the connection at a glance: a wired port, an off symbol when down, or a Wi-Fi arc whose fill tracks the signal strength.
fn net_glyph(net: Network) -> &'static str {
    match net.kind {
        NetworkKind::Ethernet => "ethernet-port",
        NetworkKind::Disconnected => "wifi-off",
        NetworkKind::Wifi => match net.signal {
            s if s >= 70 => "wifi",
            s if s >= 45 => "wifi-high",
            s if s >= 20 => "wifi-low",
            _ => "wifi-zero",
        },
    }
}

let glyph = signal(net_glyph(network::read()).to_string());
let glyph_read = glyph.read_only();
let fg = crate::module_fg();
platform_layershell::interval(Duration::from_secs(5), move || {
    glyph.set(net_glyph(network::read()).to_string());
});

let icon = crate::icon_view(move || glyph_read.get(), move || fg.get(), icon_px())?;

[view]
widget "icon"
