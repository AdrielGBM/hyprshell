use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use platform_layershell::EventSender;
use zbus::zvariant::Value;

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// One live notification, as delivered over `org.freedesktop.Notifications`. `actions` is the raw `[key, label, key, label, …]` list from the spec.
#[derive(Clone, Debug)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
    pub urgency: Urgency,
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
}

impl Inner {
    /// Applies `mutate`, then pushes a fresh snapshot to every live subscriber, dropping any whose surface has gone.
    fn commit(&self, mutate: impl FnOnce(&mut State)) {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            mutate(&mut state);
            state.snapshot()
        };
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

    fn close(&self, id: u32) {
        self.commit(|state| state.active.retain(|n| n.id != id));
    }

    /// Schedules an auto-dismiss for `id` per the spec's `expire_timeout` (`>0` ms, `0` = never, `<0` = the configured default) and the urgency/critical-sticky policy. A detached timer keeps this independent of any surface, so popups expire correctly across focus changes and reloads.
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
                close(id);
            });
    }
}

pub struct NotificationService {
    inner: Arc<Inner>,
}

static SERVICE: OnceLock<NotificationService> = OnceLock::new();

/// Starts the daemon once for the whole process (before any surface, so its state survives config reloads). `default_timeout`/`critical_sticky` seed the auto-dismiss policy. Idempotent; a second call is a no-op.
pub fn init(default_timeout: Duration, critical_sticky: bool) {
    SERVICE.get_or_init(|| {
        let inner = Arc::new(Inner {
            state: Mutex::new(State {
                active: Vec::new(),
                next_id: 0,
                unread: 0,
                dnd: false,
            }),
            subscribers: Mutex::new(Vec::new()),
            defaults: Defaults {
                timeout: default_timeout,
                critical_sticky,
            },
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

/// Dismisses one notification (from a popup timeout or manual close).
pub fn close(id: u32) {
    if let Some(service) = SERVICE.get() {
        service.inner.close(id);
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
        Ok(_conn) => loop {
            std::thread::park();
        },
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
        let id = self.inner.push(
            Notification {
                id: 0,
                app_name,
                app_icon,
                summary,
                body,
                actions,
                urgency,
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
        };
        let sample = |summary: &str| Notification {
            id: 0,
            app_name: "app".into(),
            app_icon: String::new(),
            summary: summary.into(),
            body: String::new(),
            actions: Vec::new(),
            urgency: Urgency::Normal,
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
