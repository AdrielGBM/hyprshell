[logic]
use crate::form::{
    FULLSCREEN_POPUPS, fullscreen_popups_str, parse_f32, parse_fullscreen_popups, parse_u32,
    persist, source,
};
use ::config::NotificationsConfig;

let (config, path) = source();
let n = &config.notifications;
let base = n.clone();
let critical = signal(n.critical_sticky);
let fullscreen = signal(fullscreen_popups_str(n.fullscreen).to_string());
let group_by_app = signal(n.group_by_app);
let group_preview = signal(n.group_preview_num.to_string());
let action_on_click = signal(n.action_on_click);
let body_lines = signal(n.body_lines.to_string());
let open_expanded = signal(n.open_expanded);
let sound = signal(n.sound.clone());
let clear_threshold = signal(n.clear_threshold.to_string());

let save: Box<dyn Fn()> = Box::new({
    let fullscreen = fullscreen.clone();
    let (critical, group_by_app, action_on_click, open_expanded) = (
        critical.clone(),
        group_by_app.clone(),
        action_on_click.clone(),
        open_expanded.clone(),
    );
    let (group_preview, body_lines, sound, clear_threshold) = (
        group_preview.clone(),
        body_lines.clone(),
        sound.clone(),
        clear_threshold.clone(),
    );
    move || {
        let value = NotificationsConfig {

            critical_sticky: critical.peek(),

            fullscreen: parse_fullscreen_popups(&fullscreen.peek()),
            group_by_app: group_by_app.peek(),
            group_preview_num: parse_u32(&group_preview.peek(), base.group_preview_num),
            action_on_click: action_on_click.peek(),
            body_lines: parse_u32(&body_lines.peek(), base.body_lines),
            open_expanded: open_expanded.peek(),
            sound: sound.peek(),
            clear_threshold: parse_f32(&clear_threshold.peek(), base.clear_threshold),
        };
        persist(&path, "notifications", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.notifications"))
    toggle_row label(|| telar::t!("settings.field.critical_sticky")) value:$critical
    enum_row label(|| telar::t!("settings.field.fullscreen_popups")) value:$fullscreen options:FULLSCREEN_POPUPS
    toggle_row label(|| telar::t!("settings.field.group_by_app")) value:$group_by_app
    text_row label(|| telar::t!("settings.field.group_preview_num")) value:$group_preview placeholder:"3"
    toggle_row label(|| telar::t!("settings.field.action_on_click")) value:$action_on_click
    text_row label(|| telar::t!("settings.field.body_lines")) value:$body_lines placeholder:"4"
    toggle_row label(|| telar::t!("settings.field.open_expanded")) value:$open_expanded
    text_row label(|| telar::t!("settings.field.sound")) value:$sound placeholder:"canberra-gtk-play -i message"
    text_row label(|| telar::t!("settings.field.clear_threshold")) value:$clear_threshold placeholder:"0.35"
    save_row label(|| telar::t!("settings.save.notifications")) on_press(save)
