[logic]
use crate::form::{parse_i32, persist, source};
use ::config::BrightnessConfig;

let (config, path) = source();
let b = config.brightness;
let increment = signal(b.increment.to_string());
let external = signal(b.external);

let save: Box<dyn Fn()> = Box::new({
    let (increment, external) = (increment.clone(), external.clone());
    move || {
        let value = BrightnessConfig {
            increment: parse_i32(&increment.peek(), b.increment),
            external: external.peek(),
        };
        persist(&path, "brightness", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.brightness"))
    text_row label(|| telar::t!("settings.field.increment")) value:$increment placeholder:"5"
    toggle_row label(|| telar::t!("settings.field.external_monitors")) value:$external
    save_row label(|| telar::t!("settings.save.brightness")) on_press(save)
