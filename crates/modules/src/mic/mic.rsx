[logic]
use ::services::volume::{self, Volume};
use ::ui::glyph;

let state = signal(volume::current_mic().unwrap_or(Volume {
    level: 0,
    muted: true,
}));
let read = state.read_only();
let fg = ui::module::module_fg();

platform_layershell::watch(volume::subscribe_mic, move |mic: Volume| state.set(mic));

let icon = ui::icon::icon_view(
    move || glyph::microphone(read.get()).to_string(),
    move || fg.get(),
    ui::module::icon_px(),
)?;

[view]
widget "icon"
