//! What the shell reads off the machine under it, and how it is told to.
//!
//! One `*_section` per form on the page, each owning one `[toml]` table and saving it on its own.

use std::path::PathBuf;

use telar::{
    Container, LayoutError, LayoutItem, LayoutStyle, RectStyle, RwSignal, SizeDimension,
    StyledContainer, Text, box_item, signal,
};

use crate::form::*;
use config::theme::{FontRole, NordTheme};
use config::{
    AppsConfig, BluetoothConfig, BrightnessConfig, Config, DashboardConfig, GeneralConfig,
    GpuConfig, KeyNavConfig, NetworkConfig, PathsConfig, RecorderConfig, ScreenshotConfig,
    UtilitiesConfig, WeatherConfig,
};

pub(crate) fn general_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let lang = signal(telar::current_locale().unwrap_or_else(|| config.language()));
    let over_fullscreen = signal(config.general.show_over_fullscreen);
    let logo = signal(config.general.logo.clone());
    let apps = config.general.apps.clone();
    // `[general.apps] terminal` is the field's home now; a config still carrying the older top-level key is
    // seeded from it, so editing here moves the value rather than appearing to lose it.
    let terminal = signal(if apps.terminal.trim().is_empty() {
        config.general.terminal.clone()
    } else {
        apps.terminal.clone()
    });
    let file_manager = signal(apps.file_manager.clone());
    let audio_mixer = signal(apps.audio_mixer.clone());
    let media_player = signal(apps.media_player.clone());
    let browser = signal(apps.browser.clone());
    let editor = signal(apps.editor.clone());

    let rows = vec![
        language_field(|| telar::t!("settings.field.language"), lang.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.show_over_fullscreen"),
            over_fullscreen.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.logo"),
            logo.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.terminal"),
            terminal.clone(),
            "xterm",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.file_manager"),
            file_manager.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.audio_mixer"),
            audio_mixer.clone(),
            "pavucontrol",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.media_player"),
            media_player.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.browser"),
            browser.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.editor"),
            editor.clone(),
            "xdg-open",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let legacy_terminal = config.general.terminal.clone();
    let save = save_button(
        || telar::t!("settings.save.general"),
        move || {
            persist(
                &path,
                "general",
                &GeneralConfig {
                    language: lang.peek(),
                    show_over_fullscreen: over_fullscreen.peek(),
                    logo: logo.peek(),
                    terminal: legacy_terminal.clone(),
                    apps: AppsConfig {
                        terminal: terminal.peek(),
                        file_manager: file_manager.peek(),
                        audio_mixer: audio_mixer.peek(),
                        media_player: media_player.peek(),
                        browser: browser.peek(),
                        editor: editor.peek(),
                    },
                },
            );
        },
    )?;
    section(|| telar::t!("settings.section.general"), rows, save, theme)
}

/// A cycle control over UI languages: shows the current one's native name; each press advances to the next code
/// and broadcasts the new locale to every surface via [`services::locale::set`].
fn language_field(
    label: impl Fn() -> String + 'static,
    value: RwSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value_text = value.read_only();
    let text = Text::auto(
        move || language_name(&value_text.get()),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Body, theme.text),
    )?;
    let control = StyledContainer::new(
        LayoutStyle::new()
            .flex_grow(1.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(text)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || {
        let current = value.peek();
        let index = LANGUAGES.iter().position(|o| *o == current).unwrap_or(0);
        let next = LANGUAGES[(index + 1) % LANGUAGES.len()].to_string();
        value.set(next.clone());
        services::locale::set(next);
    });
    labelled(label, Box::new(control), theme)
}

fn language_name(code: &str) -> String {
    match code {
        "en" => "English".to_string(),
        "es" => "Español".to_string(),
        other => other.to_uppercase(),
    }
}

pub(crate) fn brightness_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let b = config.brightness;
    let increment = signal(b.increment.to_string());
    let external = signal(b.external);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.increment"),
            increment.clone(),
            "5",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.external_monitors"),
            external.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.brightness"),
        move || {
            let value = BrightnessConfig {
                increment: parse_i32(&increment.peek(), b.increment),
                external: external.peek(),
            };
            persist(&path, "brightness", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.brightness"),
        rows,
        save,
        theme,
    )
}

pub(crate) fn gpu_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let g = &config.gpu;
    let enabled = signal(g.enabled);
    let backend = signal(g.backend.clone());
    let card = signal(g.card.clone());

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.card"),
            card.clone(),
            "card1",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.gpu"),
        move || {
            let value = GpuConfig {
                enabled: enabled.peek(),
                backend: backend.peek(),
                card: card.peek(),
            };
            persist(&path, "gpu", &value);
        },
    )?;
    section(|| telar::t!("settings.section.gpu"), rows, save, theme)
}

pub(crate) fn weather_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let w = &config.weather;
    let enabled = signal(w.enabled);
    let location = signal(w.location.clone());
    let latitude = signal(w.latitude.map(|v| v.to_string()).unwrap_or_default());
    let longitude = signal(w.longitude.map(|v| v.to_string()).unwrap_or_default());
    let refresh = signal(w.refresh_minutes.to_string());
    let days = signal(w.forecast_days.to_string());

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.location"),
            location.clone(),
            "Madrid",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.latitude"),
            latitude.clone(),
            "40.4168",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.longitude"),
            longitude.clone(),
            "-3.7038",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.refresh_minutes"),
            refresh.clone(),
            "15",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.forecast_days"),
            days.clone(),
            "7",
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.weather"),
        move || {
            // A blank coordinate is "not set", not zero: a stray empty field must fall back to the place name
            // rather than pinning the forecast to the Gulf of Guinea.
            let optional = |raw: String| raw.trim().parse::<f32>().ok();
            let value = WeatherConfig {
                enabled: enabled.peek(),
                location: location.peek(),
                latitude: optional(latitude.peek()),
                longitude: optional(longitude.peek()),
                refresh_minutes: parse_u32(&refresh.peek(), base.refresh_minutes),
                forecast_days: parse_u32(&days.peek(), base.forecast_days),
            };
            persist(&path, "weather", &value);
        },
    )?;
    section(|| telar::t!("settings.section.weather"), rows, save, theme)
}

pub(crate) fn dashboard_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let d = &config.dashboard;
    let tabs = signal(join_csv(&d.tabs));
    let media = signal(d.media_update_interval.to_string());
    let resources = signal(d.resource_update_interval.to_string());
    let first_day = signal(d.first_day_of_week.clone());
    let avatar = signal(d.avatar.clone());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.tabs"),
            tabs.clone(),
            "dash, media, performance, weather",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.media_update_interval"),
            media.clone(),
            "500",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.resource_update_interval"),
            resources.clone(),
            "1000",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.first_day_of_week"),
            first_day.clone(),
            WEEKDAYS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.avatar"),
            avatar.clone(),
            "~/.face",
            theme,
        )?,
    ];

    let base = d.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.dashboard"),
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
        },
    )?;
    section(
        || telar::t!("settings.section.dashboard"),
        rows,
        save,
        theme,
    )
}

pub(crate) fn paths_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let p = &config.paths;
    let wallpapers = signal(p.wallpapers.clone());
    let lyrics = signal(p.lyrics.clone());
    let recordings = signal(p.recordings.clone());
    let screenshots = signal(p.screenshots.clone());
    let assets = signal(p.assets.clone());

    // Each placeholder is the directory the shell would use if the field is left empty, resolved against this
    // machine — so the form shows where things actually land rather than a generic example.
    let show = |dir: PathBuf| dir.to_string_lossy().into_owned();
    let rows = vec![
        text_field(
            || telar::t!("settings.field.wallpapers"),
            wallpapers.clone(),
            &show(config.wallpaper_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.lyrics"),
            lyrics.clone(),
            &show(config.lyrics_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.recordings"),
            recordings.clone(),
            &show(config.recordings_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.screenshots"),
            screenshots.clone(),
            &show(config.screenshot_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.assets"),
            assets.clone(),
            "",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.paths"),
        move || {
            let value = PathsConfig {
                wallpapers: wallpapers.peek(),
                lyrics: lyrics.peek(),
                recordings: recordings.peek(),
                screenshots: screenshots.peek(),
                assets: assets.peek(),
            };
            persist(&path, "paths", &value);
        },
    )?;
    section(|| telar::t!("settings.section.paths"), rows, save, theme)
}

pub(crate) fn network_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let n = config.network;
    let enabled = signal(n.enabled);
    let rescan = signal(n.rescan_seconds.to_string());
    let max_networks = signal(n.max_networks.to_string());
    let show_hidden = signal(n.show_hidden);

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.rescan_seconds"),
            rescan.clone(),
            "300",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_networks"),
            max_networks.clone(),
            "20",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_hidden"),
            show_hidden.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.network"),
        move || {
            let value = NetworkConfig {
                enabled: enabled.peek(),
                rescan_seconds: parse_u32(&rescan.peek(), n.rescan_seconds),
                max_networks: parse_u32(&max_networks.peek(), n.max_networks),
                show_hidden: show_hidden.peek(),
            };
            persist(&path, "network", &value);
        },
    )?;
    section(|| telar::t!("settings.section.network"), rows, save, theme)
}

pub(crate) fn bluetooth_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let b = config.bluetooth;
    let enabled = signal(b.enabled);
    let scan_on_open = signal(b.scan_on_open);
    let max_devices = signal(b.max_devices.to_string());
    let show_unnamed = signal(b.show_unnamed);

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.scan_on_open"),
            scan_on_open.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_devices"),
            max_devices.clone(),
            "12",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_unnamed"),
            show_unnamed.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.bluetooth"),
        move || {
            let value = BluetoothConfig {
                enabled: enabled.peek(),
                scan_on_open: scan_on_open.peek(),
                max_devices: parse_u32(&max_devices.peek(), b.max_devices),
                show_unnamed: show_unnamed.peek(),
            };
            persist(&path, "bluetooth", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.bluetooth"),
        rows,
        save,
        theme,
    )
}

pub(crate) fn screenshot_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let s = &config.screenshot;
    let copy = signal(s.copy);
    let save_to_disk = signal(s.save);
    let cursor = signal(s.include_cursor);
    let freeze = signal(s.freeze);
    let notify = signal(s.notify);
    let backend = signal(s.backend.clone());
    let file_name = signal(s.file_name.clone());
    let annotator = signal(s.annotator.clone());

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.copy"), copy.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.save"),
            save_to_disk.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.include_cursor"),
            cursor.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.freeze"), freeze.clone(), theme)?,
        toggle_field(|| telar::t!("settings.field.notify"), notify.clone(), theme)?,
        enum_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            SHOT_BACKENDS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.file_name"),
            file_name.clone(),
            "screenshot_%Y-%m-%d_%H-%M-%S",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.annotator"),
            annotator.clone(),
            "satty --filename {file}",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.screenshot"),
        move || {
            let value = ScreenshotConfig {
                copy: copy.peek(),
                save: save_to_disk.peek(),
                include_cursor: cursor.peek(),
                freeze: freeze.peek(),
                notify: notify.peek(),
                backend: backend.peek(),
                file_name: file_name.peek(),
                annotator: annotator.peek(),
            };
            persist(&path, "screenshot", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.screenshot"),
        rows,
        save,
        theme,
    )
}

pub(crate) fn recorder_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let r = &config.recorder;
    let backend = signal(r.backend.clone());
    let audio = signal(r.audio);
    let device = signal(r.audio_device.clone());
    let fps = signal(r.fps.to_string());
    let file_name = signal(r.file_name.clone());
    let notify = signal(r.notify);
    let max_entries = signal(r.max_entries.to_string());

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            RECORDER_BACKENDS,
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.audio"), audio.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.audio_device"),
            device.clone(),
            "default_output",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.fps"), fps.clone(), "60", theme)?,
        text_field(
            || telar::t!("settings.field.file_name"),
            file_name.clone(),
            "recording_%Y-%m-%d_%H-%M-%S",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.notify"), notify.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.max_entries"),
            max_entries.clone(),
            "12",
            theme,
        )?,
    ];

    let base = r.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.recorder"),
        move || {
            let value = RecorderConfig {
                backend: backend.peek(),
                audio: audio.peek(),
                audio_device: device.peek(),
                fps: parse_u32(&fps.peek(), base.fps),
                file_name: file_name.peek(),
                notify: notify.peek(),
                max_entries: parse_u32(&max_entries.peek(), base.max_entries),
            };
            persist(&path, "recorder", &value);
        },
    )?;
    section(|| telar::t!("settings.section.recorder"), rows, save, theme)
}

pub(crate) fn utilities_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let u = &config.utilities;
    let toggles = signal(join_csv(&u.toggles));
    let show_capture = signal(u.show_capture);
    let show_recordings = signal(u.show_recordings);
    let columns = signal(u.columns.to_string());
    let preview = signal(u.window_preview_ms.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.toggles"),
            toggles.clone(),
            "wifi, bluetooth, mic, dnd",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_capture"),
            show_capture.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_recordings"),
            show_recordings.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.columns"),
            columns.clone(),
            "4",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.window_preview_ms"),
            preview.clone(),
            "1000",
            theme,
        )?,
    ];

    let base = u.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.utilities"),
        move || {
            let value = UtilitiesConfig {
                toggles: split_csv(&toggles.peek()),
                show_capture: show_capture.peek(),
                show_recordings: show_recordings.peek(),
                columns: parse_u32(&columns.peek(), base.columns),
                window_preview_ms: parse_u64(&preview.peek(), base.window_preview_ms),
            };
            persist(&path, "utilities", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.utilities"),
        rows,
        save,
        theme,
    )
}

pub(crate) fn keynav_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let vim = signal(config.keynav.vim);
    let rows = vec![toggle_field(
        || telar::t!("settings.field.vim"),
        vim.clone(),
        theme,
    )?];
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.keynav"),
        move || {
            persist(&path, "keynav", &KeyNavConfig { vim: vim.peek() });
        },
    )?;
    section(|| telar::t!("settings.section.keynav"), rows, save, theme)
}

/// K12: what this shell is and what it found to talk to.
///
/// Readings, not fields — so it has no Save. The compositor and session lines are what a bug report needs
/// first and what a user otherwise has to leave the shell to find.
pub(crate) fn about_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = telar::use_theme::<NordTheme>();
    let rows = vec![
        reading_row(
            || telar::t!("settings.field.version"),
            env!("CARGO_PKG_VERSION"),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.compositor"),
            &env_or_unknown("HYPRLAND_INSTANCE_SIGNATURE").map_or_else(
                || telar::t!("settings.about.not_hyprland"),
                |_| "Hyprland".to_string(),
            ),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.session"),
            &env_or_unknown("XDG_SESSION_TYPE").unwrap_or_else(|| telar::t!("common.unknown")),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.config_file"),
            &Config::default_path().display().to_string(),
            theme,
        )?,
    ];
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        std::iter::once(section_label(
            || telar::t!("settings.section.about"),
            theme,
        )?)
        .chain(rows)
        .collect(),
    )?;
    Ok(Box::new(column))
}

/// A non-empty environment variable, which is the only kind worth reporting.
fn env_or_unknown(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// A label and a value the user cannot change — the About page's only row shape.
fn reading_row(
    label: impl Fn() -> String + 'static,
    value: &str,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = value.to_string();
    let text = Text::auto(
        move || value.clone(),
        LayoutStyle::new().flex_grow(1.0),
        move || theme.text_style(FontRole::Body, theme.text),
    )?;
    labelled(label, Box::new(text), theme)
}
