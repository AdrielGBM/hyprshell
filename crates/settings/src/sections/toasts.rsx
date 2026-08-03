[logic]
use crate::form::{
    ALIGNS, EDGES, align_str, edge_str, parse_align, parse_edge, parse_f32, parse_u32,
    parse_u64, persist, source,
};
use ::config::theme::FontRole;
use ::config::{ToastEvents, ToastsConfig};

let (config, path) = source();
let t = &config.toasts;
let base = t.clone();
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

let save: Box<dyn Fn()> = Box::new({
    let (enabled, edge, align) = (enabled.clone(), edge.clone(), align.clone());
    let (max_toasts, timeout, width, gap) = (
        max_toasts.clone(),
        timeout.clone(),
        width.clone(),
        gap.clone(),
    );
    let (config_loaded, charging, game_mode, dnd) = (
        config_loaded.clone(),
        charging.clone(),
        game_mode.clone(),
        dnd.clone(),
    );
    let (audio_output, audio_input, lock_keys, kb_layout) = (
        audio_output.clone(),
        audio_input.clone(),
        lock_keys.clone(),
        kb_layout.clone(),
    );
    let (vpn, now_playing, screenshot, recording) = (
        vpn.clone(),
        now_playing.clone(),
        screenshot.clone(),
        recording.clone(),
    );
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
    }
});

[view]
form_section title(|| telar::t!("settings.section.toasts"))
    toggle_row label(|| telar::t!("settings.field.enabled")) value:$enabled
    enum_row label(|| telar::t!("settings.field.edge")) value:$edge options:EDGES
    enum_row label(|| telar::t!("settings.field.align")) value:$align options:ALIGNS
    text_row label(|| telar::t!("settings.field.max_toasts")) value:$max_toasts placeholder:"3"
    text_row label(|| telar::t!("settings.field.timeout_ms")) value:$timeout placeholder:"2500"
    text_row label(|| telar::t!("settings.field.width")) value:$width placeholder:"300"
    text_row label(|| telar::t!("settings.field.gap")) value:$gap placeholder:"8"
    text "{telar::t!(\"settings.subheader.events\")}" color:muted size:theme.font(FontRole::Caption) weight:700
    toggle_row label(|| telar::t!("settings.field.event_config_loaded")) value:$config_loaded
    toggle_row label(|| telar::t!("settings.field.event_charging")) value:$charging
    toggle_row label(|| telar::t!("settings.field.event_game_mode")) value:$game_mode
    toggle_row label(|| telar::t!("settings.field.event_dnd")) value:$dnd
    toggle_row label(|| telar::t!("settings.field.event_audio_output")) value:$audio_output
    toggle_row label(|| telar::t!("settings.field.event_audio_input")) value:$audio_input
    toggle_row label(|| telar::t!("settings.field.event_lock_keys")) value:$lock_keys
    toggle_row label(|| telar::t!("settings.field.event_kb_layout")) value:$kb_layout
    toggle_row label(|| telar::t!("settings.field.event_vpn")) value:$vpn
    toggle_row label(|| telar::t!("settings.field.event_now_playing")) value:$now_playing
    toggle_row label(|| telar::t!("settings.field.event_screenshot")) value:$screenshot
    toggle_row label(|| telar::t!("settings.field.event_recording")) value:$recording
    save_row label(|| telar::t!("settings.save.toasts")) on_press(save)
