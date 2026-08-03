//! Volume, the mixer, the visualiser and what is playing.
//!
//! One `*_section` per form on the page, each owning one `[toml]` table and saving it on its own.

use telar::{LayoutError, LayoutItem, LayoutStyle, RwSignal, Text, box_item, signal};

use crate::form::*;
use config::theme::{FontRole, NordTheme};
use config::{AudioConfig, LyricsConfig, MediaConfig, VisualiserConfig};

pub(crate) fn media_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let m = &config.media;
    let preferred = signal(m.preferred_player.clone());
    let max_chars = signal(m.max_chars.to_string());
    let scroll = signal(media_scroll_str(m.scroll).to_string());
    let marquee = signal(m.marquee);
    let marquee_speed = signal(m.marquee_speed_ms.to_string());
    let seek_seconds = signal(m.seek_seconds.to_string());
    let visualiser = signal(m.visualiser);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.preferred_player"),
            preferred.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_chars"),
            max_chars.clone(),
            "40",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.scroll"),
            scroll.clone(),
            MEDIA_SCROLLS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.seek_seconds"),
            seek_seconds.clone(),
            "5",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.marquee"),
            marquee.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.marquee_speed_ms"),
            marquee_speed.clone(),
            "220",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.cover_visualiser"),
            visualiser.clone(),
            theme,
        )?,
    ];

    // Aliases are map-valued, so they stay hand-edited in the TOML for now, like `theme.colors`; carrying the
    // existing map through means saving this section does not silently drop them.
    let base = m.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.media"),
        move || {
            let value = MediaConfig {
                preferred_player: preferred.peek(),
                max_chars: parse_u32(&max_chars.peek(), base.max_chars),
                scroll: parse_media_scroll(&scroll.peek()),
                marquee: marquee.peek(),
                marquee_speed_ms: parse_u32(&marquee_speed.peek(), base.marquee_speed_ms),
                seek_seconds: parse_u32(&seek_seconds.peek(), base.seek_seconds),
                visualiser: visualiser.peek(),
                aliases: base.aliases.clone(),
            };
            persist(&path, "media", &value);
        },
    )?;
    section(|| telar::t!("settings.section.media"), rows, save, theme)
}

pub(crate) fn lyrics_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let l = &config.lyrics;
    let enabled = signal(l.enabled);
    let online = signal(l.online);

    // The folder is `[paths] lyrics`, edited with the other paths rather than duplicated here.
    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.lyrics_online"),
            online.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.lyrics"),
        move || {
            let value = LyricsConfig {
                enabled: enabled.peek(),
                online: online.peek(),
            };
            persist(&path, "lyrics", &value);
        },
    )?;
    section(|| telar::t!("settings.section.lyrics"), rows, save, theme)
}

pub(crate) fn audio_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let a = config.audio;
    let increment = signal(a.increment.to_string());
    let max_volume = signal(a.max_volume.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.increment"),
            increment.clone(),
            "5",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_volume"),
            max_volume.clone(),
            "150",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.audio"),
        move || {
            let value = AudioConfig {
                increment: parse_i32(&increment.peek(), a.increment),
                max_volume: parse_i32(&max_volume.peek(), a.max_volume),
            };
            persist(&path, "audio", &value);
        },
    )?;
    section(|| telar::t!("settings.section.audio"), rows, save, theme)
}

pub(crate) fn visualiser_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let v = config.visualiser;
    let bars = signal(v.bars.to_string());
    let smoothing = signal(v.smoothing.to_string());
    let floor_db = signal(v.floor_db.to_string());
    let gain = signal(v.gain.to_string());
    let beat = signal(v.beat_sensitivity.to_string());
    let frame_rate = signal(v.frame_rate.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.visualiser_bars"),
            bars.clone(),
            "48",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.smoothing"),
            smoothing.clone(),
            "0.6",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.floor_db"),
            floor_db.clone(),
            "-60",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.gain"),
            gain.clone(),
            "1",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.beat_sensitivity"),
            beat.clone(),
            "1.35",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.frame_rate"),
            frame_rate.clone(),
            "60",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.visualiser"),
        move || {
            let value = VisualiserConfig {
                bars: parse_u32(&bars.peek(), v.bars),
                smoothing: parse_f32(&smoothing.peek(), v.smoothing),
                floor_db: parse_f32(&floor_db.peek(), v.floor_db),
                gain: parse_f32(&gain.peek(), v.gain),
                beat_sensitivity: parse_f32(&beat.peek(), v.beat_sensitivity),
                frame_rate: parse_u32(&frame_rate.peek(), v.frame_rate),
            };
            persist(&path, "visualiser", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.visualiser"),
        rows,
        save,
        theme,
    )
}

/// Every media player a `[media.aliases]` row should exist for: the ones seen on the bus this session, plus
/// any the config already renames. Both halves matter, for the reason `monitor_keys` documents.
fn player_keys(configured: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = configured.keys().cloned().collect();
    if let Some(player) = services::mpris::current()
        && !player.identity.trim().is_empty()
        && !keys.contains(&player.identity)
    {
        keys.push(player.identity.clone());
    }
    keys.sort_unstable();
    keys
}

pub(crate) fn media_aliases_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let keys = player_keys(&config.media.aliases);
    let fields: Vec<(String, RwSignal<String>)> = keys
        .iter()
        .map(|key| {
            (
                key.clone(),
                signal(config.media.aliases.get(key).cloned().unwrap_or_default()),
            )
        })
        .collect();

    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(fields.len().max(1));
    if fields.is_empty() {
        rows.push(box_item(Text::auto(
            || telar::t!("settings.media.no_players"),
            LayoutStyle::new(),
            move || theme.text_style(FontRole::Caption, theme.muted),
        )?));
    }
    for (key, value) in &fields {
        let label = key.clone();
        rows.push(text_field(
            move || label.clone(),
            value.clone(),
            key,
            theme,
        )?);
    }

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.media_aliases"),
        move || {
            let aliases: std::collections::HashMap<String, String> = fields
                .iter()
                .filter_map(|(key, value)| {
                    opt_string(&value.peek()).map(|alias| (key.clone(), alias))
                })
                .collect();
            persist_with(&path, "media", |current| MediaConfig {
                aliases,
                ..current.media.clone()
            });
        },
    )?;
    section(
        || telar::t!("settings.section.media_aliases"),
        rows,
        save,
        theme,
    )
}
