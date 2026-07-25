[logic]
use crate::shared::services::volume;

fn vol_glyph(muted: bool, level: i32) -> &'static str {
    if muted || level == 0 {
        "volume-x"
    } else if level < 50 {
        "volume-1"
    } else {
        "volume-2"
    }
}

// The container wires the click that toggles mute and pops the OSD (where the exact level lives).
let level = signal(0);
let muted = signal(false);
let level_glyph = level.read_only();
let muted_glyph = muted.read_only();
let fg = crate::module_fg();
platform_layershell::watch(volume::subscribe, move |v: volume::Volume| {
    level.set(v.level);
    muted.set(v.muted);
});

let glyph = memo(move || vol_glyph(muted_glyph.get(), level_glyph.get()));
let icon = crate::icon_view(move || glyph.get().to_string(), move || fg.get(), icon_px())?;

[view]
widget "icon"
