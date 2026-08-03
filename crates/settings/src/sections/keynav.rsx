[logic]
use crate::form::{persist, source};
use ::config::KeyNavConfig;

let (config, path) = source();
let vim = signal(config.keynav.vim);

let save: Box<dyn Fn()> = Box::new({
    let vim = vim.clone();
    move || persist(&path, "keynav", &KeyNavConfig { vim: vim.peek() })
});

[view]
form_section title(|| telar::t!("settings.section.keynav"))
    toggle_row label(|| telar::t!("settings.field.vim")) value:$vim
    save_row label(|| telar::t!("settings.save.keynav")) on_press(save)
