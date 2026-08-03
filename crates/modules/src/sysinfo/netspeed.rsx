[logic]
use ::config::theme::{FontRole, NordTheme};
use ::services::netspeed::{self, NetSpeed, format_rate};

let initial = netspeed::current().unwrap_or_default();
let down = signal(format_rate(initial.down));
let up = signal(format_rate(initial.up));
let down_view = down.read_only();
let up_view = up.read_only();

platform_layershell::watch(netspeed::subscribe, move |speed: NetSpeed| {
    down.set(format_rate(speed.down));
    up.set(format_rate(speed.up));
});

let fg = ui::module::module_fg();
let fg_down = fg.clone();
let fg_up = fg.clone();
let caption = use_theme::<NordTheme>().font(FontRole::Caption);
// Half-height arrows stacked in the chip: two rates need two lines to stay readable at bar size, and the
// direction glyph says which is which without a label.
let arrow_size = (ui::module::icon_px() * 0.55).round();
let down_icon = ui::icon::icon_view(
    || "arrow-down".to_string(),
    move || fg_down.get(),
    arrow_size,
)?;
let up_icon = ui::icon::icon_view(|| "arrow-up".to_string(), move || fg_up.get(), arrow_size)?;

[view]
col justify:center gap:1
    row align:center gap:4
        widget "down_icon"
        text "{$down_view}" size:caption color:$fg
    row align:center gap:4
        widget "up_icon"
        text "{$up_view}" size:caption color:$fg

[preview "Netspeed"]
netspeed
