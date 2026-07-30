[logic]
use crate::core::config::TemperatureConfig;
use crate::shared::services::resources::{self, Resources};
use crate::shared::theme::{FontRole, NordTheme};

fn heat_color(celsius: Option<f32>, config: &TemperatureConfig, fg: Color) -> Color {
    let t = use_theme::<NordTheme>();
    match celsius {
        Some(c) if c >= config.critical => t.red,
        Some(c) if c >= config.warn => t.yellow,
        _ => fg,
    }
}

// A machine with no hwmon (a VM, some ARM boards) has nothing to show; the chip renders a dash rather than a
// misleading 0 °C.
fn heat_text(celsius: Option<f32>, config: &TemperatureConfig) -> String {
    match celsius {
        Some(c) => config.unit.format(c),
        None => telar::t!("sysinfo.no_reading"),
    }
}

let config = crate::surface_env()
    .map(|env| env.config.temperature.clone())
    .unwrap_or_default();
let text_config = config.clone();
let tint_config = config.clone();
let sensor = config.sensor.clone();

let initial = resources::current().unwrap_or_default();
let temp = signal(initial.temperature_of(&config.sensor));
let temp_text = temp.read_only();
let temp_tint = temp.read_only();

platform_layershell::watch(resources::subscribe, move |r: Resources| {
    temp.set(r.temperature_of(&sensor))
});

let fg = crate::module_fg();
let fg_tint = fg.clone();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let reading = memo(move || heat_text(temp_text.get(), &text_config));
let icon = crate::icon_view(
    || "thermometer".to_string(),
    move || heat_color(temp_tint.get(), &tint_config, fg_tint.get()),
    icon_px(),
)?;

[view]
row align:center gap:6
    widget "icon"
    text "{$reading}" size:body color:$fg
