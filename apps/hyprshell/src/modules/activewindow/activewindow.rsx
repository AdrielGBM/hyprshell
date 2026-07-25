[logic]
use crate::modules::activewindow::{compact_label, label};
use crate::shared::services::hyprland::{self, ActiveWindow};
use crate::shared::theme::{FontRole, NordTheme};

let env = crate::surface_env();
let config = env
    .as_ref()
    .map(|e| e.config.active_window)
    .unwrap_or_default();

fn text_for(window: &ActiveWindow, config: &crate::core::config::ActiveWindowConfig) -> String {
    if window.is_empty() {
        return rsx::t!("activewindow.none");
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

platform_layershell::watch(hyprland::subscribe_active_window, move |window: ActiveWindow| {
    title.set(text_for(&window, &config));
    icon_name.set(window.class.clone());
});

let fg = crate::module_fg();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let size = crate::icon_px();
// The app's own artwork, not a tinted glyph: the point of this chip is recognising the app at a glance. A class
// with no installed icon simply renders nothing, leaving the title to carry the chip.
let app_icon = crate::shared::icon::app_icon_view(&icon_view.get(), size)?;
let show_icon = config.show_icon && app_icon.is_some();
let icon = app_icon.unwrap_or(rsx::box_item(rsx::Container::new(
    rsx::LayoutStyle::new(),
    vec![],
)?));

[view]
row align:center gap:8
    if show_icon
        widget "icon"
    text "{$title_view}" size:body color:$fg
