use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use platform_layershell::EventSender;
use serde::{Deserialize, Serialize};
use zbus::zvariant::Value;

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
/// Most recent notifications kept in the persisted history, so the file stays bounded.
const MAX_HISTORY: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    fn from_hint(byte: u8) -> Self {
        match byte {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// A notification's own image (from the `image-data`/`icon_data` hint), decoded to RGBA8. Runtime-only — not
/// persisted (raw pixels would bloat the history file), so a restored notification falls back to its dot.
#[derive(Clone, Debug)]
pub struct NotificationImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One live notification, as delivered over `org.freedesktop.Notifications`. `actions` is the raw `[key, label, key, label, …]` list from the spec.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    #[serde(default)]
    pub actions: Vec<String>,
    pub urgency: Urgency,
    /// Whether this notification may still raise a popup: `true` for a fresh arrival, `false` for one restored
    /// from persisted history at startup (those belong in the history panel, not re-popped). Runtime-only —
    /// skipped from serialization, so every restored notification deserializes back to `false`.
    #[serde(skip)]
    pub popup: bool,
    /// The notification's image, when it carried one. Runtime-only (not persisted).
    #[serde(skip)]
    pub image: Option<NotificationImage>,
}

/// An immutable view of the daemon's state, broadcast to every subscribed surface. Shared behind `Arc` so a fan-out to N surfaces clones a pointer, not the list.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Active notifications, oldest first.
    pub active: Vec<Notification>,
    /// Notifications received since the history was last marked read.
    pub unread: u32,
    /// Do-Not-Disturb: popups are suppressed, history still records.
    pub dnd: bool,
}

pub type SharedSnapshot = Arc<Snapshot>;

struct State {
    active: Vec<Notification>,
    next_id: u32,
    unread: u32,
    dnd: bool,
}

impl State {
    fn snapshot(&self) -> SharedSnapshot {
        Arc::new(Snapshot {
            active: self.active.clone(),
            unread: self.unread,
            dnd: self.dnd,
        })
    }
}

/// Auto-dismiss policy, seeded from `[notifications]` config at startup.
#[derive(Clone, Copy)]
struct Defaults {
    timeout: Duration,
    critical_sticky: bool,
}

/// The in-process notification daemon: owns the D-Bus name, holds the live state, and fans each change out to every surface that subscribed. This is the shared source the architecture note calls for — one owner, many independent per-surface subscriptions.
struct Inner {
    state: Mutex<State>,
    subscribers: Mutex<Vec<EventSender<SharedSnapshot>>>,
    defaults: Defaults,
    /// The current history is shipped here after every change; a background thread debounces and writes it.
    saver: Sender<Vec<Notification>>,
}

impl Inner {
    /// Applies `mutate`, persists the new history (debounced, off-thread), then pushes a fresh snapshot to
    /// every live subscriber, dropping any whose surface has gone.
    fn commit(&self, mutate: impl FnOnce(&mut State)) {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            mutate(&mut state);
            state.snapshot()
        };
        let _ = self.saver.send(snapshot.active.clone());
        self.subscribers
            .lock()
            .unwrap()
            .retain(|tx| tx.send(SharedSnapshot::clone(&snapshot)));
    }

    fn push(&self, mut notification: Notification, replaces_id: u32) -> u32 {
        let mut assigned = 0;
        self.commit(|state| {
            let id = if replaces_id != 0 {
                replaces_id
            } else {
                state.next_id = state.next_id.wrapping_add(1).max(1);
                state.next_id
            };
            notification.id = id;
            assigned = id;
            if let Some(existing) = state.active.iter_mut().find(|n| n.id == id) {
                *existing = notification;
            } else {
                state.active.push(notification);
                state.unread = state.unread.saturating_add(1);
            }
        });
        assigned
    }

    /// Removes `id` from the history entirely — a manual dismiss (a history-card tap or clear-all).
    fn close(&self, id: u32) {
        self.commit(|state| state.active.retain(|n| n.id != id));
    }

    /// Retires `id`'s popup while keeping it in the history: the popup stack stops showing it (it filters on
    /// `popup`), but the panel — which lists all of `active` — keeps it until dismissed. This is what a popup
    /// timeout does, so an auto-dismissed notification is still there to read later.
    fn expire(&self, id: u32) {
        self.commit(|state| {
            if let Some(n) = state.active.iter_mut().find(|n| n.id == id) {
                n.popup = false;
            }
        });
    }

    /// Schedules a popup expiry for `id` per the spec's `expire_timeout` (`>0` ms, `0` = never, `<0` = the configured default) and the urgency/critical-sticky policy. A detached timer keeps this independent of any surface, so popups expire correctly across focus changes and reloads. The notification stays in the history.
    fn schedule_expiry(&self, id: u32, expire_timeout: i32, urgency: Urgency) {
        if urgency == Urgency::Critical && self.defaults.critical_sticky {
            return;
        }
        let ms = match expire_timeout {
            t if t > 0 => t as u64,
            0 => return,
            _ => self.defaults.timeout.as_millis() as u64,
        };
        if ms == 0 {
            return;
        }
        let _ = std::thread::Builder::new()
            .name("hyprshell-notif-expiry".to_string())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(ms));
                expire(id);
            });
    }
}

pub struct NotificationService {
    inner: Arc<Inner>,
}

static SERVICE: OnceLock<NotificationService> = OnceLock::new();
/// The daemon's live D-Bus connection, kept so action invocations and closes can emit their signals from any
/// thread (the daemon thread just parks). Set once the bus name is claimed.
static CONNECTION: OnceLock<zbus::blocking::Connection> = OnceLock::new();

/// Starts the daemon once for the whole process (before any surface, so its state survives config reloads). `default_timeout`/`critical_sticky` seed the auto-dismiss policy. Idempotent; a second call is a no-op.
pub fn init(default_timeout: Duration, critical_sticky: bool) {
    SERVICE.get_or_init(|| {
        // Restored notifications deserialize with `popup = false`, so they populate the history panel without
        // re-popping on login; ids continue past the highest restored one.
        let restored = load_history();
        let next_id = restored.iter().map(|n| n.id).max().unwrap_or(0);
        let (saver, saver_rx) = channel::<Vec<Notification>>();
        let _ = std::thread::Builder::new()
            .name("hyprshell-notif-save".to_string())
            .spawn(move || run_saver(saver_rx));
        let inner = Arc::new(Inner {
            state: Mutex::new(State {
                active: restored,
                next_id,
                unread: 0,
                dnd: false,
            }),
            subscribers: Mutex::new(Vec::new()),
            defaults: Defaults {
                timeout: default_timeout,
                critical_sticky,
            },
            saver,
        });
        spawn_daemon(Arc::clone(&inner));
        NotificationService { inner }
    });
}

/// Registers `tx` (bound to a surface's event loop) to receive every state change, and immediately sends the current snapshot so the surface starts in sync. Called from a surface's `watch` producer; a no-op before [`init`].
pub fn subscribe(tx: EventSender<SharedSnapshot>) {
    if let Some(service) = SERVICE.get() {
        let snapshot = service.inner.state.lock().unwrap().snapshot();
        if tx.send(snapshot) {
            service.inner.subscribers.lock().unwrap().push(tx);
        }
    }
}

/// The current state without subscribing — for an initial read or tests; surfaces should [`subscribe`] to stay live.
pub fn snapshot_now() -> Option<SharedSnapshot> {
    SERVICE
        .get()
        .map(|service| service.inner.state.lock().unwrap().snapshot())
}

/// Removes one notification from the history — a manual dismiss (history-card tap). Emits `NotificationClosed`.
pub fn close(id: u32) {
    if let Some(service) = SERVICE.get() {
        service.inner.close(id);
    }
    emit_closed(id, 2);
}

/// Retires a notification's popup after its timeout while keeping it in the history. Emits `NotificationClosed`
/// with the expired reason, as the spec expects when a popup times out.
pub fn expire(id: u32) {
    if let Some(service) = SERVICE.get() {
        service.inner.expire(id);
    }
    emit_closed(id, 1);
}

/// Invokes a notification's action `key`: emits `ActionInvoked`, then closes it (the sender closes on
/// invocation, per the spec). Wired to the history panel's action buttons.
pub fn invoke_action(id: u32, key: &str) {
    if let Some(conn) = CONNECTION.get() {
        let _ = conn.emit_signal(None::<&str>, OBJECT_PATH, BUS_NAME, "ActionInvoked", &(id, key));
    }
    close(id);
}

/// Emits `NotificationClosed(id, reason)` (1 = expired, 2 = dismissed, 3 = app-requested) to any listeners.
fn emit_closed(id: u32, reason: u32) {
    if let Some(conn) = CONNECTION.get() {
        let _ = conn.emit_signal(None::<&str>, OBJECT_PATH, BUS_NAME, "NotificationClosed", &(id, reason));
    }
}

/// Clears the whole history and resets the unread count.
pub fn clear_all() {
    if let Some(service) = SERVICE.get() {
        service.inner.commit(|state| {
            state.active.clear();
            state.unread = 0;
        });
    }
}

/// Marks the history as seen without discarding it (e.g. when the bell panel opens).
pub fn mark_read() {
    if let Some(service) = SERVICE.get() {
        service.inner.commit(|state| state.unread = 0);
    }
}

/// Toggles Do-Not-Disturb; popups are suppressed while on, history keeps recording.
pub fn set_dnd(dnd: bool) {
    if let Some(service) = SERVICE.get() {
        service.inner.commit(|state| state.dnd = dnd);
    }
}

/// The persisted history file: a TOML array of tables under the data dir.
#[derive(Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    notifications: Vec<Notification>,
}

/// Owns the disk writes on its own thread: takes each new history snapshot, debounces a burst (keeping only
/// the last), and writes it. Ends when the daemon — and thus the sender — is dropped (i.e. process exit).
fn run_saver(rx: Receiver<Vec<Notification>>) {
    let path = history_path();
    while let Ok(mut latest) = rx.recv() {
        while let Ok(next) = rx.recv_timeout(Duration::from_millis(500)) {
            latest = next;
        }
        save_history(&path, &latest);
    }
}

fn load_history() -> Vec<Notification> {
    let Ok(text) = fs::read_to_string(history_path()) else {
        return Vec::new();
    };
    match toml::from_str::<HistoryFile>(&text) {
        Ok(file) => file.notifications,
        Err(e) => {
            tracing::warn!("notification history parse error ({e}); starting with an empty history");
            Vec::new()
        }
    }
}

/// Writes the most recent [`MAX_HISTORY`] notifications. Best-effort: a failure is logged, not surfaced.
fn save_history(path: &Path, active: &[Notification]) {
    let start = active.len().saturating_sub(MAX_HISTORY);
    let file = HistoryFile {
        notifications: active[start..].to_vec(),
    };
    match toml::to_string_pretty(&file) {
        Ok(text) => {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(path, text) {
                tracing::warn!("notification history save failed: {e}");
            }
        }
        Err(e) => tracing::warn!("notification history serialize failed: {e}"),
    }
}

fn history_path() -> PathBuf {
    crate::shared::paths::data_dir().join("notifications.toml")
}

fn spawn_daemon(inner: Arc<Inner>) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-notifications".to_string())
        .spawn(move || run_daemon(inner));
}

fn run_daemon(inner: Arc<Inner>) {
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(BUS_NAME))
        .and_then(|b| b.serve_at(OBJECT_PATH, NotificationsIface { inner }))
        .and_then(|b| b.build());
    match conn {
        Ok(conn) => {
            let _ = CONNECTION.set(conn);
            loop {
                std::thread::park();
            }
        }
        Err(e) => {
            tracing::warn!("notifications daemon not started ({e}); another daemon likely owns {BUS_NAME}");
        }
    }
}

struct NotificationsIface {
    inner: Arc<Inner>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationsIface {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .map(Urgency::from_hint)
            .unwrap_or(Urgency::Normal);
        let image = extract_image(&hints);
        let id = self.inner.push(
            Notification {
                id: 0,
                app_name,
                app_icon,
                summary,
                body,
                actions,
                urgency,
                popup: true,
                image,
            },
            replaces_id,
        );
        self.inner.schedule_expiry(id, expire_timeout, urgency);
        id
    }

    fn close_notification(&self, id: u32) {
        self.inner.close(id);
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".to_string(),
            "body-markup".to_string(),
            "actions".to_string(),
            "icon-static".to_string(),
        ]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "hyprshell".to_string(),
            "hyprshell".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }
}

/// Pulls a notification image out of the spec's raw-pixel hints (`image-data`, its underscore variant, or the
/// legacy `icon_data`) — each an `(iiibiiay)` struct — and converts it to RGBA8. `None` if absent or malformed.
fn extract_image(hints: &HashMap<String, Value<'_>>) -> Option<NotificationImage> {
    ["image-data", "image_data", "icon_data"]
        .iter()
        .find_map(|key| hints.get(*key).and_then(image_from_hint))
}

fn image_from_hint(value: &Value<'_>) -> Option<NotificationImage> {
    let Value::Structure(s) = value else {
        return None;
    };
    let fields = s.fields();
    if fields.len() < 7 {
        return None;
    }
    let width = u32::try_from(i32::try_from(&fields[0]).ok()?).ok()?;
    let height = u32::try_from(i32::try_from(&fields[1]).ok()?).ok()?;
    let rowstride = usize::try_from(i32::try_from(&fields[2]).ok()?).ok()?;
    let channels = usize::try_from(i32::try_from(&fields[5]).ok()?).ok()?;
    let Value::Array(array) = &fields[6] else {
        return None;
    };
    let data: Vec<u8> = array.iter().filter_map(|v| u8::try_from(v).ok()).collect();
    let rgba = to_rgba(width, height, rowstride, channels, &data)?;
    Some(NotificationImage {
        width,
        height,
        rgba,
    })
}

/// Repacks raw image bytes (3-channel RGB or 4-channel RGBA, laid out with `rowstride`-byte rows) into tight
/// RGBA8. `None` if the channel count is unsupported or the data is short.
fn to_rgba(width: u32, height: u32, rowstride: usize, channels: usize, data: &[u8]) -> Option<Vec<u8>> {
    if channels != 3 && channels != 4 {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = data.get(y * rowstride..y * rowstride + w * channels)?;
        for x in 0..w {
            let px = &row[x * channels..x * channels + channels];
            let alpha = if channels == 4 { px[3] } else { 255 };
            out.extend_from_slice(&[px[0], px[1], px[2], alpha]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_assigns_ids_replaces_and_counts_unread() {
        let inner = Inner {
            state: Mutex::new(State {
                active: Vec::new(),
                next_id: 0,
                unread: 0,
                dnd: false,
            }),
            subscribers: Mutex::new(Vec::new()),
            defaults: Defaults {
                timeout: Duration::from_millis(5000),
                critical_sticky: true,
            },
            saver: channel().0,
        };
        let sample = |summary: &str| Notification {
            id: 0,
            app_name: "app".into(),
            app_icon: String::new(),
            summary: summary.into(),
            body: String::new(),
            actions: Vec::new(),
            urgency: Urgency::Normal,
            popup: true,
            image: None,
        };

        let first = inner.push(sample("a"), 0);
        let second = inner.push(sample("b"), 0);
        assert_ne!(first, second, "fresh notifications get distinct ids");
        assert_eq!(inner.state.lock().unwrap().unread, 2);

        inner.push(sample("b-edited"), second);
        let state = inner.state.lock().unwrap();
        assert_eq!(state.active.len(), 2, "a replaces_id updates in place, no new entry");
        assert_eq!(state.active[1].summary, "b-edited");
        assert_eq!(state.unread, 2, "a replacement does not bump unread");
        drop(state);

        inner.close(first);
        assert_eq!(inner.state.lock().unwrap().active.len(), 1);
    }

    #[test]
    fn history_round_trips_and_restored_notifications_do_not_popup() {
        let file = HistoryFile {
            notifications: vec![Notification {
                id: 7,
                app_name: "Slack".into(),
                app_icon: String::new(),
                summary: "Ada".into(),
                body: "review at 3?".into(),
                actions: vec!["default".into(), "Open".into()],
                urgency: Urgency::Critical,
                popup: true,
                image: None,
            }],
        };
        let text = toml::to_string_pretty(&file).expect("serialize");
        let parsed: HistoryFile = toml::from_str(&text).expect("parse");
        assert_eq!(parsed.notifications.len(), 1);
        let n = &parsed.notifications[0];
        assert_eq!((n.id, n.summary.as_str()), (7, "Ada"));
        assert_eq!(n.urgency, Urgency::Critical);
        assert_eq!(n.actions, vec!["default".to_string(), "Open".to_string()]);
        // `popup` is runtime-only: it is never written, and a restored notification comes back non-popping.
        assert!(!text.contains("popup"), "popup must not be persisted");
        assert!(!n.popup, "restored notifications must not re-popup on startup");
    }

    #[test]
    fn to_rgba_repacks_channels_and_honors_rowstride() {
        // 2×1 RGB, tight rows: alpha filled to 255.
        assert_eq!(
            to_rgba(2, 1, 6, 3, &[10, 20, 30, 40, 50, 60]).unwrap(),
            vec![10, 20, 30, 255, 40, 50, 60, 255]
        );
        // 1×2 RGBA: passes through unchanged.
        assert_eq!(
            to_rgba(1, 2, 4, 4, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8]
        );
        // 1×2 RGB with a padded rowstride (3 bytes + 1 pad per row): the pad byte is skipped.
        assert_eq!(
            to_rgba(1, 2, 4, 3, &[1, 2, 3, 0, 4, 5, 6, 0]).unwrap(),
            vec![1, 2, 3, 255, 4, 5, 6, 255]
        );
        // Short data and unsupported channel counts are rejected.
        assert!(to_rgba(2, 1, 6, 3, &[1, 2, 3]).is_none());
        assert!(to_rgba(1, 1, 2, 2, &[1, 2]).is_none());
    }

    #[test]
    fn expiry_retires_the_popup_but_keeps_the_notification_in_history() {
        let inner = Inner {
            state: Mutex::new(State {
                active: Vec::new(),
                next_id: 0,
                unread: 0,
                dnd: false,
            }),
            subscribers: Mutex::new(Vec::new()),
            defaults: Defaults {
                timeout: Duration::from_millis(5000),
                critical_sticky: true,
            },
            saver: channel().0,
        };
        let id = inner.push(
            Notification {
                id: 0,
                app_name: "a".into(),
                app_icon: String::new(),
                summary: "hi".into(),
                body: String::new(),
                actions: Vec::new(),
                urgency: Urgency::Normal,
                popup: true,
                image: None,
            },
            0,
        );

        inner.expire(id);
        {
            let state = inner.state.lock().unwrap();
            assert_eq!(state.active.len(), 1, "expiry keeps it in the history");
            assert!(!state.active[0].popup, "but retires its popup");
        }

        inner.close(id);
        assert!(
            inner.state.lock().unwrap().active.is_empty(),
            "a manual dismiss removes it from the history"
        );
    }

    // Live D-Bus round-trip. Run under a private bus so it never collides with the desktop's real daemon:
    // `dbus-run-session -- cargo test -p hyprshell --lib notifications::tests::daemon -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a session bus; run under dbus-run-session"]
    fn daemon_receives_notify_over_dbus() {
        init(Duration::from_millis(5000), true);
        let client = zbus::blocking::Connection::session().expect("session bus");
        let hints: HashMap<&str, Value> = HashMap::new();
        let mut sent = false;
        for _ in 0..50 {
            let call = client.call_method(
                Some(BUS_NAME),
                OBJECT_PATH,
                Some("org.freedesktop.Notifications"),
                "Notify",
                &(
                    "test-app",
                    0u32,
                    "",
                    "Hello",
                    "World",
                    Vec::<&str>::new(),
                    &hints,
                    -1i32,
                ),
            );
            if call.is_ok() {
                sent = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(sent, "daemon claimed the name and answered Notify");

        let snapshot = snapshot_now().expect("service initialized");
        assert_eq!(snapshot.active.len(), 1);
        assert_eq!(snapshot.active[0].summary, "Hello");
        assert_eq!(snapshot.active[0].body, "World");
        assert_eq!(snapshot.unread, 1);
    }
}
