[logic]
use crate::form::{WEEKDAYS, join_csv, parse_u64, persist, source, split_csv};
use ::config::DashboardConfig;

let (config, path) = source();
let d = &config.dashboard;
let base = d.clone();
let tabs = signal(join_csv(&d.tabs));
let media = signal(d.media_update_interval.to_string());
let resources = signal(d.resource_update_interval.to_string());
let first_day = signal(d.first_day_of_week.clone());
let avatar = signal(d.avatar.clone());

let save: Box<dyn Fn()> = Box::new({
    let (tabs, media, resources) = (tabs.clone(), media.clone(), resources.clone());
    let (first_day, avatar) = (first_day.clone(), avatar.clone());
    move || {
        let value = DashboardConfig {
            tabs: split_csv(&tabs.peek()),
            media_update_interval: parse_u64(&media.peek(), base.media_update_interval),
            resource_update_interval: parse_u64(
                &resources.peek(),
                base.resource_update_interval,
            ),
            first_day_of_week: first_day.peek(),
            avatar: avatar.peek(),
        };
        persist(&path, "dashboard", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.dashboard"))
    text_row label(|| telar::t!("settings.field.tabs")) value:$tabs placeholder:"dash, media, performance, weather"
    text_row label(|| telar::t!("settings.field.media_update_interval")) value:$media placeholder:"500"
    text_row label(|| telar::t!("settings.field.resource_update_interval")) value:$resources placeholder:"1000"
    enum_row label(|| telar::t!("settings.field.first_day_of_week")) value:$first_day options:WEEKDAYS
    text_row label(|| telar::t!("settings.field.avatar")) value:$avatar placeholder:"~/.face"
    save_row label(|| telar::t!("settings.save.dashboard")) on_press(save)
