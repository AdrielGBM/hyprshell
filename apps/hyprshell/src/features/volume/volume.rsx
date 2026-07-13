[logic]
use std::time::Duration;

use crate::core::icon::icon;
use crate::shared::volume;

fn vol_glyph(muted: bool, level: i32) -> &'static str {
    if muted || level == 0 {
        "volume-x"
    } else if level < 50 {
        "volume-1"
    } else {
        "volume-2"
    }
}

// Seed from a real read; poll on the surface loop (no-op headless). The container wires the click that
// toggles mute and pops the OSD (where the exact level lives).
let init = volume::read();
let level = signal(init.map(|v| v.level).unwrap_or(0));
let muted = signal(init.map(|v| v.muted).unwrap_or(false));
let level_glyph = level.read_only();
let muted_glyph = muted.read_only();
let fg = crate::module_fg();
platform_layershell::interval(Duration::from_secs(3), move || {
    if let Some(v) = volume::read() {
        level.set(v.level);
        muted.set(v.muted);
    }
});

let glyph = memo(move || vol_glyph(muted_glyph.get(), level_glyph.get()));

[view]
svg src:icon($glyph) tint:$fg width:icon_px() height:icon_px()
