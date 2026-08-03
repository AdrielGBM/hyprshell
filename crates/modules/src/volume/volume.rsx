[logic]
use ::services::volume;
use ::ui::glyph;

// The container wires the click that toggles mute and pops the OSD (where the exact level lives).
let state = signal(volume::current().unwrap_or(volume::Volume {
    level: 0,
    muted: false,
}));
let read = state.read_only();
let fg = ui::module::module_fg();
platform_layershell::watch(volume::subscribe, move |v: volume::Volume| state.set(v));

let icon = ui::icon::icon_view(
    move || glyph::volume(read.get()).to_string(),
    move || fg.get(),
    ui::module::icon_px(),
)?;

[view]
widget "icon"

[preview "Volume"]
volume
