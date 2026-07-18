[logic]
use crate::shared::services::battery::{self, BatteryDetails, ChargeState};
use crate::shared::theme::{FontRole, NordTheme};

// The panel echoes the chip's colour language: warning tints at low charge, the theme's green while charging, otherwise the plain text colour.
fn level_color(level: i32, charging: bool, fg: Color) -> Color {
    let t = use_theme::<NordTheme>();
    if charging {
        t.green
    } else if level <= 15 {
        t.red
    } else if level <= 30 {
        t.yellow
    } else {
        fg
    }
}

fn duration_text(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn status_text(d: &BatteryDetails) -> String {
    match d.state {
        ChargeState::Charging => match duration_text(d.time_to_full).as_str() {
            "" => rsx::t!("battery.charging"),
            t => rsx::t!("battery.until_full", time = t),
        },
        ChargeState::Discharging => match duration_text(d.time_to_empty).as_str() {
            "" => rsx::t!("battery.on_battery"),
            t => rsx::t!("battery.remaining", time = t),
        },
        ChargeState::Full => rsx::t!("battery.full"),
        ChargeState::Empty => rsx::t!("battery.empty"),
        ChargeState::Pending => rsx::t!("battery.pending"),
        ChargeState::Unknown => rsx::t!("battery.unknown"),
    }
}

fn rate_text(d: &BatteryDetails) -> String {
    if d.energy_rate <= 0.0 {
        return String::new();
    }
    format!("{:.1} W", d.energy_rate)
}

let theme = use_theme::<NordTheme>();
let fg = theme.text;
let init = battery::details();

let level = signal(init.map(|d| d.level).unwrap_or(0));
let charging = signal(init.map(|d| d.state.is_charging()).unwrap_or(false));
let status = signal(init.map(|d| status_text(&d)).unwrap_or_else(|| "No battery".to_string()));
let rate = signal(init.map(|d| rate_text(&d)).unwrap_or_default());

let level_pct = level.read_only();
let level_tint = level.read_only();
let charging_glyph = charging.read_only();
let charging_tint = charging.read_only();
let status_view = status.read_only();
let rate_view = rate.read_only();

platform_layershell::watch(
    move |tx| battery::stream_details(tx),
    move |d| {
        level.set(d.level);
        charging.set(d.state.is_charging());
        status.set(status_text(&d));
        rate.set(rate_text(&d));
    },
);

let pct = memo(move || format!("{}%", level_pct.get()));
let glyph = memo(move || {
    if charging_glyph.get() {
        "battery-charging".to_string()
    } else {
        "battery".to_string()
    }
});
let icon = crate::icon_view(
    move || glyph.get(),
    move || level_color(level_tint.get(), charging_tint.get(), fg),
    44.0,
)?;

let display = theme.font(FontRole::Display);
let body = theme.font(FontRole::Body);
let caption = theme.font(FontRole::Caption);

[view]
col align:center gap:6
    widget "icon"
    text "{$pct}" size:display color:text align:center
    text "{$status_view}" size:body color:subtle align:center
    text "{$rate_view}" size:caption color:muted align:center
