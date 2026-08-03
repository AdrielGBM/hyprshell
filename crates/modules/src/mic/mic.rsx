[logic]
use ::services::volume::{self, Volume};
use ::ui::glyph;

let state = signal(volume::current_mic().unwrap_or(Volume {
    level: 0,
    muted: true,
}));
let read = state.read_only();
let fg = ui::module::module_fg();
let icon = ui::module::icon_px();

platform_layershell::watch(volume::subscribe_mic, move |mic: Volume| state.set(mic));

[view]
icon_glyph name(move || glyph::microphone(read.get()).to_string()) tint(move || fg.get()) size:(icon)

[preview "Mic" fixture:ui::preview::bar_chip]
mic
