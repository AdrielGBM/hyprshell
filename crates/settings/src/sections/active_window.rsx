[logic]
use crate::form::{persist, source};
use ::config::ActiveWindowConfig;

let (config, path) = source();
let w = config.active_window;
let compact = signal(w.compact);
let show_icon = signal(w.show_icon);
let inverted = signal(w.inverted);

let save: Box<dyn Fn()> = Box::new({
    let (compact, show_icon) = (compact.clone(), show_icon.clone());
    let inverted = inverted.clone();
    move || {
        let value = ActiveWindowConfig {
            compact: compact.peek(),
            show_icon: show_icon.peek(),
            inverted: inverted.peek(),
        };
        persist(&path, "active_window", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.active_window"))
    toggle_row label(|| telar::t!("settings.field.compact")) value:$compact
    toggle_row label(|| telar::t!("settings.field.show_icon")) value:$show_icon
    toggle_row label(|| telar::t!("settings.field.inverted")) value:$inverted
    save_row label(|| telar::t!("settings.save.active_window")) on_press(save)
