//! Open windows over `ext-foreign-toplevel-list-v1`: what exists, what it is called, and a handle on it.
//!
//! The enumeration a dock, a window switcher and an open-window launcher mode all rest on, read from the
//! compositor rather than from one compositor's IPC.
//!
//! **The identifier is the join.** Each toplevel carries an opaque, stable string the compositor promises is
//! unique for that window's whole life. On Hyprland it is exactly the `stableId` of `hyprctl clients` —
//! verified against a live 0.56.1 session — which is what lets a reading taken here be matched against one
//! taken over IPC by equality rather than by guessing from titles.
//!
//! **What this protocol does not carry.** A toplevel handle reports a title, an app id and that identifier. It
//! does *not* report which workspace or output the window is on, whether it is focused, minimised or
//! fullscreen, and it offers no way to act on the window — `zwlr-foreign-toplevel-management-v1` is the only
//! portable route to any of that. A list built from this alone is a list, not a switcher.
//!
//! It is also the only way to *name* a window as a capture source: `ext-image-copy-capture-v1` will capture a
//! toplevel, but only one identified by the `ext_foreign_toplevel_handle_v1` that only this protocol hands out.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

/// The global a compositor advertises when it can list windows at all.
pub const TOPLEVEL_LIST_INTERFACE: &str = "ext_foreign_toplevel_list_v1";

/// Names one window for as long as it is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ToplevelId(u32);

/// An open window, as the compositor describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toplevel {
    pub id: ToplevelId,
    /// Opaque, unique and stable for the window's life. Hyprland's `stableId`, and the field to match on when
    /// the same window is also being read over compositor IPC.
    pub identifier: String,
    pub title: String,
    /// The application's own id — what `class` is called everywhere except Hyprland.
    pub app_id: String,
}

type Handler = Box<dyn FnMut(&[Toplevel]) + Send>;

static HANDLERS: Mutex<Vec<Handler>> = Mutex::new(Vec::new());
static LATEST: Mutex<Vec<Toplevel>> = Mutex::new(Vec::new());
static WATCHING: OnceLock<bool> = OnceLock::new();

/// Whether the compositor lists windows at all, asked over a connection of its own so it answers outside a
/// running shell. `None` means no compositor could be reached.
pub fn toplevels_supported() -> Option<bool> {
    crate::globals::advertises(TOPLEVEL_LIST_INTERFACE)
}

/// Registers `on_change` for the window list, starting the watcher on first use.
///
/// Returns false when the compositor does not implement the protocol, in which case `on_change` is never
/// called. A handler registered after the watcher is running is handed the current list immediately.
pub fn watch(on_change: impl FnMut(&[Toplevel]) + Send + 'static) -> bool {
    let mut handler: Handler = Box::new(on_change);
    let started = *WATCHING.get_or_init(start);
    if started {
        let latest = LATEST.lock().unwrap().clone();
        if !latest.is_empty() {
            handler(&latest);
        }
    }
    HANDLERS.lock().unwrap().push(handler);
    started
}

/// The last list published, without waiting for a window to open or close.
pub fn current() -> Vec<Toplevel> {
    LATEST.lock().unwrap().clone()
}

/// Connects and binds here rather than on the watcher thread, so the answer to "does this compositor list
/// windows" is known by the time [`watch`] returns.
fn start() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, queue)) = registry_queue_init::<Watcher>(&connection) else {
        return false;
    };
    let qh = queue.handle();
    if let Err(e) = globals.bind::<ExtForeignToplevelListV1, _, _>(&qh, 1..=1, ()) {
        tracing::debug!("no ext-foreign-toplevel-list-v1: {e}");
        return false;
    }

    std::thread::Builder::new()
        .name("hyprshell-ext-toplevels".to_string())
        .spawn(move || run(Watcher::default(), connection, queue))
        .is_ok()
}

fn run(mut watcher: Watcher, connection: Connection, queue: EventQueue<Watcher>) {
    let Ok(mut event_loop) = EventLoop::<Watcher>::try_new() else {
        return;
    };
    if WaylandSource::new(connection, queue)
        .insert(event_loop.handle())
        .is_err()
    {
        return;
    }
    while !watcher.finished {
        if event_loop.dispatch(None, &mut watcher).is_err() {
            break;
        }
    }
}

#[derive(Default)]
struct Entry {
    identifier: String,
    title: String,
    app_id: String,
}

/// The accumulated list, with no protocol object of its own — which is what lets the reading be checked
/// without a compositor.
#[derive(Default)]
struct State {
    windows: HashMap<u32, Entry>,
    /// Announcement order, which is the only order this protocol offers: a `HashMap` would reshuffle a window
    /// list on every publish.
    order: Vec<u32>,
}

#[derive(Default)]
struct Watcher {
    state: State,
    finished: bool,
}

impl State {
    fn add(&mut self, key: u32) {
        if self.windows.insert(key, Entry::default()).is_none() {
            self.order.push(key);
        }
    }

    fn remove(&mut self, key: u32) {
        self.windows.remove(&key);
        self.order.retain(|open| *open != key);
    }

    fn snapshot(&self) -> Vec<Toplevel> {
        self.order
            .iter()
            .filter_map(|key| {
                let entry = self.windows.get(key)?;
                Some(Toplevel {
                    id: ToplevelId(*key),
                    identifier: entry.identifier.clone(),
                    title: entry.title.clone(),
                    app_id: entry.app_id.clone(),
                })
            })
            .collect()
    }
}

impl Watcher {
    /// Publishes the whole list. The protocol batches a window's changes behind its own `done`, so a title
    /// being retyped one keystroke at a time is one publish per commit rather than one per event.
    fn publish(&self) {
        let snapshot = self.state.snapshot();
        *LATEST.lock().unwrap() = snapshot.clone();
        for handler in HANDLERS.lock().unwrap().iter_mut() {
            handler(&snapshot);
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for Watcher {
    wayland_client::event_created_child!(Watcher, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } => {
                state.state.add(toplevel.id().protocol_id())
            }
            ext_foreign_toplevel_list_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for Watcher {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let key = proxy.id().protocol_id();
        match event {
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                state.state.windows.entry(key).or_default().identifier = identifier
            }
            ext_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.state.windows.entry(key).or_default().title = title
            }
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.state.windows.entry(key).or_default().app_id = app_id
            }
            ext_foreign_toplevel_handle_v1::Event::Done => state.publish(),
            // The window is gone and the handle is inert; destroying it is this client's half of the exchange.
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                state.state.remove(key);
                proxy.destroy();
                state.publish();
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Watcher {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from a live Hyprland 0.56.1 session, identifiers included: they are the `stableId` values
    /// `hyprctl clients` reported for the same windows at the same moment.
    fn open_windows() -> State {
        let mut state = State::default();
        for (key, identifier, app_id, title) in [
            (20, "1800000d", "kitty", "nvim"),
            (21, "1800000b", "code", "README.md - Visual Studio Code"),
            (22, "18000008", "helium", "Chat - Helium"),
        ] {
            state.add(key);
            let entry = state.windows.get_mut(&key).unwrap();
            entry.identifier = identifier.to_string();
            entry.app_id = app_id.to_string();
            entry.title = title.to_string();
        }
        state
    }

    #[test]
    fn the_list_keeps_the_order_the_compositor_announced() {
        let state = open_windows();
        assert_eq!(
            state
                .snapshot()
                .iter()
                .map(|w| w.app_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kitty", "code", "helium"]
        );
    }

    /// A window closing must leave the rest in place — the bug a `HashMap` order hides until the day a list
    /// reorders itself under the pointer.
    #[test]
    fn a_closed_window_leaves_the_others_where_they_were() {
        let mut state = open_windows();
        state.remove(21);
        assert_eq!(
            state
                .snapshot()
                .iter()
                .map(|w| w.app_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kitty", "helium"]
        );
        // Removing something that is not there is what a second `closed` would be, and it must not panic.
        state.remove(21);
        assert_eq!(state.snapshot().len(), 2);
    }

    /// Reopening the same protocol id — the compositor reuses them — must not double an entry in the order.
    #[test]
    fn a_reused_handle_is_one_entry() {
        let mut state = open_windows();
        state.add(20);
        assert_eq!(state.snapshot().len(), 3);
    }

    /// The identifier is what a reading taken here is matched against one taken over IPC. If it ever arrives
    /// empty the join silently becomes "everything matches everything".
    #[test]
    fn every_window_carries_the_identifier_the_join_is_made_on() {
        assert!(
            open_windows()
                .snapshot()
                .iter()
                .all(|w| !w.identifier.is_empty())
        );
    }

    /// The half no fixture can prove: that this reads a real compositor, and that what it reads agrees with
    /// what that compositor says about itself.
    ///
    /// `HYPRSHELL_WAYLAND_LIVE=1 cargo test -p platform-wayland toplevels -- --nocapture --test-threads=1`
    #[test]
    fn the_watcher_lists_the_windows_that_are_open() {
        use std::sync::mpsc;
        use std::time::Duration;

        if std::env::var("HYPRSHELL_WAYLAND_LIVE").is_err() {
            eprintln!("set HYPRSHELL_WAYLAND_LIVE to list the real compositor's windows; skipping");
            return;
        }

        let (published, changes) = mpsc::channel();
        assert!(
            watch(move |windows| {
                let _ = published.send(windows.to_vec());
            }),
            "the compositor advertises ext-foreign-toplevel-list-v1 but the watcher would not start"
        );

        // One publish per window as the compositor announces them; the last one to arrive is the whole list.
        let mut listed = Vec::new();
        while let Ok(windows) = changes.recv_timeout(Duration::from_millis(500)) {
            listed = windows;
        }
        eprintln!("{} windows: {listed:#?}", listed.len());

        assert!(
            !listed.is_empty(),
            "this test is running in a terminal, which is itself a window"
        );
        assert!(
            listed.iter().all(|w| !w.identifier.is_empty()),
            "an empty identifier would make every match against IPC succeed"
        );
        let identifiers: std::collections::HashSet<&str> =
            listed.iter().map(|w| w.identifier.as_str()).collect();
        assert_eq!(
            identifiers.len(),
            listed.len(),
            "the protocol promises the identifier is unique, and the join depends on it"
        );
    }
}
