[logic]
use crate::shared::glyph;
use crate::shared::services::volume::{self, Volume};

let state = signal(volume::current_mic().unwrap_or(Volume { level: 0, muted: true }));
let read = state.read_only();
let fg = crate::module_fg();

platform_layershell::watch(volume::subscribe_mic, move |mic: Volume| state.set(mic));

let icon = crate::icon_view(
    move || glyph::microphone(read.get()).to_string(),
    move || fg.get(),
    icon_px(),
)?;

[view]
widget "icon"
