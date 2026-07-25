[logic]
use crate::shared::services::volume::{self, Volume};

// Mute is the state that matters at a glance, so it wins over the level; below that the glyph tracks how far
// the input is turned down, matching how the volume chip reads.
fn mic_glyph(mic: Volume) -> &'static str {
    if mic.muted {
        "mic-off"
    } else if mic.level == 0 {
        "mic-off"
    } else {
        "mic"
    }
}

let initial = volume::read_mic().unwrap_or(Volume { level: 0, muted: true });
let glyph = signal(mic_glyph(initial).to_string());
let glyph_read = glyph.read_only();
let fg = crate::module_fg();

platform_layershell::watch(volume::subscribe_mic, move |mic: Volume| {
    glyph.set(mic_glyph(mic).to_string());
});

let icon = crate::icon_view(move || glyph_read.get(), move || fg.get(), icon_px())?;

[view]
widget "icon"
