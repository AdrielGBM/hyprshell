[logic]
use crate::activewindow::{compact_label, label};
use ::config::theme::{FontRole, NordTheme};
use ::services::hyprland::{self, ActiveWindow};

let env = ui::module::surface_env();
let config = env
    .as_ref()
    .map(|e| e.config.active_window)
    .unwrap_or_default();

fn text_for(window: &ActiveWindow, config: &::config::ActiveWindowConfig) -> String {
    if window.is_empty() {
        return telar::t!("activewindow.none");
    }
    if config.compact {
        compact_label(window, config)
    } else {
        label(window, config)
    }
}

let initial = hyprland::socket_dir()
    .map(|dir| hyprland::active_window(&dir))
    .unwrap_or_default();

let title = signal(text_for(&initial, &config));
let icon_name = signal(initial.class.clone());
let title_view = title.read_only();
let icon_view = icon_name.read_only();

platform_layershell::watch(
    hyprland::subscribe_active_window,
    move |window: ActiveWindow| {
        title.set(text_for(&window, &config));
        icon_name.set(window.class.clone());
    },
);

let fg = ui::module::module_fg();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let size = ui::module::icon_px();
// The app's own artwork, not a tinted glyph: the point of this chip is recognising the app at a glance. A class
// with no installed icon simply renders nothing, leaving the title to carry the chip.
let class = icon_view.get();
let show_icon = config.show_icon && ::ui::icon::app_icon_view(&class, size)?.is_some();
let leading = show_icon && !config.inverted;
let trailing = show_icon && config.inverted;

[view]
row align:center gap:8
    if leading
        build "crate::activewindow::icon_slot(&class, size)?"
    text "{$title_view}" size:body color:$fg
    if trailing
        build "crate::activewindow::icon_slot(&class, size)?"

[preview "Activewindow" fixture:ui::preview::bar_chip]
activewindow
