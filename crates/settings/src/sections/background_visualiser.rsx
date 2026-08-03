[logic]
use crate::form::{EDGES, parse_edge, parse_f32, parse_u32, persist, source};
use ::config::{BackgroundConfig, BackgroundVisualiserConfig};

let (config, path) = source();
let v = config.background.visualiser;
let base = config.background.clone();
let enabled = signal(v.enabled);
let edge = signal(v.edge.as_str().to_string());
let reach = signal(v.reach.to_string());
let gap = signal(v.gap.to_string());
let radius = signal(v.radius.to_string());
let opacity = signal(v.opacity.to_string());
let hide = signal(v.hide_when_silent);
let accent = signal(v.accent);
let margin = signal(v.margin.to_string());

let save: Box<dyn Fn()> = Box::new({
    let (enabled, edge, reach) = (enabled.clone(), edge.clone(), reach.clone());
    let (gap, radius, opacity) = (gap.clone(), radius.clone(), opacity.clone());
    let (hide, accent, margin) = (hide.clone(), accent.clone(), margin.clone());
    move || {
        let visualiser = BackgroundVisualiserConfig {
            enabled: enabled.peek(),
            edge: parse_edge(&edge.peek()),
            reach: parse_u32(&reach.peek(), base.visualiser.reach),
            gap: parse_f32(&gap.peek(), base.visualiser.gap),
            radius: parse_f32(&radius.peek(), base.visualiser.radius),
            opacity: parse_f32(&opacity.peek(), base.visualiser.opacity),
            hide_when_silent: hide.peek(),
            accent: accent.peek(),
            margin: parse_u32(&margin.peek(), base.visualiser.margin),
        };
        let value = BackgroundConfig {
            visualiser,
            ..base.clone()
        };
        persist(&path, "background", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.background_visualiser"))
    toggle_row label(|| telar::t!("settings.field.enabled")) value:$enabled
    enum_row label(|| telar::t!("settings.field.edge")) value:$edge options:EDGES
    text_row label(|| telar::t!("settings.field.reach")) value:$reach placeholder:"140"
    text_row label(|| telar::t!("settings.field.gap")) value:$gap placeholder:"3"
    text_row label(|| telar::t!("settings.field.radius")) value:$radius placeholder:"3"
    text_row label(|| telar::t!("settings.field.bar_opacity")) value:$opacity placeholder:"0.75"
    text_row label(|| telar::t!("settings.field.margin")) value:$margin placeholder:"0"
    toggle_row label(|| telar::t!("settings.field.hide_when_silent")) value:$hide
    toggle_row label(|| telar::t!("settings.field.accent")) value:$accent
    save_row label(|| telar::t!("settings.save.background_visualiser")) on_press(save)
