[logic]
use crate::core::icon::icon;
use crate::core::theme::NordTheme;
use crate::shared::battery;

// Low/critical levels flag with the theme's warning colors; otherwise the icon takes the container
// foreground (`fg`), so the single glyph still signals a low battery at a glance.
fn level_color(level: i32, fg: Color) -> Color {
    let t = use_theme::<NordTheme>();
    if level <= 15 {
        t.red
    } else if level <= 30 {
        t.yellow
    } else {
        fg
    }
}

let init = battery::read();
let level = signal(init.map(|b| b.level).unwrap_or(0));
let charging = signal(init.map(|b| b.charging).unwrap_or(false));
let level_tint = level.read_only();
let charging_glyph = charging.read_only();
let fg = crate::module_fg();
// Subscribe to UPower change events (sub-second on plug/unplug), no-op headless where the seed stands.
platform_layershell::watch(
    move |tx| battery::stream(tx),
    move |b| {
        level.set(b.level);
        charging.set(b.charging);
    },
);
// The glyph name is reactive; `svg src:icon($glyph)` re-resolves it so the icon swaps battery ↔ charging.
let glyph = memo(move || if charging_glyph.get() { "battery-charging" } else { "battery" });

[view]
svg src:icon($glyph) tint:level_color($level_tint, $fg) width:icon_px() height:icon_px()
