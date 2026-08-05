//! Workspaces over `ext-workspace-v1`: what exists, which one is active, and the outputs each sits on.
//!
//! The reading a bar needs, from the compositor rather than from one compositor's IPC. Hyprland, Niri, Sway,
//! labwc, dwl and COSMIC all speak this; a shell that reads `hyprctl workspaces` instead works on exactly one.
//!
//! **What the protocol does not carry, and no amount of care here will produce.** A workspace handle reports a
//! name, optional coordinates, a state (active, urgent, hidden) and what may be requested of it. It does *not*
//! report how many windows are on the workspace, which applications those are, or a numeric id — and
//! `ext-foreign-toplevel-list-v1` cannot fill any of them either, because a toplevel handle never says which
//! workspace it belongs to. Anything a bar draws from window occupancy therefore has a compositor-specific
//! source or no source at all, and the caller decides which. This module reports only what was actually said.
//!
//! **A watcher is a connection and a thread of its own**, not a second consumer of the driver's loop: the driver
//! only exists inside a running shell, and a workspace reading is wanted by anything from a bar to a one-shot
//! IPC command. It starts on the first [`watch`] and lives as long as the process, like the compositor event
//! stream it replaces.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{
    Channel, Event as ChannelEvent, Sender, channel,
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

/// The global a compositor advertises when it can answer any of this.
pub const WORKSPACE_INTERFACE: &str = "ext_workspace_manager_v1";

/// The `wl_output` version that names an output. Below it a connector name is not knowable from `wl_output`
/// alone, so a workspace comes back with no outputs rather than with a wrong one.
const OUTPUT_NAME_SINCE: u32 = 4;

const STATE_ACTIVE: u32 = 1;
const STATE_URGENT: u32 = 2;
const STATE_HIDDEN: u32 = 4;
const CAN_ACTIVATE: u32 = 1;

/// Names one workspace for [`activate`], for as long as that workspace exists.
///
/// Deliberately opaque: the protocol's own `id` is an optional string that Hyprland does not send at all, and a
/// workspace's *name* is neither unique across groups nor stable. What is unambiguous is the protocol object,
/// and this is its identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceId(u32);

/// A workspace, as the compositor describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    /// Human-readable and meant for display. Hyprland sends the workspace number here.
    pub name: String,
    /// The compositor's own ordering, when it arranges workspaces in a grid at all. One dimension on Hyprland,
    /// carrying the workspace number; empty on a compositor that numbers workspaces without geometry.
    pub coordinates: Vec<u32>,
    /// The outputs of the group this workspace belongs to. Empty while it belongs to none — the protocol
    /// creates workspaces unassigned — and on a compositor whose `wl_output` predates version 4.
    pub outputs: Vec<String>,
    pub active: bool,
    pub urgent: bool,
    /// The compositor asks that this one not be displayed.
    pub hidden: bool,
    /// Whether [`activate`] would be honoured. Hyprland drops this on the workspace that is already active.
    pub can_activate: bool,
}

type Handler = Box<dyn FnMut(&[Workspace]) + Send>;

static HANDLERS: Mutex<Vec<Handler>> = Mutex::new(Vec::new());
static LATEST: Mutex<Vec<Workspace>> = Mutex::new(Vec::new());
static REQUESTS: OnceLock<Sender<Request>> = OnceLock::new();
static WATCHING: OnceLock<bool> = OnceLock::new();

enum Request {
    Activate(WorkspaceId),
}

/// Whether the compositor lists workspaces at all, asked over a connection of its own so it answers outside a
/// running shell. `None` means no compositor could be reached, which is not the same as one without workspaces.
pub fn workspaces_supported() -> Option<bool> {
    crate::globals::advertises(WORKSPACE_INTERFACE)
}

/// Registers `on_change` for the workspace list, starting the watcher on first use.
///
/// Returns false when the compositor does not implement the protocol, in which case `on_change` is never
/// called and the caller is expected to have another route. A handler registered after the watcher is already
/// running is handed the current list immediately, so a late subscriber is not blind until something moves.
pub fn watch(on_change: impl FnMut(&[Workspace]) + Send + 'static) -> bool {
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

/// The last list published, without waiting for the next change.
pub fn current() -> Vec<Workspace> {
    LATEST.lock().unwrap().clone()
}

/// Asks the compositor to activate a workspace, reporting whether the request could be sent.
///
/// Sent, not honoured: the protocol makes no promise that a workspace activates, and the compositor answers by
/// publishing a new state rather than by replying. A caller wanting to know watches for it.
pub fn activate(id: WorkspaceId) -> bool {
    REQUESTS
        .get()
        .is_some_and(|requests| requests.send(Request::Activate(id)).is_ok())
}

/// Connects, binds, and hands the loop to a thread. Binding happens here rather than there so the answer to
/// "does this compositor list workspaces" is known by the time [`watch`] returns.
fn start() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, queue)) = registry_queue_init::<Watcher>(&connection) else {
        return false;
    };
    let qh = queue.handle();
    let manager = match globals.bind::<ExtWorkspaceManagerV1, _, _>(&qh, 1..=1, ()) {
        Ok(manager) => manager,
        Err(e) => {
            tracing::debug!("no ext-workspace-v1: {e}");
            return false;
        }
    };

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

    let watcher = Watcher {
        connection: connection.clone(),
        manager,
        bound,
        state: State::default(),
        finished: false,
    };
    std::thread::Builder::new()
        .name("hyprshell-ext-workspace".to_string())
        .spawn(move || run(watcher, connection, queue, channel))
        .is_ok()
}

fn run(
    mut watcher: Watcher,
    connection: Connection,
    queue: EventQueue<Watcher>,
    requests: Channel<Request>,
) {
    let Ok(mut event_loop) = EventLoop::<Watcher>::try_new() else {
        return;
    };
    let handle = event_loop.handle();
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
struct Group {
    outputs: Vec<u32>,
    workspaces: Vec<u32>,
}

#[derive(Default)]
struct Entry {
    handle: Option<ExtWorkspaceHandleV1>,
    name: String,
    coordinates: Vec<u32>,
    state: u32,
    capabilities: u32,
}

/// Everything the events accumulate, with no protocol object of its own — which is what lets the reading this
/// module exists to produce be checked without a compositor.
#[derive(Default)]
struct State {
    /// Connector names by protocol object id, which is how a group's `output_enter` names one.
    names: HashMap<u32, String>,
    groups: HashMap<u32, Group>,
    workspaces: HashMap<u32, Entry>,
}

struct Watcher {
    connection: Connection,
    manager: ExtWorkspaceManagerV1,
    /// Bound outputs by registry name, so a monitor being unplugged can drop its own.
    bound: HashMap<u32, wl_output::WlOutput>,
    state: State,
    finished: bool,
}

impl Watcher {
    fn apply(&mut self, request: Request) {
        let Request::Activate(WorkspaceId(key)) = request;
        let Some(handle) = self
            .state
            .workspaces
            .get(&key)
            .and_then(|entry| entry.handle.as_ref())
        else {
            return;
        };
        handle.activate();
        // Every request in this protocol is staged until a commit, and a request made outside the loop's own
        // dispatch sits in the outgoing buffer until something flushes it.
        self.manager.commit();
        let _ = self.connection.flush();
    }

    /// Publishes the whole list, on `done` and never before it: the protocol batches a change that spans several
    /// objects — deactivating one workspace and activating another — and publishing per event would put a bar
    /// through a frame with no active workspace at all.
    fn publish(&self) {
        let snapshot = self.state.snapshot();
        *LATEST.lock().unwrap() = snapshot.clone();
        for handler in HANDLERS.lock().unwrap().iter_mut() {
            handler(&snapshot);
        }
    }
}

impl State {
    /// The outputs of whichever group holds `workspace`, named. A workspace belongs to at most one group, and
    /// to none at all between being created and being assigned.
    fn outputs_of(&self, workspace: u32) -> Vec<String> {
        self.groups
            .values()
            .find(|group| group.workspaces.contains(&workspace))
            .map(|group| {
                group
                    .outputs
                    .iter()
                    .filter_map(|output| self.names.get(output).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Ordered by the compositor's own coordinates, then by name — a `HashMap` order would reshuffle the bar on
    /// every publish. Numeric order is the caller's business: `name` is a string, and "10" sorts before "2".
    fn snapshot(&self) -> Vec<Workspace> {
        let mut workspaces: Vec<Workspace> = self
            .workspaces
            .iter()
            .map(|(key, entry)| Workspace {
                id: WorkspaceId(*key),
                name: entry.name.clone(),
                coordinates: entry.coordinates.clone(),
                outputs: self.outputs_of(*key),
                active: entry.state & STATE_ACTIVE != 0,
                urgent: entry.state & STATE_URGENT != 0,
                hidden: entry.state & STATE_HIDDEN != 0,
                can_activate: entry.capabilities & CAN_ACTIVATE != 0,
            })
            .collect();
        workspaces.sort_by(|a, b| {
            a.coordinates
                .cmp(&b.coordinates)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        workspaces
    }
}

/// Coordinates arrive as a flat array of native-endian `uint32`.
fn coordinates(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for Watcher {
    wayland_client::event_created_child!(Watcher, ExtWorkspaceManagerV1, [
        ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                state
                    .state
                    .groups
                    .insert(workspace_group.id().protocol_id(), Group::default());
            }
            ext_workspace_manager_v1::Event::Workspace { workspace } => {
                state.state.workspaces.insert(
                    workspace.id().protocol_id(),
                    Entry {
                        handle: Some(workspace),
                        ..Entry::default()
                    },
                );
            }
            ext_workspace_manager_v1::Event::Done => state.publish(),
            ext_workspace_manager_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for Watcher {
    fn event(
        state: &mut Self,
        proxy: &ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let key = proxy.id().protocol_id();
        if let ext_workspace_group_handle_v1::Event::Removed = event {
            state.state.groups.remove(&key);
            proxy.destroy();
            return;
        }
        let group = state.state.groups.entry(key).or_default();
        match event {
            ext_workspace_group_handle_v1::Event::OutputEnter { output } => {
                group.outputs.push(output.id().protocol_id())
            }
            ext_workspace_group_handle_v1::Event::OutputLeave { output } => {
                let gone = output.id().protocol_id();
                group.outputs.retain(|output| *output != gone);
            }
            ext_workspace_group_handle_v1::Event::WorkspaceEnter { workspace } => {
                group.workspaces.push(workspace.id().protocol_id())
            }
            ext_workspace_group_handle_v1::Event::WorkspaceLeave { workspace } => {
                let gone = workspace.id().protocol_id();
                group.workspaces.retain(|workspace| *workspace != gone);
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for Watcher {
    fn event(
        state: &mut Self,
        proxy: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let key = proxy.id().protocol_id();
        if let ext_workspace_handle_v1::Event::Removed = event {
            state.state.workspaces.remove(&key);
            proxy.destroy();
            return;
        }
        let entry = state.state.workspaces.entry(key).or_default();
        match event {
            ext_workspace_handle_v1::Event::Name { name } => entry.name = name,
            ext_workspace_handle_v1::Event::Coordinates { coordinates: raw } => {
                entry.coordinates = coordinates(&raw)
            }
            ext_workspace_handle_v1::Event::State { state } => entry.state = state.into(),
            ext_workspace_handle_v1::Event::Capabilities { capabilities } => {
                entry.capabilities = capabilities.into()
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

/// A monitor plugged in after the watcher started still has to be nameable, or every workspace on it comes back
/// with no output and a per-monitor bar shows nothing.
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

    #[test]
    fn coordinates_are_read_as_native_endian_words() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&3u32.to_ne_bytes());
        bytes.extend_from_slice(&7u32.to_ne_bytes());
        assert_eq!(coordinates(&bytes), vec![3, 7]);
        assert_eq!(coordinates(&[]), Vec::<u32>::new());
        // A trailing partial word is not a coordinate; taking it would invent one.
        assert_eq!(coordinates(&[1, 0, 0]), Vec::<u32>::new());
    }

    /// The bits a bar reads, and the one that must not be confused with occupancy: `hidden` is the compositor
    /// asking that a workspace not be drawn, which is not the same as it having no windows — a question this
    /// protocol does not answer at all.
    #[test]
    fn the_state_bits_are_independent() {
        let entry = |state: u32| Entry {
            state,
            ..Entry::default()
        };
        let read = |e: &Entry| {
            (
                e.state & STATE_ACTIVE != 0,
                e.state & STATE_URGENT != 0,
                e.state & STATE_HIDDEN != 0,
            )
        };
        assert_eq!(read(&entry(0)), (false, false, false));
        assert_eq!(read(&entry(STATE_ACTIVE)), (true, false, false));
        assert_eq!(
            read(&entry(STATE_ACTIVE | STATE_URGENT | STATE_HIDDEN)),
            (true, true, true)
        );
    }

    /// Hyprland's own reading, captured from a live 0.56.1 session: one group per output, no `id` event at all,
    /// one coordinate carrying the workspace number, and `activate` missing from the capabilities of whichever
    /// workspace is already active.
    fn hyprland_state() -> State {
        let mut state = State::default();
        state.names.insert(3, "eDP-1".to_string());
        state.groups.insert(
            100,
            Group {
                outputs: vec![3],
                workspaces: vec![11, 12, 13, 14],
            },
        );
        for (key, name, coordinate, bits, caps) in [
            (11, "1", 1u32, 0, CAN_ACTIVATE | 8),
            (12, "4", 4, STATE_ACTIVE, 8),
            (13, "2", 2, 0, CAN_ACTIVATE | 8),
            (14, "3", 3, STATE_URGENT, CAN_ACTIVATE | 8),
        ] {
            state.workspaces.insert(
                key,
                Entry {
                    handle: None,
                    name: name.to_string(),
                    coordinates: vec![coordinate],
                    state: bits,
                    capabilities: caps,
                },
            );
        }
        state
    }

    #[test]
    fn a_snapshot_reports_what_the_compositor_said_in_its_own_order() {
        let workspaces = hyprland_state().snapshot();

        assert_eq!(
            workspaces
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3", "4"],
            "the coordinates are the compositor's order, not the order it announced them in"
        );
        let active: Vec<&str> = workspaces
            .iter()
            .filter(|w| w.active)
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(active, vec!["4"]);
        assert!(workspaces.iter().any(|w| w.urgent && w.name == "3"));
        assert!(
            workspaces.iter().all(|w| w.outputs == vec!["eDP-1"]),
            "every workspace takes the outputs of the group holding it"
        );
        assert!(
            !workspaces.iter().find(|w| w.active).unwrap().can_activate,
            "Hyprland drops `activate` from the workspace already active, which is what a pill must not offer"
        );
    }

    /// The protocol creates a workspace before assigning it to a group, and a bar filtering per monitor has to
    /// survive the gap rather than dropping the workspace or inventing an output for it.
    #[test]
    fn a_workspace_in_no_group_has_no_outputs() {
        let mut state = hyprland_state();
        state
            .groups
            .get_mut(&100)
            .unwrap()
            .workspaces
            .retain(|w| *w != 12);
        let orphan = state
            .snapshot()
            .into_iter()
            .find(|w| w.name == "4")
            .expect("it is still a workspace");
        assert!(orphan.outputs.is_empty());
        assert!(orphan.active, "and everything else about it still reads");
    }

    /// A group whose output the client never bound — `wl_output` below version 4 — names nothing rather than
    /// reporting a placeholder a per-monitor filter would then match against.
    #[test]
    fn an_unnamed_output_is_absent_rather_than_guessed() {
        let mut state = hyprland_state();
        state.names.clear();
        assert!(state.snapshot().iter().all(|w| w.outputs.is_empty()));
    }

    /// The half no fixture can prove: that the watcher reads a real compositor, and that activating over the
    /// protocol actually moves it.
    ///
    /// Needs a live session, so it is opt-in the way the clipboard and capture round-trips are:
    /// `HYPRSHELL_WAYLAND_LIVE=1 cargo test -p platform-wayland workspaces -- --nocapture --test-threads=1`
    ///
    /// **It switches workspace and switches back**, which is the only way to observe an activation: the protocol
    /// answers a request with a new state, not with a reply. The workspace that was active when the test started
    /// is restored before anything is asserted, so a failed expectation does not leave the desktop somewhere
    /// else.
    #[test]
    fn the_watcher_reads_the_compositor_and_activation_moves_it() {
        use std::sync::mpsc;
        use std::time::Duration;

        if std::env::var("HYPRSHELL_WAYLAND_LIVE").is_err() {
            eprintln!("set HYPRSHELL_WAYLAND_LIVE to watch the real compositor; skipping");
            return;
        }

        let (published, changes) = mpsc::channel();
        assert!(
            watch(move |workspaces| {
                let _ = published.send(workspaces.to_vec());
            }),
            "the compositor advertises ext-workspace-v1 but the watcher would not start"
        );

        let first = changes
            .recv_timeout(Duration::from_secs(3))
            .expect("the watcher publishes the current list without waiting for a change");
        eprintln!("{} workspaces: {first:#?}", first.len());
        assert!(!first.is_empty(), "a session has at least one workspace");
        assert_eq!(
            first.iter().filter(|w| w.active).count(),
            1,
            "exactly one workspace is active on a single-output session"
        );
        assert!(
            first.iter().all(|w| !w.name.is_empty()),
            "a name is what a pill draws"
        );

        let was_active = first.iter().find(|w| w.active).expect("one is active").id;
        let Some(target) = first.iter().find(|w| w.can_activate && !w.active) else {
            eprintln!("only one workspace exists; nothing to activate");
            return;
        };
        assert!(activate(target.id), "the request could not be sent");

        let mut moved = None;
        while let Ok(workspaces) = changes.recv_timeout(Duration::from_secs(3)) {
            if let Some(active) = workspaces.iter().find(|w| w.active) {
                moved = Some(active.id);
                break;
            }
        }
        activate(was_active);
        // Given back before asserting, and given time to land: a failing expectation must not be the reason the
        // desktop is left on a workspace nobody asked for.
        std::thread::sleep(Duration::from_millis(300));

        assert_eq!(
            moved,
            Some(target.id),
            "activating a workspace over the protocol has to move the compositor"
        );
    }
}
