[logic]
use crate::shared::services::resources::{self, Resources};
use crate::shared::theme::{FontRole, NordTheme};

fn heat_color(celsius: Option<f32>, fg: Color) -> Color {
    let t = use_theme::<NordTheme>();
    match celsius {
        Some(c) if c >= 85.0 => t.red,
        Some(c) if c >= 70.0 => t.yellow,
        _ => fg,
    }
}

// A machine with no hwmon (a VM, some ARM boards) has nothing to show; the chip renders a dash rather than a
// misleading 0 °C.
fn heat_text(celsius: Option<f32>) -> String {
    match celsius {
        Some(c) => format!("{c:.0}°"),
        None => rsx::t!("sysinfo.no_reading"),
    }
}

let initial = resources::current().unwrap_or_default();
let temp = signal(initial.temperature);
let temp_text = temp.read_only();
let temp_tint = temp.read_only();

platform_layershell::watch(resources::subscribe, move |r: Resources| {
    temp.set(r.temperature)
});

let fg = crate::module_fg();
let fg_tint = fg.clone();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let reading = memo(move || heat_text(temp_text.get()));
let icon = crate::icon_view(
    || "thermometer".to_string(),
    move || heat_color(temp_tint.get(), fg_tint.get()),
    icon_px(),
)?;

[view]
row align:center gap:6
    widget "icon"
    text "{$reading}" size:body color:$fg
