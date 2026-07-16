[logic]
use std::time::Duration;

use crate::shared::services::brightness;

// A dimmer sun below 40%, a full sun above — so the single glyph still reads the level at a glance.
fn bright_glyph(level: i32) -> &'static str {
    if level < 40 { "sun-dim" } else { "sun" }
}

let level = signal(brightness::read().unwrap_or(0));
let level_glyph = level.read_only();
let fg = crate::module_fg();
platform_layershell::interval(Duration::from_secs(5), move || {
    if let Some(b) = brightness::read() {
        level.set(b);
    }
});

let glyph = memo(move || bright_glyph(level_glyph.get()));
let icon = crate::icon_view(move || glyph.get().to_string(), move || fg.get(), icon_px())?;

[view]
widget "icon"
