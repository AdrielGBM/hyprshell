[logic]
use ::config::theme::NordTheme;
use ::services::battery;
use ::ui::glyph;

let init = battery::read();
let level = signal(init.map(|b| b.level).unwrap_or(0));
let charging = signal(init.map(|b| b.charging).unwrap_or(false));
let level_tint = level.read_only();
let charging_tint = charging.read_only();
let charging_glyph = charging.read_only();
let fg = ui::module::module_fg();
let theme = use_theme::<NordTheme>();
// Subscribe to the single shared battery source (UPower sub-second on plug/unplug), no-op headless.
platform_layershell::watch(battery::subscribe, move |b| {
    level.set(b.level);
    charging.set(b.charging);
});

let icon = ui::icon::icon_view(
    move || glyph::battery(charging_glyph.get()).to_string(),
    move || glyph::battery_tint(level_tint.get(), charging_tint.get(), theme, fg.get()),
    ui::module::icon_px(),
)?;

[view]
widget "icon"
