[logic]
use crate::shared::services::resources::{self, Resources};
use crate::shared::theme::{FontRole, NordTheme};

// The chip tints as load climbs, so a busy machine is visible without reading the number.
fn load_color(percent: f32, fg: Color) -> Color {
    let t = use_theme::<NordTheme>();
    if percent >= 90.0 {
        t.red
    } else if percent >= 70.0 {
        t.yellow
    } else {
        fg
    }
}

let initial = resources::current().unwrap_or_default();
let load = signal(initial.cpu);
let load_text = load.read_only();
let load_tint = load.read_only();

platform_layershell::watch(resources::subscribe, move |r: Resources| load.set(r.cpu));

let fg = crate::module_fg();
let fg_tint = fg.clone();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let percent = memo(move || format!("{:.0}%", load_text.get()));
let icon = crate::icon_view(
    || "cpu".to_string(),
    move || load_color(load_tint.get(), fg_tint.get()),
    icon_px(),
)?;

[view]
row align:center gap:6
    widget "icon"
    text "{$percent}" size:body color:$fg
