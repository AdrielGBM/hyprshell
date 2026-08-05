//! Windows over `zwlr-foreign-toplevel-management-v1`: which one has focus, where it is, and acting on it.
//!
//! The other half of a window list, and the only portable one. `ext-foreign-toplevel-list-v1` (`toplevels.rs`)
//! enumerates windows and gives each a stable identifier; it says nothing about which is focused, minimised or
//! fullscreen, reports no output, and offers no way to raise or close anything. This protocol answers all of
//! that and carries no identifier at all.
//!
//! **The two do not join.** A handle here and a handle there describe the same window and share nothing a
//! client could match on — not an id, not a serial, nothing but a title and an app id that any two windows of
//! the same application have in common. So a reading is taken from one protocol or the other in whole, never
//! assembled from both: "the focused window" comes from here, "the window to capture" from there.
//!
//! **What it cannot say either.** No geometry, no workspace, no process id. A window's position and size are
//! deliberately absent — `set_rectangle` sends a rectangle *to* the compositor, for the animation a minimise
//! comes out of, and there is no reverse. Anything needing those stays on a compositor's own IPC.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::channel::{
    Channel, Event as ChannelEvent, Sender, channel,
};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

/// The global a compositor advertises when it can be told to act on a window.
pub const TOPLEVEL_MANAGER_INTERFACE: &str = "zwlr_foreign_toplevel_manager_v1";

/// The version that added fullscreen, both as a state and as a request. Below it a caller asking for one is
/// told so rather than being silently ignored.
const FULLSCREEN_SINCE: u32 = 2;

/// The `wl_output` version that names an output.
const OUTPUT_NAME_SINCE: u32 = 4;

const STATE_MAXIMIZED: u32 = 0;
const STATE_MINIMIZED: u32 = 1;
const STATE_ACTIVATED: u32 = 2;
const STATE_FULLSCREEN: u32 = 3;

/// Names one window for as long as it is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ManagedToplevelId(u32);

impl ManagedToplevelId {
    /// The raw token, for a caller that has to key something on a window's identity — a list row, a stored
    /// preference — and wants a number rather than a `Debug` rendering it would then depend on.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Rebuilds an id from [`ManagedToplevelId::raw`].
    ///
    /// Mostly for tests, which cannot otherwise produce two windows that differ only in identity — the case a
    /// window list has to survive, since two windows of one application share a title far more often than they
    /// share nothing. Fabricating one is safe: an id the compositor never issued matches no window, so every
    /// action against it is a no-op rather than the wrong window being acted on.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// A window, as the compositor's management protocol describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedToplevel {
    pub id: ManagedToplevelId,
    pub title: String,
    /// The application's own id — what `class` is called everywhere except Hyprland.
    pub app_id: String,
    /// The outputs the window is visible on. More than one when it straddles a boundary, none while the
    /// compositor has not placed it or its `wl_output` predates version 4.
    pub outputs: Vec<String>,
    /// This is the focused window. The reading `ext-foreign-toplevel-list-v1` cannot produce at all.
    pub activated: bool,
    pub minimized: bool,
    pub maximized: bool,
    /// Reported only by a compositor implementing version 2 or above; false on version 1 whether or not the
    /// window is actually fullscreen.
    pub fullscreen: bool,
}

type Handler = Box<dyn FnMut(&[ManagedToplevel]) + Send>;

static HANDLERS: Mutex<Vec<Handler>> = Mutex::new(Vec::new());
static LATEST: Mutex<Vec<ManagedToplevel>> = Mutex::new(Vec::new());
static REQUESTS: OnceLock<Sender<Request>> = OnceLock::new();
static WATCHING: OnceLock<bool> = OnceLock::new();

enum Request {
    Focus(ManagedToplevelId),
    Close(ManagedToplevelId),
    Fullscreen(ManagedToplevelId, bool),
    Minimized(ManagedToplevelId, bool),
    Maximized(ManagedToplevelId, bool),
}

/// Whether the compositor can be told to act on a window, asked over a connection of its own so it answers
/// outside a running shell. `None` means no compositor could be reached.
pub fn toplevel_control_supported() -> Option<bool> {
    crate::globals::advertises(TOPLEVEL_MANAGER_INTERFACE)
}

/// Registers `on_change` for the window list, starting the watcher on first use.
///
/// Returns false when the compositor does not implement the protocol, in which case `on_change` is never
/// called. A handler registered after the watcher is running is handed the current list immediately.
pub fn watch(on_change: impl FnMut(&[ManagedToplevel]) + Send + 'static) -> bool {
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

/// The last list published, without waiting for a window to open, close or take focus.
pub fn current() -> Vec<ManagedToplevel> {
    LATEST.lock().unwrap().clone()
}

/// The focused window, when the compositor reports one. `None` on an empty workspace, and while a layer
/// surface this shell owns holds the keyboard.
pub fn focused() -> Option<ManagedToplevel> {
    current().into_iter().find(|window| window.activated)
}

/// Raises and focuses a window.
pub fn focus(id: ManagedToplevelId) -> bool {
    send(Request::Focus(id))
}

/// Asks a window to close — the same request its own close button makes, so an application with unsaved work
/// gets to put up its dialog rather than being killed.
pub fn close(id: ManagedToplevelId) -> bool {
    send(Request::Close(id))
}

pub fn set_fullscreen(id: ManagedToplevelId, fullscreen: bool) -> bool {
    send(Request::Fullscreen(id, fullscreen))
}

pub fn set_minimized(id: ManagedToplevelId, minimized: bool) -> bool {
    send(Request::Minimized(id, minimized))
}

pub fn set_maximized(id: ManagedToplevelId, maximized: bool) -> bool {
    send(Request::Maximized(id, maximized))
}

/// Whether the request could be handed to the watcher — not whether the compositor honoured it. The protocol
/// answers an action by publishing a new state, so a caller wanting to know watches for it.
fn send(request: Request) -> bool {
    REQUESTS
        .get()
        .is_some_and(|requests| requests.send(request).is_ok())
}

/// Connects and binds here rather than on the watcher thread, so the answer to "can this compositor be told to
/// act on a window" is known by the time [`watch`] returns.
fn start() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, queue)) = registry_queue_init::<Watcher>(&connection) else {
        return false;
    };
    let qh = queue.handle();
    if let Err(e) = globals.bind::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ()) {
        tracing::debug!("no wlr-foreign-toplevel-management: {e}");
        return false;
    }
    // Focusing a window is a request against a seat, so a compositor with no seat can list windows and not
    // raise them. That is a working watcher, not a reason to have none.
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ()).ok();
    if seat.is_none() {
        tracing::warn!("no wl_seat: windows can be listed and closed but not focused");
    }

    let mut bound = HashMap::new();
    globals.contents().with_list(|list| {
        for global in list {
            if global.interface == "wl_output" && global.version >= OUTPUT_NAME_SINCE {
                let output: wl_output::WlOutput =
                    globals
                        .registry()
                        .bind(global.name, OUTPUT_NAME_SINCE, &qh, ());
                bound.insert(global.name, output);
            }
        }
    });

    let (requests, channel) = channel();
    if REQUESTS.set(requests).is_err() {
        return false;
    }

    std::thread::Builder::new()
        .name("hyprshell-wlr-toplevels".to_string())
        .spawn(move || run(Seed { seat, bound }, connection, queue, channel))
        .is_ok()
}

/// What the watcher needs that can cross a thread boundary. The loop handle cannot — it holds an `Rc` — and it
/// does not exist until the loop does, so the watcher itself is assembled on the far side.
struct Seed {
    seat: Option<wl_seat::WlSeat>,
    bound: HashMap<u32, wl_output::WlOutput>,
}

fn run(seed: Seed, connection: Connection, queue: EventQueue<Watcher>, requests: Channel<Request>) {
    let Ok(mut event_loop) = EventLoop::<Watcher>::try_new() else {
        return;
    };
    let handle = event_loop.handle();
    let mut watcher = Watcher {
        connection: connection.clone(),
        seat: seed.seat,
        bound: seed.bound,
        state: State::default(),
        loop_handle: Some(handle.clone()),
        pending: None,
        finished: false,
    };
    if WaylandSource::new(connection, queue)
        .insert(handle.clone())
        .is_err()
    {
        return;
    }
    let registered = handle.insert_source(requests, |event, _, watcher: &mut Watcher| {
        if let ChannelEvent::Msg(request) = event {
            watcher.apply(request);
        }
    });
    if registered.is_err() {
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
    handle: Option<ZwlrForeignToplevelHandleV1>,
    title: String,
    app_id: String,
    outputs: Vec<u32>,
    states: Vec<u32>,
}

/// Everything the events accumulate, with no protocol object of its own — which is what lets the reading be
/// checked without a compositor.
#[derive(Default)]
struct State {
    names: HashMap<u32, String>,
    windows: HashMap<u32, Entry>,
    /// Announcement order, the only order this protocol offers.
    order: Vec<u32>,
}

/// A focus request that has been sent and not yet taken effect, with the tries it has left.
struct Pending {
    target: u32,
    left: u8,
}

struct Watcher {
    connection: Connection,
    seat: Option<wl_seat::WlSeat>,
    bound: HashMap<u32, wl_output::WlOutput>,
    state: State,
    /// Filled in once the loop exists, which is what lets a focus request arm a retry.
    loop_handle: Option<LoopHandle<'static, Watcher>>,
    pending: Option<Pending>,
    finished: bool,
}

impl State {
    fn add(&mut self, key: u32, handle: ZwlrForeignToplevelHandleV1) {
        if self
            .windows
            .insert(
                key,
                Entry {
                    handle: Some(handle),
                    ..Entry::default()
                },
            )
            .is_none()
        {
            self.order.push(key);
        }
    }

    fn remove(&mut self, key: u32) {
        self.windows.remove(&key);
        self.order.retain(|open| *open != key);
    }

    fn snapshot(&self) -> Vec<ManagedToplevel> {
        self.order
            .iter()
            .filter_map(|key| {
                let entry = self.windows.get(key)?;
                Some(ManagedToplevel {
                    id: ManagedToplevelId(*key),
                    title: entry.title.clone(),
                    app_id: entry.app_id.clone(),
                    outputs: entry
                        .outputs
                        .iter()
                        .filter_map(|output| self.names.get(output).cloned())
                        .collect(),
                    activated: entry.states.contains(&STATE_ACTIVATED),
                    minimized: entry.states.contains(&STATE_MINIMIZED),
                    maximized: entry.states.contains(&STATE_MAXIMIZED),
                    fullscreen: entry.states.contains(&STATE_FULLSCREEN),
                })
            })
            .collect()
    }
}

impl Watcher {
    fn apply(&mut self, request: Request) {
        let key = match request {
            Request::Focus(ManagedToplevelId(key))
            | Request::Close(ManagedToplevelId(key))
            | Request::Fullscreen(ManagedToplevelId(key), _)
            | Request::Minimized(ManagedToplevelId(key), _)
            | Request::Maximized(ManagedToplevelId(key), _) => key,
        };
        let Some(handle) = self
            .state
            .windows
            .get(&key)
            .and_then(|entry| entry.handle.as_ref())
        else {
            return;
        };
        match request {
            Request::Focus(_) => match &self.seat {
                Some(seat) => {
                    handle.activate(seat);
                    self.await_focus(key);
                }
                None => tracing::warn!("cannot focus a window without a seat"),
            },
            Request::Close(_) => handle.close(),
            Request::Fullscreen(_, on) if handle.version() >= FULLSCREEN_SINCE => {
                if on {
                    handle.set_fullscreen(None);
                } else {
                    handle.unset_fullscreen();
                }
            }
            Request::Fullscreen(..) => tracing::warn!(
                "this compositor's wlr-foreign-toplevel-management is version {}; it has no fullscreen request",
                handle.version()
            ),
            Request::Minimized(_, true) => handle.set_minimized(),
            Request::Minimized(_, false) => handle.unset_minimized(),
            Request::Maximized(_, true) => handle.set_maximized(),
            Request::Maximized(_, false) => handle.unset_maximized(),
        }
        // A request made outside the loop's own dispatch sits in the outgoing buffer until something flushes it.
        let _ = self.connection.flush();
    }

    /// Watches for a focus request to take effect, and asks again if it did not.
    ///
    /// **A compositor ignores `activate` while another surface holds the seat's keyboard.** Measured against
    /// Hyprland 0.56.1: with a layer surface up at `KeyboardInteractivity::Exclusive` the request changes
    /// nothing at all, and the same request lands the moment that surface is gone. That is the whole reason a
    /// window switcher living in a layer surface did nothing — and it cannot be fixed by ordering alone,
    /// because the surface is torn down over the shell's connection while this goes over the watcher's, and
    /// two connections have no ordering between them.
    ///
    /// So the request is repeated, briefly, until the compositor acts on it or the window stops existing. The
    /// deadline is short enough that a user who changed their mind and clicked elsewhere is not fought with.
    fn await_focus(&mut self, target: u32) {
        const TRIES: u8 = 8;
        self.pending = Some(Pending {
            target,
            left: TRIES,
        });
        self.arm_retry();
    }

    fn arm_retry(&mut self) {
        const RETRY: Duration = Duration::from_millis(70);
        let Some(handle) = self.loop_handle.clone() else {
            return;
        };
        let _ = handle.insert_source(
            Timer::from_duration(RETRY),
            |_, _, watcher: &mut Watcher| {
                watcher.retry_focus();
                TimeoutAction::Drop
            },
        );
    }

    fn retry_focus(&mut self) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let target = pending.target;
        let landed = self
            .state
            .windows
            .get(&target)
            .is_some_and(|entry| entry.states.contains(&STATE_ACTIVATED));
        // Gone, or focused: either way there is nothing left to ask for.
        if landed || !self.state.windows.contains_key(&target) {
            self.pending = None;
            return;
        }
        pending.left -= 1;
        if pending.left == 0 {
            tracing::warn!("the compositor never acted on a request to focus a window");
            self.pending = None;
            return;
        }
        if let (Some(seat), Some(entry)) = (self.seat.clone(), self.state.windows.get(&target))
            && let Some(handle) = entry.handle.as_ref()
        {
            handle.activate(&seat);
            let _ = self.connection.flush();
        }
        self.arm_retry();
    }

    /// Publishes the whole list. Each window batches its own changes behind a `done`, so a title retyped a
    /// keystroke at a time is one publish per commit rather than one per event.
    fn publish(&self) {
        let snapshot = self.state.snapshot();
        *LATEST.lock().unwrap() = snapshot.clone();
        for handler in HANDLERS.lock().unwrap().iter_mut() {
            handler(&snapshot);
        }
    }
}

/// The state arrives as a flat array of native-endian `uint32`, one per state that is set.
fn states(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Watcher {
    wayland_client::event_created_child!(Watcher, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } => {
                state.state.add(toplevel.id().protocol_id(), toplevel)
            }
            zwlr_foreign_toplevel_manager_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Watcher {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let key = proxy.id().protocol_id();
        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.state.windows.entry(key).or_default().title = title
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.state.windows.entry(key).or_default().app_id = app_id
            }
            zwlr_foreign_toplevel_handle_v1::Event::OutputEnter { output } => state
                .state
                .windows
                .entry(key)
                .or_default()
                .outputs
                .push(output.id().protocol_id()),
            zwlr_foreign_toplevel_handle_v1::Event::OutputLeave { output } => {
                let gone = output.id().protocol_id();
                state
                    .state
                    .windows
                    .entry(key)
                    .or_default()
                    .outputs
                    .retain(|output| *output != gone);
            }
            // The whole set every time, so a state that stopped being reported is one that was unset.
            zwlr_foreign_toplevel_handle_v1::Event::State { state: raw } => {
                state.state.windows.entry(key).or_default().states = states(&raw)
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => state.publish(),
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                state.state.remove(key);
                proxy.destroy();
                state.publish();
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Watcher {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.state.names.insert(proxy.id().protocol_id(), name);
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Watcher {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// A monitor plugged in after the watcher started still has to be nameable, or every window on it reports no
/// output at all.
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Watcher {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" && version >= OUTPUT_NAME_SINCE => {
                let output: wl_output::WlOutput = registry.bind(name, OUTPUT_NAME_SINCE, qh, ());
                state.bound.insert(name, output);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(output) = state.bound.remove(&name) {
                    state.state.names.remove(&output.id().protocol_id());
                    output.release();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_windows() -> State {
        let mut state = State::default();
        state.names.insert(3, "eDP-1".to_string());
        for (key, app_id, title, states) in [
            (30, "kitty", "nvim", vec![STATE_ACTIVATED]),
            (31, "code", "README.md", vec![STATE_MAXIMIZED]),
            (32, "helium", "Docs", vec![STATE_MINIMIZED]),
        ] {
            state.add_for_test(key);
            let entry = state.windows.get_mut(&key).unwrap();
            entry.app_id = app_id.to_string();
            entry.title = title.to_string();
            entry.outputs = vec![3];
            entry.states = states;
        }
        state
    }

    impl State {
        /// [`State::add`] without a protocol object, which a fixture cannot make.
        fn add_for_test(&mut self, key: u32) {
            if self.windows.insert(key, Entry::default()).is_none() {
                self.order.push(key);
            }
        }
    }

    #[test]
    fn states_are_read_as_native_endian_words() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&STATE_ACTIVATED.to_ne_bytes());
        bytes.extend_from_slice(&STATE_MAXIMIZED.to_ne_bytes());
        assert_eq!(states(&bytes), vec![STATE_ACTIVATED, STATE_MAXIMIZED]);
        assert_eq!(states(&[]), Vec::<u32>::new());
    }

    /// The reading this protocol exists for, and the one `ext-foreign-toplevel-list-v1` cannot produce.
    #[test]
    fn exactly_one_window_is_the_focused_one() {
        let windows = open_windows().snapshot();
        let focused: Vec<&str> = windows
            .iter()
            .filter(|w| w.activated)
            .map(|w| w.app_id.as_str())
            .collect();
        assert_eq!(focused, vec!["kitty"]);
        assert!(windows.iter().any(|w| w.minimized && w.app_id == "helium"));
        assert!(windows.iter().any(|w| w.maximized && w.app_id == "code"));
        assert!(
            windows.iter().all(|w| !w.fullscreen),
            "a state the compositor did not report is unset, not unknown"
        );
        assert!(windows.iter().all(|w| w.outputs == vec!["eDP-1"]));
    }

    /// The `state` event carries the whole set every time, so unsetting one is that value no longer arriving —
    /// a handler that merged rather than replaced would leave a window minimised for ever.
    #[test]
    fn a_state_that_stops_being_reported_is_unset() {
        let mut state = open_windows();
        state.windows.get_mut(&32).unwrap().states = Vec::new();
        let restored = state
            .snapshot()
            .into_iter()
            .find(|w| w.app_id == "helium")
            .unwrap();
        assert!(!restored.minimized);
    }

    #[test]
    fn a_closed_window_leaves_the_others_where_they_were() {
        let mut state = open_windows();
        state.remove(31);
        assert_eq!(
            state
                .snapshot()
                .iter()
                .map(|w| w.app_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kitty", "helium"]
        );
    }

    /// The half no fixture can prove: that this reads a real compositor, and that what it calls the focused
    /// window is the one that actually has focus.
    ///
    /// `HYPRSHELL_WAYLAND_LIVE=1 cargo test -p platform-wayland toplevel_control -- --nocapture --test-threads=1`
    #[test]
    fn the_watcher_agrees_with_the_compositor_about_which_window_has_focus() {
        use std::sync::mpsc;
        use std::time::Duration;

        if std::env::var("HYPRSHELL_WAYLAND_LIVE").is_err() {
            eprintln!("set HYPRSHELL_WAYLAND_LIVE to read the real compositor; skipping");
            return;
        }

        let (published, changes) = mpsc::channel();
        assert!(
            watch(move |windows| {
                let _ = published.send(windows.to_vec());
            }),
            "the compositor advertises the manager but the watcher would not start"
        );

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
            listed.iter().filter(|w| w.activated).count() <= 1,
            "two focused windows at once means the state array is being merged instead of replaced"
        );
        assert_eq!(
            focused().map(|w| w.id),
            listed.iter().find(|w| w.activated).map(|w| w.id),
            "the convenience reading and the list have to agree"
        );
    }

    /// Whether `activate` actually moves the compositor, with no layer surface anywhere near it.
    ///
    /// This exists to tell two failures apart. A window switcher that does nothing could be failing here — the
    /// request never lands — or failing above, because whoever asked closed a keyboard-grabbing surface
    /// straight afterwards and the compositor handed focus back. Only isolating the request answers that.
    ///
    /// `HYPRSHELL_WAYLAND_LIVE=1 cargo test -p platform-wayland activate_moves -- --nocapture`
    ///
    /// **It focuses another window and puts the focus back.**
    #[test]
    fn activate_moves_the_compositor_on_its_own() {
        use std::sync::mpsc;
        use std::time::Duration;

        if std::env::var("HYPRSHELL_WAYLAND_LIVE").is_err() {
            eprintln!("set HYPRSHELL_WAYLAND_LIVE to focus a real window; skipping");
            return;
        }

        let (published, changes) = mpsc::channel();
        assert!(watch(move |windows| {
            let _ = published.send(windows.to_vec());
        }));

        let mut listed = Vec::new();
        while let Ok(windows) = changes.recv_timeout(Duration::from_millis(500)) {
            listed = windows;
        }
        let was = listed.iter().find(|w| w.activated).map(|w| w.id);
        let Some(target) = listed.iter().find(|w| !w.activated) else {
            eprintln!("only one window is open; nothing to switch to");
            return;
        };
        eprintln!(
            "focused={was:?} switching to {:?} {:?}",
            target.app_id, target.title
        );

        assert!(focus(target.id), "the request could not be sent");
        let mut moved = None;
        let deadline = 12;
        for _ in 0..deadline {
            if let Ok(windows) = changes.recv_timeout(Duration::from_millis(250))
                && let Some(active) = windows.iter().find(|w| w.activated)
            {
                moved = Some(active.id);
                if moved == Some(target.id) {
                    break;
                }
            }
        }

        if let Some(was) = was {
            focus(was);
            std::thread::sleep(Duration::from_millis(400));
        }
        assert_eq!(
            moved,
            Some(target.id),
            "activate did not move the compositor, so the switcher's problem is here and not in its caller"
        );
    }
}
