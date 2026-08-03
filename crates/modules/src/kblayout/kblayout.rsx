[logic]
use ::config::theme::{FontRole, NordTheme};
use ::services::hyprland::{self, KeyboardLayout};

// Hyprland names layouts in full ("English (US)"); a bar has room for the two-letter code people actually scan
// for, so this takes the initials of the leading words — "English (US)" → "EN", "Spanish" → "ES".
fn short_name(layout: &KeyboardLayout) -> String {
    let base = layout.name.split('(').next().unwrap_or(&layout.name).trim();
    let code: String = base
        .split_whitespace()
        .next()
        .unwrap_or(base)
        .chars()
        .filter(|c| c.is_alphabetic())
        .take(2)
        .collect();
    code.to_uppercase()
}

let initial = hyprland::socket_dir()
    .and_then(|dir| hyprland::keyboard_layout(&dir))
    .unwrap_or_default();

let code = signal(short_name(&initial));
let code_view = code.read_only();

platform_layershell::watch(
    hyprland::subscribe_keyboard,
    move |layout: KeyboardLayout| {
        code.set(short_name(&layout));
    },
);

let fg = ui::module::module_fg();
let body = use_theme::<NordTheme>().font(FontRole::Body);

[view]
text "{$code_view}" size:body color:$fg

[preview "Kblayout" fixture:ui::preview::bar_chip]
kblayout
