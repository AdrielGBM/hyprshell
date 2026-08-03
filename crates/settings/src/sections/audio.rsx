[logic]
use crate::form::{parse_i32, persist, source};
use ::config::AudioConfig;

let (config, path) = source();
let a = config.audio;
let increment = signal(a.increment.to_string());
let max_volume = signal(a.max_volume.to_string());

let save: Box<dyn Fn()> = Box::new({
    let (increment, max_volume) = (increment.clone(), max_volume.clone());
    move || {
        let value = AudioConfig {
            increment: parse_i32(&increment.peek(), a.increment),
            max_volume: parse_i32(&max_volume.peek(), a.max_volume),
        };
        persist(&path, "audio", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.audio"))
    text_row label(|| telar::t!("settings.field.increment")) value:$increment placeholder:"5"
    text_row label(|| telar::t!("settings.field.max_volume")) value:$max_volume placeholder:"150"
    save_row label(|| telar::t!("settings.save.audio")) on_press(save)
