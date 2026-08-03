[logic]
use crate::form::{parse_u32, persist, source};
use ::config::ActiveWindowConfig;

let (config, path) = source();
let w = config.active_window;
let compact = signal(w.compact);
let show_icon = signal(w.show_icon);
let inverted = signal(w.inverted);
let max_chars = signal(w.max_chars.to_string());

let save: Box<dyn Fn()> = Box::new({
    let (compact, show_icon) = (compact.clone(), show_icon.clone());
    let (inverted, max_chars) = (inverted.clone(), max_chars.clone());
    move || {
        let value = ActiveWindowConfig {
            compact: compact.peek(),
            show_icon: show_icon.peek(),
            inverted: inverted.peek(),
            max_chars: parse_u32(&max_chars.peek(), w.max_chars),
        };
        persist(&path, "active_window", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.active_window"))
    toggle_row label(|| telar::t!("settings.field.compact")) value:$compact
    toggle_row label(|| telar::t!("settings.field.show_icon")) value:$show_icon
    toggle_row label(|| telar::t!("settings.field.inverted")) value:$inverted
    text_row label(|| telar::t!("settings.field.max_chars")) value:$max_chars placeholder:"300"
    save_row label(|| telar::t!("settings.save.active_window")) on_press(save)
