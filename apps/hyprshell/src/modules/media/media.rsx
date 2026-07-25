[logic]
use crate::modules::media::{glyph, label};
use crate::shared::services::mpris::{self, Player};
use crate::shared::theme::{FontRole, NordTheme};

let config = crate::surface_env()
    .map(|e| e.config.media.clone())
    .unwrap_or_default();
let for_update = config.clone();

let initial = mpris::current().unwrap_or_default();
let text = signal(label(&initial, &config));
let icon_name = signal(glyph(&initial).to_string());
let text_view = text.read_only();
let text_empty = text.read_only();
let icon_view = icon_name.read_only();
// A vertical bar has no room for a track title, so it shows only the transport glyph; the same module works on
// every edge instead of needing a second one.
let vertical = crate::bar_is_vertical();

platform_layershell::watch(mpris::subscribe, move |player: Player| {
    text.set(label(&player, &for_update));
    icon_name.set(glyph(&player).to_string());
});

let fg = crate::module_fg();
let fg_icon = fg.clone();
let body = use_theme::<NordTheme>().font(FontRole::Body);
let icon = crate::icon_view(move || icon_view.get(), move || fg_icon.get(), icon_px())?;
let show_text = memo(move || !vertical && !text_empty.get().is_empty());

[view]
row align:center gap:6
    widget "icon"
    if $show_text
        text "{$text_view}" size:body color:$fg
