//! `[notifications]`, `[toasts]` and `[sidebar]`.
//!
//! One type per `[toml]` table, each with the defaults the shell falls back to. The doc comment on a
//! field is what `hyprshell config schema` prints for it, so it is written for a user reading the reference.

use serde::{Deserialize, Serialize};

use crate::sections::*;

/// Whether a popup still appears while a fullscreen window has focus. The three values escalate: `on` never
/// holds anything back, `off` holds back everything but `critical` (don't interrupt a game or a film unless it
/// matters), `never` holds back all of it. Suppression only affects the *popup* — the notification is recorded
/// and waits in the history either way, exactly as Do-Not-Disturb does.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FullscreenPopups {
    On,
    #[default]
    Off,
    Never,
}

impl FullscreenPopups {
    /// Whether a notification of `urgency` may still pop while a fullscreen window is focused.
    pub fn allows(self, urgency: crate::policy::Urgency) -> bool {
        use crate::policy::Urgency;
        match self {
            Self::On => true,
            Self::Off => urgency == Urgency::Critical,
            Self::Never => false,
        }
    }
}

/// Notification popups: where the stack anchors (defaults to top-right), how many show at once before the rest queue, the per-popup auto-dismiss (`0` = sticky), whether `critical` urgency ignores that timeout, and the card width. Popups follow the focused monitor.
///
/// The history panel's own behaviour lives here too, since it draws the same cards: `group_by_app` collapses an
/// application's notifications under one header with a count, a mute and a clear, showing `group_preview_num`
/// of them until the group is expanded; `action_on_click` makes tapping a card body invoke the notification's
/// `default` action rather than only dismissing it; `body_lines`/`open_expanded` bound (or release) how much of
/// a long body a card shows.
///
/// `sound` is a command run — detached, through `sh -c` — each time a notification actually pops. Empty is
/// silent, which is the default: a shell that started making noise on upgrade would be a bug, and the right
/// command is per-machine (`canberra-gtk-play -i message`, `paplay /usr/share/sounds/…`).
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct NotificationsConfig {
    pub edge: Edge,
    pub align: Align,
    pub max_visible: u32,
    pub timeout_ms: u64,
    pub critical_sticky: bool,
    pub width: f32,
    pub gap: f32,
    pub fullscreen: FullscreenPopups,
    pub group_by_app: bool,
    pub group_preview_num: u32,
    pub action_on_click: bool,
    /// Lines of body a card shows before ellipsizing. Ignored while `open_expanded` is on.
    pub body_lines: u32,
    pub open_expanded: bool,
    pub sound: String,
    /// How far sideways a card must be dragged before letting go dismisses it, as a fraction of its width.
    /// `0` switches the gesture off, which is what a touchpad user who keeps catching it wants.
    pub clear_threshold: f32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            align: Align::End,
            max_visible: 4,
            timeout_ms: 5000,
            critical_sticky: true,
            width: 380.0,
            gap: 10.0,
            fullscreen: FullscreenPopups::default(),
            group_by_app: true,
            group_preview_num: 3,
            action_on_click: true,
            body_lines: 4,
            open_expanded: false,
            sound: String::new(),
            clear_threshold: 0.35,
        }
    }
}

impl NotificationsConfig {
    /// How many of a group's cards show before it is expanded. At least one, so a cap of `0` collapses a group
    /// to its header instead of hiding the notifications behind a row that says nothing is there.
    pub fn group_preview(&self) -> usize {
        self.group_preview_num.max(1) as usize
    }

    /// The card's body cap, or `None` when `open_expanded` asks for the whole thing. Clamped so a `0` cannot
    /// render a card with no body at all.
    pub fn body_max_lines(&self) -> Option<u16> {
        (!self.open_expanded).then(|| self.body_lines.clamp(1, 100) as u16)
    }

    /// The command to run when a notification pops, if one is configured.
    pub fn sound_command(&self) -> Option<&str> {
        let command = self.sound.trim();
        (!command.is_empty()).then_some(command)
    }

    /// The swipe distance that dismisses a card, in px for a card `width` wide, or `None` when the gesture is
    /// off. Bounded below the full width: a threshold you cannot reach is a gesture that never fires, which
    /// reads as the card being stuck rather than as the setting being wrong.
    pub fn swipe_distance(&self, width: f32) -> Option<f32> {
        if !self.clear_threshold.is_finite() || self.clear_threshold <= 0.0 {
            return None;
        }
        Some(width * self.clear_threshold.min(0.9))
    }
}

/// Which in-shell toasts to show (`[toasts.events]`).
///
/// One switch per event rather than a single `enabled`, because the useful set is personal: the point of a toast
/// is that it tells you something you would otherwise miss, and a toast about something you already know is
/// noise. Every one is on by default except the two that fire most often.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct ToastEvents {
    pub config_loaded: bool,
    pub charging: bool,
    pub game_mode: bool,
    pub dnd: bool,
    pub audio_output: bool,
    pub audio_input: bool,
    /// Caps Lock and Num Lock — the one piece of state a keyboard changes and never reports.
    pub lock_keys: bool,
    pub kb_layout: bool,
    pub vpn: bool,
    /// Off by default: a music player already says what it is playing, and every skipped track would be a toast.
    pub now_playing: bool,
    pub screenshot: bool,
    pub recording: bool,
}

impl Default for ToastEvents {
    fn default() -> Self {
        Self {
            config_loaded: true,
            charging: true,
            game_mode: true,
            dnd: true,
            audio_output: true,
            audio_input: true,
            lock_keys: true,
            kb_layout: true,
            vpn: true,
            now_playing: false,
            screenshot: false,
            recording: true,
        }
    }
}

/// In-shell toasts (`[toasts]`): the transient messages the shell says about itself.
///
/// Not notifications. A notification belongs to an application, goes into history and waits under
/// Do-Not-Disturb; "Caps Lock is on" is feedback about a key that was just pressed and is worthless a second
/// later. See `shared::services::toaster`.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ToastsConfig {
    pub enabled: bool,
    pub edge: Edge,
    pub align: Align,
    pub max_toasts: u32,
    pub timeout_ms: u64,
    pub width: f32,
    pub gap: f32,
    pub events: ToastEvents,
}

impl Default for ToastsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            edge: Edge::Bottom,
            align: Align::Center,
            max_toasts: 3,
            timeout_ms: 2500,
            width: 300.0,
            gap: 8.0,
            events: ToastEvents::default(),
        }
    }
}

impl ToastsConfig {
    /// Whether `event` should reach the screen: toasts on at all, and this event not switched off.
    pub fn allows(&self, event: crate::policy::ToastEvent) -> bool {
        use crate::policy::ToastEvent as Event;
        self.enabled
            && match event {
                Event::ConfigLoaded => self.events.config_loaded,
                Event::Charging => self.events.charging,
                Event::GameMode => self.events.game_mode,
                Event::Dnd => self.events.dnd,
                Event::AudioOutput => self.events.audio_output,
                Event::AudioInput => self.events.audio_input,
                Event::LockKeys => self.events.lock_keys,
                Event::KbLayout => self.events.kb_layout,
                Event::Vpn => self.events.vpn,
                Event::NowPlaying => self.events.now_playing,
                Event::Screenshot => self.events.screenshot,
                Event::Recording => self.events.recording,
            }
    }

    /// How long a toast stays. Floored rather than allowed to be zero: a toast that expires on the frame it was
    /// posted is a feature that looks broken.
    pub fn lifetime(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.timeout_ms.clamp(400, 60_000))
    }

    pub fn visible(&self) -> usize {
        self.max_toasts.clamp(1, 10) as usize
    }
}

/// The notification centre (`[sidebar]`): a full-height surface that is the home for the notification history and
/// the quick toggles.
///
/// Distinct from the bell drawer, which is a glance: this is where a user goes to *deal with* what has arrived,
/// so it takes the whole edge, scrolls, and hosts the utilities panel's own toggles rather than a second set.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SidebarConfig {
    pub edge: Edge,
    /// Width for a left/right sidebar, height for a top/bottom one, in px.
    pub size: u32,
    pub show_toggles: bool,
    pub show_history: bool,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            size: 400,
            show_toggles: true,
            show_history: true,
        }
    }
}

impl SidebarConfig {
    /// Clamped so a hand-edited `size` cannot produce a sidebar too narrow to read or one that covers the screen.
    pub fn thickness(&self) -> u32 {
        self.size.clamp(240, 1200)
    }
}
