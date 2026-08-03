[logic]
use crate::form::record_field;
use ::config::theme::NordTheme;

/// A labelled switch, bound to the signal a section seeds from `config.toml`. The switch itself is the
/// catalogue's, so it looks like every other one in the shell.
pub struct Props {
    pub label: Box<dyn Fn() -> String> = Box::new(String::new),
    pub value: RwSignal<bool> = signal(false),
}

let value = props.value;
record_field(&value);
let label = props.label;
let accent = use_theme::<NordTheme>().accent;
let switch = telar::toggle(telar::ToggleProps {
    checked: Some(value),
    color: Box::new(move || accent),
    ..Default::default()
})?;

[view]
field_row label(move || label())
    widget "switch"
