//! Notifications, the toasts that announce them, and the sidebar that collects them.
//!
//! One `*_section` per form on the page, each owning one `[toml]` table and saving it on its own.


use telar::{LayoutError, LayoutItem, signal};

use crate::form::*;
use config::theme::NordTheme;
use config::{NotificationsConfig, SidebarConfig, ToastEvents, ToastsConfig};

pub(crate) fn notifications_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let n = &config.notifications;
    let edge = signal(edge_str(n.edge).to_string());
    let align = signal(align_str(n.align).to_string());
    let max_visible = signal(n.max_visible.to_string());
    let timeout = signal(n.timeout_ms.to_string());
    let critical = signal(n.critical_sticky);
    let width = signal(n.width.to_string());
    let gap = signal(n.gap.to_string());
    let fullscreen = signal(fullscreen_popups_str(n.fullscreen).to_string());
    let group_by_app = signal(n.group_by_app);
    let group_preview = signal(n.group_preview_num.to_string());
    let action_on_click = signal(n.action_on_click);
    let body_lines = signal(n.body_lines.to_string());
    let open_expanded = signal(n.open_expanded);
    let sound = signal(n.sound.clone());
    let clear_threshold = signal(n.clear_threshold.to_string());

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_visible"),
            max_visible.clone(),
            "4",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "5000",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.critical_sticky"),
            critical.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "380",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "10", theme)?,
        enum_field(
            || telar::t!("settings.field.fullscreen_popups"),
            fullscreen.clone(),
            FULLSCREEN_POPUPS,
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.group_by_app"),
            group_by_app.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.group_preview_num"),
            group_preview.clone(),
            "3",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.action_on_click"),
            action_on_click.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.body_lines"),
            body_lines.clone(),
            "4",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.open_expanded"),
            open_expanded.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.sound"),
            sound.clone(),
            "canberra-gtk-play -i message",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.clear_threshold"),
            clear_threshold.clone(),
            "0.35",
            theme,
        )?,
    ];

    let base = n.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.notifications"),
        move || {
            let value = NotificationsConfig {
                edge: parse_edge(&edge.peek()),
                align: parse_align(&align.peek()),
                max_visible: parse_u32(&max_visible.peek(), base.max_visible),
                timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
                critical_sticky: critical.peek(),
                width: parse_f32(&width.peek(), base.width),
                gap: parse_f32(&gap.peek(), base.gap),
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
        },
    )?;
    section(
        || telar::t!("settings.section.notifications"),
        rows,
        save,
        theme,
    )
}

/// `[toasts]`, including the per-event switches.
///
/// The event matrix is a nested table (`[toasts.events]`) with a fixed set of keys, so it is edited here rather
/// than left to the TOML — the same reason `background.monitors` came off the map-editing list: the keys are
/// enumerable, so the panel can name them all.
pub(crate) fn toasts_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let t = &config.toasts;
    let enabled = signal(t.enabled);
    let edge = signal(edge_str(t.edge).to_string());
    let align = signal(align_str(t.align).to_string());
    let max_toasts = signal(t.max_toasts.to_string());
    let timeout = signal(t.timeout_ms.to_string());
    let width = signal(t.width.to_string());
    let gap = signal(t.gap.to_string());

    let events = t.events;
    let config_loaded = signal(events.config_loaded);
    let charging = signal(events.charging);
    let game_mode = signal(events.game_mode);
    let dnd = signal(events.dnd);
    let audio_output = signal(events.audio_output);
    let audio_input = signal(events.audio_input);
    let lock_keys = signal(events.lock_keys);
    let kb_layout = signal(events.kb_layout);
    let vpn = signal(events.vpn);
    let now_playing = signal(events.now_playing);
    let screenshot = signal(events.screenshot);
    let recording = signal(events.recording);

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_toasts"),
            max_toasts.clone(),
            "3",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "2500",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "300",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "8", theme)?,
        subheader(|| telar::t!("settings.subheader.events"), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_config_loaded"),
            config_loaded.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_charging"),
            charging.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_game_mode"),
            game_mode.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.event_dnd"), dnd.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_audio_output"),
            audio_output.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_audio_input"),
            audio_input.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_lock_keys"),
            lock_keys.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_kb_layout"),
            kb_layout.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.event_vpn"), vpn.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_now_playing"),
            now_playing.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_screenshot"),
            screenshot.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_recording"),
            recording.clone(),
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.toasts"),
        move || {
            let value = ToastsConfig {
                enabled: enabled.peek(),
                edge: parse_edge(&edge.peek()),
                align: parse_align(&align.peek()),
                max_toasts: parse_u32(&max_toasts.peek(), base.max_toasts),
                timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
                width: parse_f32(&width.peek(), base.width),
                gap: parse_f32(&gap.peek(), base.gap),
                events: ToastEvents {
                    config_loaded: config_loaded.peek(),
                    charging: charging.peek(),
                    game_mode: game_mode.peek(),
                    dnd: dnd.peek(),
                    audio_output: audio_output.peek(),
                    audio_input: audio_input.peek(),
                    lock_keys: lock_keys.peek(),
                    kb_layout: kb_layout.peek(),
                    vpn: vpn.peek(),
                    now_playing: now_playing.peek(),
                    screenshot: screenshot.peek(),
                    recording: recording.peek(),
                },
            };
            persist(&path, "toasts", &value);
        },
    )?;
    section(|| telar::t!("settings.section.toasts"), rows, save, theme)
}

pub(crate) fn sidebar_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let s = &config.sidebar;
    let edge = signal(edge_str(s.edge).to_string());
    let size = signal(s.size.to_string());
    let show_toggles = signal(s.show_toggles);
    let show_history = signal(s.show_history);

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.size"),
            size.clone(),
            "400",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_toggles"),
            show_toggles.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_history"),
            show_history.clone(),
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.sidebar"),
        move || {
            let value = SidebarConfig {
                edge: parse_edge(&edge.peek()),
                size: parse_u32(&size.peek(), base.size),
                show_toggles: show_toggles.peek(),
                show_history: show_history.peek(),
            };
            persist(&path, "sidebar", &value);
        },
    )?;
    section(|| telar::t!("settings.section.sidebar"), rows, save, theme)
}
