[logic]
use crate::form::{persist, source};
use ::config::GpuConfig;

let (config, path) = source();
let g = &config.gpu;
let enabled = signal(g.enabled);
let backend = signal(g.backend.clone());
let card = signal(g.card.clone());

let save: Box<dyn Fn()> = Box::new({
    let (enabled, backend, card) = (enabled.clone(), backend.clone(), card.clone());
    move || {
        let value = GpuConfig {
            enabled: enabled.peek(),
            backend: backend.peek(),
            card: card.peek(),
        };
        persist(&path, "gpu", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.gpu"))
    toggle_row label(|| telar::t!("settings.field.enabled")) value:$enabled
    text_row label(|| telar::t!("settings.field.backend")) value:$backend placeholder:"auto"
    text_row label(|| telar::t!("settings.field.card")) value:$card placeholder:"card1"
    save_row label(|| telar::t!("settings.save.gpu")) on_press(save)
