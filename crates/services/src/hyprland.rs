use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

use platform_layershell::EventSender;

use util::broadcast::{Broadcast, Service};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub active: i32,
    /// The monitor holding focus, so a per-monitor bar can show only its own workspaces.
    pub focused_monitor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: i32,
    /// Hyprland's own name. Numbered workspaces name themselves after their id; a special workspace is
    /// `special:<name>`, which is the only place its name is recoverable.
    pub name: String,
    pub windows: u32,
    pub monitor: String,
    /// The window classes on this workspace, in Hyprland's order — what a pill draws app icons from.
    pub clients: Vec<String>,
}

impl Workspace {
    /// Hyprland gives special workspaces (scratchpads) negative ids and a `special:` name.
    pub fn is_special(&self) -> bool {
        self.id < 0
    }

    /// The bare name of a special workspace (`special:magic` → `magic`), for matching config icons.
    pub fn special_name(&self) -> &str {
        self.name.strip_prefix("special:").unwrap_or(&self.name)
    }

    pub fn is_occupied(&self) -> bool {
        self.windows > 0
    }
}

/// The focused window, or `None` when the compositor reports none (an empty workspace, a layer surface holding
/// focus). Every field is what Hyprland calls it, so a config regex written against `hyprctl clients` matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveWindow {
    pub title: String,
    pub class: String,
    pub address: String,
}

impl ActiveWindow {
    pub fn is_empty(&self) -> bool {
        self.address.is_empty()
    }
}

/// The active keyboard layout of the main keyboard, as Hyprland names it (`"English (US)"`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayout {
    pub name: String,
    /// The device the layout belongs to, needed to switch it.
    pub device: String,
}

#[derive(Deserialize)]
struct WorkspaceJson {
    id: i32,
    #[serde(default)]
    name: String,
    windows: u32,
    #[serde(default)]
    monitor: String,
}

/// One window the compositor is managing, as `j/clients` reports it. Every field keeps Hyprland's own meaning
/// so a rule written against `hyprctl clients` reads the same here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Client {
    /// `0x…`, unique and stable for the window's life — the handle every dispatcher takes.
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: i32,
    pub workspace: i32,
    pub workspace_name: String,
    /// Hyprland's monitor *index*, not its name: `j/clients` reports the id and only `j/monitors` maps it back.
    pub monitor: i32,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub floating: bool,
    /// Maximized or fullscreen; Hyprland distinguishes the two, a shell asking "is something covering the
    /// screen" does not.
    pub fullscreen: bool,
    pub pinned: bool,
    /// An unmapped window is one the compositor is not drawing — a tray-minimized application, mostly. Kept
    /// rather than filtered so a window list can show it as hidden instead of losing it.
    pub mapped: bool,
    pub xwayland: bool,
}

#[derive(Deserialize)]
struct ClientJson {
    #[serde(default)]
    address: String,
    #[serde(default)]
    class: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    pid: i32,
    workspace: ClientWorkspaceJson,
    #[serde(default)]
    monitor: i32,
    #[serde(default)]
    at: (i32, i32),
    #[serde(default)]
    size: (i32, i32),
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    fullscreen: Fullscreen,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    mapped: bool,
    #[serde(default)]
    xwayland: bool,
}

/// Hyprland reported `fullscreen` as a bool until 0.42 and as a mode integer (0 none, 1 maximized, 2 fullscreen)
/// after it. Accepting both keeps the client list working across the versions a user might be on, instead of
/// failing the whole parse on the field's type.
#[derive(Deserialize, Default, Clone, Copy)]
#[serde(untagged)]
enum Fullscreen {
    Flag(bool),
    Mode(i64),
    #[default]
    Absent,
}

impl Fullscreen {
    fn is_set(self) -> bool {
        match self {
            Self::Flag(on) => on,
            Self::Mode(mode) => mode != 0,
            Self::Absent => false,
        }
    }
}

#[derive(Deserialize)]
struct ClientWorkspaceJson {
    id: i32,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct ActiveJson {
    id: i32,
}

/// One output, as `j/monitors` describes it. The connector `name` (`DP-1`) is what per-monitor config, the
/// layer-shell surfaces and the settings app all key on; `make`/`model` are what a human recognises it by.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Screen {
    pub name: String,
    pub description: String,
    pub make: String,
    pub model: String,
    pub serial: String,
    pub width: u32,
    pub height: u32,
    pub refresh: f32,
    /// Position in the compositor's layout space, which is what orders the list left-to-right.
    pub at: (i32, i32),
    pub scale: f32,
    pub transform: i32,
    pub focused: bool,
    pub disabled: bool,
    pub vrr: bool,
    pub dpms: bool,
    pub active_workspace: i32,
}

#[derive(Deserialize)]
struct MonitorJson {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    make: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    serial: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default, rename = "refreshRate")]
    refresh_rate: f32,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default)]
    transform: i32,
    focused: bool,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    vrr: bool,
    #[serde(default = "yes", rename = "dpmsStatus")]
    dpms_status: bool,
    #[serde(default, rename = "activeWorkspace")]
    active_workspace: Option<ActiveJson>,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct ActiveWindowJson {
    #[serde(default)]
    title: String,
    #[serde(default)]
    class: String,
    #[serde(default)]
    address: String,
}

#[derive(Deserialize)]
struct DevicesJson {
    keyboards: Vec<KeyboardJson>,
}

#[derive(Deserialize)]
struct KeyboardJson {
    name: String,
    main: bool,
    active_keymap: String,
}

/// The per-instance Hyprland socket directory, or `None` when not running under Hyprland. Hyprland ≥ 0.40 puts it under `$XDG_RUNTIME_DIR/hypr/$SIG`; older versions used `/tmp/hypr/$SIG`.
pub fn socket_dir() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(runtime).join("hypr").join(&sig);
        if path.exists() {
            return Some(path);
        }
    }
    let legacy = PathBuf::from("/tmp/hypr").join(&sig);
    legacy.exists().then_some(legacy)
}

fn request(dir: &Path, command: &str) -> std::io::Result<String> {
    let mut stream = UnixStream::connect(dir.join(".socket.sock"))?;
    stream.write_all(command.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

/// The name of the currently focused monitor, or `None` if Hyprland can't be queried.
pub fn focused_monitor(dir: &Path) -> Option<String> {
    let raw = request(dir, "j/monitors").ok()?;
    serde_json::from_str::<Vec<MonitorJson>>(&raw)
        .ok()?
        .into_iter()
        .find(|m| m.focused)
        .map(|m| m.name)
}

fn query_monitors(dir: &Path, command: &str) -> Option<Vec<MonitorJson>> {
    serde_json::from_str(&request(dir, command).ok()?).ok()
}

/// Every output the compositor knows about, ordered left-to-right then top-to-bottom by their position in the
/// layout — the order a user reads their desk in, and the one a per-monitor settings list wants.
pub fn screens(dir: &Path) -> Vec<Screen> {
    // `all` includes outputs that are connected but switched off, which is the difference between a settings
    // list that can re-enable a monitor and one that cannot see it. Not every Hyprland accepts the argument, so
    // a refusal falls back to the plain query rather than reporting no screens at all.
    let Some(parsed) =
        query_monitors(dir, "j/monitors all").or_else(|| query_monitors(dir, "j/monitors"))
    else {
        return Vec::new();
    };
    let mut screens: Vec<Screen> = parsed
        .into_iter()
        .map(|m| Screen {
            name: m.name,
            description: m.description,
            make: m.make,
            model: m.model,
            serial: m.serial,
            width: m.width,
            height: m.height,
            refresh: m.refresh_rate,
            at: (m.x, m.y),
            scale: m.scale,
            transform: m.transform,
            focused: m.focused,
            disabled: m.disabled,
            vrr: m.vrr,
            dpms: m.dpms_status,
            active_workspace: m.active_workspace.map(|w| w.id).unwrap_or_default(),
        })
        .collect();
    screens.sort_by_key(|s| (s.at.1, s.at.0));
    screens
}

/// The monitor name carried by a `focusedmon>>NAME,WORKSPACE` event line, if that's what it is.
pub fn monitor_from_focus_event(line: &str) -> Option<String> {
    line.strip_prefix("focusedmon>>")
        .and_then(|rest| rest.split(',').next())
        .map(str::to_string)
}

/// Runs a Hyprland dispatcher. Hyprland ≥ 0.55 evaluates socket commands as Lua, so `dispatch workspace N` no
/// longer parses; `call` is the Lua expression the socket wraps as `hl.dispatch(<call>)`, e.g.
/// `hl.dsp.focus({ workspace = 3 })`. Errors are reported rather than returned: every caller is a UI handler
/// with nothing useful to do about a refused dispatch.
fn dispatch(dir: &Path, call: &str) {
    let cmd = format!("dispatch {call}");
    match request(dir, &cmd) {
        Ok(resp) if resp.to_ascii_lowercase().contains("error") => {
            tracing::warn!("hyprshell: `{cmd}` -> {resp:?}")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("hyprshell: `{cmd}` failed: {e}"),
    }
}

pub fn focus_workspace(dir: &Path, id: i32) {
    dispatch(dir, &format!("hl.dsp.focus({{ workspace = {id} }})"));
}

/// The two ways Hyprland 0.56's `dpms` dispatcher might take its state.
///
/// Every other Lua dispatcher names its arguments when given the wrong ones — `hl.dsp.focus({ nonsense = 1 })`
/// answers with "Expected one of: direction, monitor, window, …" — but `hl.dsp.dpms` accepts anything at all,
/// including a function, without complaint. There is nothing to read the shape off, so it is not guessed:
/// [`set_dpms`] tries these and checks whether the compositor's own `dpmsStatus` moved.
fn dpms_calls(state: &str) -> [String; 2] {
    [
        format!("hl.dsp.dpms(\"{state}\")"),
        format!("hl.dsp.dpms({{ state = \"{state}\" }})"),
    ]
}

/// Switches every monitor's output on or off, and reports whether it worked.
///
/// Verified rather than trusted, for the reason above: the call is made and `dpmsStatus` read back, so an idle
/// stage that blanks the screen either does or says it could not. DPMS is idempotent, so a second shape landing
/// after a first one already worked costs nothing.
pub fn set_dpms(dir: &Path, on: bool) -> bool {
    let state = if on { "on" } else { "off" };
    if dpms_is(dir, on) {
        return true;
    }
    for call in dpms_calls(state) {
        dispatch(dir, &call);
        if dpms_is(dir, on) {
            return true;
        }
    }
    tracing::warn!("hyprshell: no `hl.dsp.dpms` call shape turned the outputs {state}");
    false
}

/// Whether every enabled monitor's output is in the requested state. Read from the compositor rather than
/// remembered, since a `hyprctl` or a keybind can move it behind the shell's back.
fn dpms_is(dir: &Path, on: bool) -> bool {
    let Some(monitors) = query_monitors(dir, "j/monitors") else {
        return false;
    };
    !monitors.is_empty() && monitors.iter().all(|m| m.dpms_status == on)
}

/// Every window the compositor is managing, in Hyprland's own order. One parse feeds both readers of it — the
/// workspace pills, which want the classes grouped by workspace, and the client list itself — so the two can't
/// disagree about what is open.
pub fn clients(dir: &Path) -> Vec<Client> {
    let Ok(raw) = request(dir, "j/clients") else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<Vec<ClientJson>>(&raw) else {
        return Vec::new();
    };
    parsed
        .into_iter()
        .map(|c| Client {
            address: c.address,
            class: c.class,
            title: c.title,
            pid: c.pid,
            workspace: c.workspace.id,
            workspace_name: c.workspace.name,
            monitor: c.monitor,
            at: c.at,
            size: c.size,
            floating: c.floating,
            fullscreen: c.fullscreen.is_set(),
            pinned: c.pinned,
            mapped: c.mapped,
            xwayland: c.xwayland,
        })
        .collect()
}

/// The window classes on each workspace, keyed by workspace id. A window with no class draws no icon, so it is
/// dropped here rather than leaving a gap in the pill.
fn classes_by_workspace(clients: &[Client]) -> HashMap<i32, Vec<String>> {
    let mut by_workspace: HashMap<i32, Vec<String>> = HashMap::new();
    for client in clients {
        if client.class.is_empty() {
            continue;
        }
        by_workspace
            .entry(client.workspace)
            .or_default()
            .push(client.class.clone());
    }
    by_workspace
}

/// Special workspaces are kept rather than filtered out — a scratchpad is something a bar wants to indicate —
/// and sorted after the numbered ones so their negative ids don't put them at the front.
fn query_snapshot(dir: &Path) -> Option<Snapshot> {
    let workspaces_raw = request(dir, "j/workspaces").ok()?;
    let active_raw = request(dir, "j/activeworkspace").ok()?;
    let mut clients = classes_by_workspace(&clients(dir));

    let mut workspaces: Vec<Workspace> =
        serde_json::from_str::<Vec<WorkspaceJson>>(&workspaces_raw)
            .ok()?
            .into_iter()
            .map(|w| Workspace {
                windows: w.windows,
                clients: clients.remove(&w.id).unwrap_or_default(),
                name: w.name,
                monitor: w.monitor,
                id: w.id,
            })
            .collect();
    workspaces.sort_by_key(|w| (w.is_special(), w.id));

    let active = serde_json::from_str::<ActiveJson>(&active_raw).ok()?.id;
    Some(Snapshot {
        workspaces,
        active,
        focused_monitor: focused_monitor(dir).unwrap_or_default(),
    })
}

fn affects_workspaces(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "workspace>>",
        "workspacev2>>",
        "createworkspace",
        "destroyworkspace",
        "focusedmon>>",
        "moveworkspace",
        "openwindow>>",
        "closewindow>>",
        "movewindow>>",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// A reader of the raw Hyprland event stream.
type EventHandler = Box<dyn FnMut(&str) + Send>;

static HANDLERS: Mutex<Vec<EventHandler>> = Mutex::new(Vec::new());
static EVENT_THREAD: OnceLock<()> = OnceLock::new();

/// Registers `handler` on the compositor's event stream, opening it on first use.
///
/// Hyprland's `.socket2.sock` is a single-consumer firehose, and every derived reading — workspaces, the
/// focused window, the keyboard layout — is driven by the same lines. One connection with a list of handlers
/// keeps that at one socket and one read per event no matter how many services read from it, rather than a
/// connection per service on top of the connection-per-bar the shared-source design already rules out.
fn on_events(handler: EventHandler) {
    HANDLERS.lock().unwrap().push(handler);
    EVENT_THREAD.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("hyprshell-hypr-events".to_string())
            .spawn(run_event_stream);
    });
}

fn run_event_stream() {
    let Some(dir) = socket_dir() else { return };
    let Ok(stream) = UnixStream::connect(dir.join(".socket2.sock")) else {
        tracing::warn!("cannot open the Hyprland event socket; live updates are off");
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        for handler in HANDLERS.lock().unwrap().iter_mut() {
            handler(&line);
        }
    }
}

static WORKSPACES: Service<Snapshot> = Service::new("hyprshell-workspaces", run_workspaces);

/// The single shared workspaces source: publishes the current layout, then republishes on every event that
/// could have changed it. Fanned out to every bar that subscribed, so N bars cost one parse per change (the M3
/// "one producer, N readers"), not N.
fn run_workspaces(service: &Arc<Broadcast<Snapshot>>) {
    let Some(dir) = socket_dir() else { return };
    if let Some(snapshot) = query_snapshot(&dir) {
        service.publish(snapshot);
    }
    // The broadcast outlives this call: the handler owns a clone of the `Arc` the service holds, so the
    // producer thread can return once it has registered instead of parking on a socket of its own.
    let published = Arc::clone(service);
    on_events(Box::new(move |line| {
        if affects_workspaces(line)
            && let Some(snapshot) = query_snapshot(&dir)
        {
            published.publish(snapshot);
        }
    }));
}

/// The last published workspace snapshot, with no socket round-trip — what a scroll handler steps from.
pub fn current_workspaces() -> Option<Snapshot> {
    WORKSPACES.current()
}

/// Registers `tx` (bound to a bar's event loop) for live workspace snapshots and sends the current one, spinning
/// up the single shared Hyprland listener on first use. Called from a bar's `watch` producer.
pub fn subscribe(tx: EventSender<Snapshot>) {
    WORKSPACES.subscribe(tx);
}

/// Whether a line reports something that could have changed which window has focus, or its title.
fn affects_active_window(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "activewindow>>",
        "activewindowv2>>",
        "windowtitle>>",
        "windowtitlev2>>",
        "closewindow>>",
        "openwindow>>",
        "workspace>>",
        "focusedmon>>",
        "fullscreen>>",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// The focused window, or the empty value when nothing is focused. Hyprland answers `j/activewindow` with `{}`
/// on an empty workspace, which deserializes to the default rather than failing.
pub fn active_window(dir: &Path) -> ActiveWindow {
    let Ok(raw) = request(dir, "j/activewindow") else {
        return ActiveWindow::default();
    };
    serde_json::from_str::<ActiveWindowJson>(&raw)
        .map(|w| ActiveWindow {
            title: w.title,
            class: w.class,
            address: w.address,
        })
        .unwrap_or_default()
}

static ACTIVE_WINDOW: Service<ActiveWindow> =
    Service::new("hyprshell-active-window", run_active_window);

fn run_active_window(service: &Arc<Broadcast<ActiveWindow>>) {
    let Some(dir) = socket_dir() else { return };
    service.publish(active_window(&dir));
    let published = Arc::clone(service);
    // A title changes on nearly every keystroke in a terminal or browser; republishing an identical reading
    // would wake every subscribed surface for nothing, so unchanged readings are dropped here.
    let mut last = ActiveWindow::default();
    on_events(Box::new(move |line| {
        if !affects_active_window(line) {
            return;
        }
        let current = active_window(&dir);
        if current != last {
            last = current.clone();
            published.publish(current);
        }
    }));
}

pub fn subscribe_active_window(tx: EventSender<ActiveWindow>) {
    ACTIVE_WINDOW.subscribe(tx);
}

/// Focuses a window by its Hyprland address (`0x…`), for clicking the active-window chip or a window list.
pub fn focus_window(dir: &Path, address: &str) {
    dispatch(
        dir,
        &format!("hl.dsp.focus({{ window = \"address:{address}\" }})"),
    );
}

/// The window actions live under `hl.dsp.window.<action>`, and which *shape* each takes cannot be read off the
/// compositor: called outside a dispatch context they refuse to build at all, so the usual trick of passing a
/// nonsense field and reading the "expected one of" reply gets nothing. The same rule the `dpms` dispatcher
/// taught applies — try a shape, then check whether the compositor's own state moved. See [`set_dpms`].
///
/// The table form first, because it is the one shape a verified dispatcher (`focus`) is known to take.
fn window_calls(action: &str, address: &str, extra: &str) -> [String; 2] {
    [
        format!("hl.dsp.window.{action}({{ window = \"address:{address}\"{extra} }})"),
        format!("hl.dsp.window.{action}(\"address:{address}\")"),
    ]
}

/// Runs each shape until `moved` reports the compositor did what was asked. The state is re-read after each
/// attempt rather than slept on: a dispatch reply arrives after the action, so the next query already sees it.
fn dispatch_until(dir: &Path, calls: [String; 2], moved: impl Fn(&Path) -> bool) -> bool {
    if moved(dir) {
        return true;
    }
    for call in calls {
        dispatch(dir, &call);
        if moved(dir) {
            return true;
        }
    }
    false
}

fn client_of(dir: &Path, address: &str) -> Option<Client> {
    clients(dir).into_iter().find(|c| c.address == address)
}

/// Closes a window.
///
/// The one action with no second shape to fall back on: a close either happened or it did not, and trying another
/// spelling of it afterwards is how the wrong window gets closed twice. A refused dispatch is a log line.
pub fn close_window(dir: &Path, address: &str) {
    dispatch(
        dir,
        &format!("hl.dsp.window.close({{ window = \"address:{address}\" }})"),
    );
}

/// Floats or tiles a window, reporting whether the compositor agreed.
pub fn set_floating(dir: &Path, address: &str, floating: bool) -> bool {
    let address = address.to_string();
    dispatch_until(
        dir,
        window_calls("float", &address, &format!(", state = {floating}")),
        move |dir| client_of(dir, &address).is_some_and(|client| client.floating == floating),
    )
}

/// Puts a window into or out of fullscreen, reporting whether the compositor agreed.
pub fn set_fullscreen(dir: &Path, address: &str, fullscreen: bool) -> bool {
    let address = address.to_string();
    dispatch_until(
        dir,
        window_calls("fullscreen", &address, &format!(", state = {fullscreen}")),
        move |dir| client_of(dir, &address).is_some_and(|client| client.fullscreen == fullscreen),
    )
}

/// Moves a window to a workspace by id, reporting whether it arrived.
pub fn move_window_to_workspace(dir: &Path, address: &str, workspace: i32) -> bool {
    let address = address.to_string();
    let calls = [
        format!(
            "hl.dsp.window.move({{ window = \"address:{address}\", workspace = \"{workspace}\" }})"
        ),
        format!("hl.dsp.window.move(\"address:{address}\", \"{workspace}\")"),
    ];
    dispatch_until(dir, calls, move |dir| {
        client_of(dir, &address).is_some_and(|client| client.workspace == workspace)
    })
}

/// Whether a line reports something that could have changed the set of open windows, where they are, or how
/// they are laid out. Deliberately wider than [`affects_workspaces`]: a window moving between monitors or
/// toggling float changes nothing about the workspace pills and everything about a window list.
fn affects_clients(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "openwindow>>",
        "closewindow>>",
        "movewindow>>",
        "movewindowv2>>",
        "windowtitle>>",
        "windowtitlev2>>",
        "fullscreen>>",
        "changefloatingmode>>",
        "pin>>",
        "minimize>>",
        "monitorremoved>>",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

static CLIENTS: Service<Vec<Client>> = Service::new("hyprshell-clients", run_clients);

/// The window list, republished whenever the compositor reports something that could have changed it. Costs one
/// `j/clients` round-trip per such event — the same one the workspace pills already pay — and nothing at rest.
fn run_clients(service: &Arc<Broadcast<Vec<Client>>>) {
    let Some(dir) = socket_dir() else { return };
    let mut last = clients(&dir);
    service.publish(last.clone());
    let published = Arc::clone(service);
    on_events(Box::new(move |line| {
        if !affects_clients(line) {
            return;
        }
        // A title changes on nearly every keystroke in a terminal, and most of those land on a window nobody is
        // listing; republishing an identical list would wake every subscriber for nothing.
        let current = clients(&dir);
        if current != last {
            last = current.clone();
            published.publish(current);
        }
    }));
}

pub fn subscribe_clients(tx: EventSender<Vec<Client>>) {
    CLIENTS.subscribe(tx);
}

/// The last published window list, with no socket round-trip — what a click handler acts on.
pub fn current_clients() -> Option<Vec<Client>> {
    CLIENTS.current()
}

fn affects_screens(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "monitoradded>>",
        "monitoraddedv2>>",
        "monitorremoved>>",
        "monitorremovedv2>>",
        "focusedmon>>",
        "configreloaded",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

static SCREENS: Service<Vec<Screen>> = Service::new("hyprshell-screens", run_screens);

/// The output list. Separate from the compositor-agnostic `platform_layershell::outputs()` the surface layer
/// reconciles against, which knows a Wayland output's name and nothing else: mode, scale, make and model only
/// exist on this side, and a settings page listing monitors needs them.
fn run_screens(service: &Arc<Broadcast<Vec<Screen>>>) {
    let Some(dir) = socket_dir() else { return };
    let mut last = screens(&dir);
    service.publish(last.clone());
    let published = Arc::clone(service);
    on_events(Box::new(move |line| {
        if !affects_screens(line) {
            return;
        }
        let current = screens(&dir);
        if current != last {
            last = current.clone();
            published.publish(current);
        }
    }));
}

pub fn subscribe_screens(tx: EventSender<Vec<Screen>>) {
    SCREENS.subscribe(tx);
}

/// The last published output list, without a socket round-trip.
pub fn current_screens() -> Option<Vec<Screen>> {
    SCREENS.current()
}

/// The main keyboard's active layout, or `None` when Hyprland reports no keyboard.
pub fn keyboard_layout(dir: &Path) -> Option<KeyboardLayout> {
    let raw = request(dir, "j/devices").ok()?;
    let devices: DevicesJson = serde_json::from_str(&raw).ok()?;
    let keyboard = devices
        .keyboards
        .iter()
        .find(|k| k.main)
        .or_else(|| devices.keyboards.first())?;
    Some(KeyboardLayout {
        name: keyboard.active_keymap.clone(),
        device: keyboard.name.clone(),
    })
}

static KEYBOARD: Service<KeyboardLayout> = Service::new("hyprshell-keyboard", run_keyboard);

fn run_keyboard(service: &Arc<Broadcast<KeyboardLayout>>) {
    let Some(dir) = socket_dir() else { return };
    if let Some(layout) = keyboard_layout(&dir) {
        service.publish(layout);
    }
    let published = Arc::clone(service);
    on_events(Box::new(move |line| {
        if line.starts_with("activelayout>>")
            && let Some(layout) = keyboard_layout(&dir)
        {
            published.publish(layout);
        }
    }));
}

pub fn subscribe_keyboard(tx: EventSender<KeyboardLayout>) {
    KEYBOARD.subscribe(tx);
}

/// The wire form of a layout switch: a bare top-level command with space-separated arguments, *not* a
/// `dispatch` payload. Separated from the call so a test can hold the shape without switching anyone's keyboard.
fn switch_layout_command(device: &str, to: &str) -> String {
    format!("switchxkblayout {device} {to}")
}

/// Moves `device` to its next configured keyboard layout.
///
/// **Not a dispatcher, which is why this looked impossible.** `switchxkblayout` was a hyprlang dispatcher and
/// was not carried into the Lua API: `hl.dsp` has no keyboard entry on 0.56, and `hl.device` is a config setter
/// that returns nothing for a keyboard name. It survives as a *top-level IPC command* — the same kind as
/// `devices` or `keyword` — so it goes over the socket verbatim rather than through `dispatch`. Looking only at
/// `hl.dsp` is what made this read as unsupported.
///
/// Verified against the running compositor rather than assumed, the same rule `set_dpms` follows: `next` and an
/// explicit index both answer `ok`, and a name no keyboard has answers `device not found`. That last one is what
/// makes the reply worth reading — a wrong device fails silently otherwise.
///
/// Nothing is read back here. The compositor emits `activelayout>>` on a real change, which the keyboard
/// service already watches, so the chip follows a switch made from a keybind exactly as it follows this one.
pub fn cycle_keyboard_layout(dir: &Path, device: &str) {
    let command = switch_layout_command(device, "next");
    match request(dir, &command) {
        Ok(reply) if reply.trim().eq_ignore_ascii_case("ok") => {}
        Ok(reply) => tracing::warn!("hyprshell: `{command}` -> {reply:?}"),
        Err(e) => tracing::warn!("hyprshell: `{command}` failed: {e}"),
    }
}

/// Cycles the layout of whichever keyboard [`keyboard_layout`] reports, for a caller that has no device in hand
/// — a bar chip, a keybind. Resolved per call rather than remembered: the main keyboard is exactly the thing
/// that changes when one is plugged in.
pub fn cycle_main_keyboard_layout() {
    let Some(dir) = socket_dir() else { return };
    let Some(layout) = keyboard_layout(&dir) else {
        tracing::warn!("hyprshell: no keyboard to switch the layout of");
        return;
    };
    cycle_keyboard_layout(&dir, &layout.device);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout switch is a top-level command, not a dispatch, and the difference is the whole feature.
    ///
    /// `switchxkblayout` was a hyprlang dispatcher and did not survive into the Lua API, which is what made this
    /// read as impossible for a release: `hl.dsp` has no keyboard entry and `hl.device` answers nothing for a
    /// keyboard name. It is still there as a bare IPC command, the same kind as `devices`. Wrapping it in
    /// `dispatch …` — the shape every other mutation here takes — is the one plausible way to get this wrong,
    /// and the compositor answers a Lua parse error rather than anything that looks like a missing feature.
    #[test]
    fn a_layout_switch_goes_over_the_socket_bare_rather_than_as_a_dispatch() {
        let command = switch_layout_command("at-translated-set-2-keyboard", "next");
        assert_eq!(command, "switchxkblayout at-translated-set-2-keyboard next");
        assert!(
            !command.contains("dispatch") && !command.contains("hl.dsp"),
            "a dispatch payload would be answered with a Lua parse error, not a missing-feature error: {command}"
        );
        // An index is the other spelling the compositor takes, and the caller builds it the same way.
        assert_eq!(switch_layout_command("kbd", "0"), "switchxkblayout kbd 0");
    }

    #[test]
    fn event_filters_match_their_own_events_and_nothing_else() {
        assert!(affects_workspaces("workspacev2>>3,3"));
        assert!(affects_workspaces("openwindow>>0x1,3,kitty,term"));
        assert!(!affects_workspaces("activelayout>>kbd,English (US)"));

        assert!(affects_active_window("activewindowv2>>0x55f1"));
        assert!(affects_active_window("windowtitle>>0x55f1"));
        assert!(!affects_active_window("activelayout>>kbd,English (US)"));
        assert!(
            !affects_active_window("createworkspace>>4"),
            "a workspace appearing does not change which window has focus"
        );
    }

    #[test]
    fn an_empty_workspace_reports_no_active_window_rather_than_failing() {
        // Hyprland answers `j/activewindow` with `{}` when nothing is focused.
        let parsed: ActiveWindowJson = serde_json::from_str("{}").expect("an empty object parses");
        let window = ActiveWindow {
            title: parsed.title,
            class: parsed.class,
            address: parsed.address,
        };
        assert!(window.is_empty());
        assert_eq!(window, ActiveWindow::default());
    }

    #[test]
    fn active_window_json_keeps_the_fields_a_chip_shows() {
        let raw = r#"{"address":"0x55f1","class":"firefox","title":"Docs — Firefox","pid":42}"#;
        let parsed: ActiveWindowJson = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.class, "firefox");
        assert_eq!(parsed.title, "Docs — Firefox");
        assert_eq!(parsed.address, "0x55f1", "unknown fields are ignored");
    }

    #[test]
    fn keyboard_layout_prefers_the_main_keyboard() {
        let raw = r#"{"keyboards":[
            {"name":"usb-kbd","main":false,"active_keymap":"German"},
            {"name":"builtin","main":true,"active_keymap":"English (US)"}
        ]}"#;
        let devices: DevicesJson = serde_json::from_str(raw).unwrap();
        let main = devices.keyboards.iter().find(|k| k.main).unwrap();
        assert_eq!(main.active_keymap, "English (US)");
        assert_eq!(main.name, "builtin", "the device name is what switches it");
    }

    #[test]
    fn the_client_list_and_the_workspace_pills_come_from_one_parse() {
        let raw = r#"[
            {"address":"0x1","class":"kitty","title":"nvim","pid":10,"workspace":{"id":3,"name":"3"},
             "monitor":0,"at":[10,20],"size":[800,600],"floating":false,"fullscreen":0,"mapped":true,
             "pinned":false,"xwayland":false},
            {"address":"0x2","class":"firefox","title":"Docs","pid":11,"workspace":{"id":3,"name":"3"},
             "monitor":0,"at":[0,0],"size":[1920,1080],"floating":true,"fullscreen":2,"mapped":true,
             "pinned":true,"xwayland":true},
            {"address":"0x3","class":"","title":"","pid":12,"workspace":{"id":-99,"name":"special:magic"},
             "monitor":1,"at":[0,0],"size":[1,1],"floating":false,"fullscreen":0,"mapped":false,
             "pinned":false,"xwayland":false}
        ]"#;
        let parsed: Vec<ClientJson> = serde_json::from_str(raw).expect("the client list parses");
        let list: Vec<Client> = parsed
            .into_iter()
            .map(|c| Client {
                address: c.address,
                class: c.class,
                title: c.title,
                pid: c.pid,
                workspace: c.workspace.id,
                workspace_name: c.workspace.name,
                monitor: c.monitor,
                at: c.at,
                size: c.size,
                floating: c.floating,
                fullscreen: c.fullscreen.is_set(),
                pinned: c.pinned,
                mapped: c.mapped,
                xwayland: c.xwayland,
            })
            .collect();

        assert_eq!(list[0].at, (10, 20));
        assert_eq!(list[0].size, (800, 600));
        assert!(!list[0].fullscreen);
        assert!(list[1].fullscreen, "mode 2 is fullscreen");
        assert!(list[1].pinned && list[1].xwayland);
        assert_eq!(list[2].workspace_name, "special:magic");
        assert!(!list[2].mapped, "an unmapped window stays in the list");

        // The pills read the same parse, and a window with no class draws no icon rather than an empty slot.
        let grouped = classes_by_workspace(&list);
        assert_eq!(
            grouped[&3],
            vec!["kitty".to_string(), "firefox".to_string()]
        );
        assert!(
            !grouped.contains_key(&-99),
            "the only window there has no class"
        );
    }

    #[test]
    fn the_fullscreen_field_is_read_as_both_a_flag_and_a_mode() {
        // Hyprland changed the type in 0.42; a shell that only understood one would fail the whole parse on the
        // other, losing the window list rather than one field.
        let flag: ClientJson =
            serde_json::from_str(r#"{"workspace":{"id":1},"fullscreen":true}"#).unwrap();
        assert!(flag.fullscreen.is_set());
        let mode: ClientJson =
            serde_json::from_str(r#"{"workspace":{"id":1},"fullscreen":1}"#).unwrap();
        assert!(
            mode.fullscreen.is_set(),
            "1 is maximized, which still covers"
        );
        let none: ClientJson = serde_json::from_str(r#"{"workspace":{"id":1}}"#).unwrap();
        assert!(
            !none.fullscreen.is_set(),
            "an absent field is not fullscreen"
        );
    }

    #[test]
    fn screens_keep_the_fields_a_monitor_list_shows() {
        let raw = r#"[
            {"name":"DP-1","description":"Dell U2720Q","make":"Dell","model":"U2720Q","serial":"ABC",
             "width":3840,"height":2160,"refreshRate":59.997,"x":1920,"y":0,"scale":2.0,"transform":0,
             "focused":true,"disabled":false,"vrr":false,"dpmsStatus":true,
             "activeWorkspace":{"id":3,"name":"3"}},
            {"name":"eDP-1","focused":false}
        ]"#;
        let parsed: Vec<MonitorJson> = serde_json::from_str(raw).expect("the monitor list parses");
        assert_eq!(parsed[0].refresh_rate, 59.997);
        assert_eq!(parsed[0].active_workspace.as_ref().unwrap().id, 3);
        // A minimal entry still parses: the fields a per-monitor list needs are optional, the connector is not.
        assert_eq!(parsed[1].name, "eDP-1");
        assert_eq!(parsed[1].scale, 1.0, "an absent scale is 1, not 0");
        assert!(
            parsed[1].dpms_status,
            "an absent dpms state means the output is on"
        );
    }

    #[test]
    fn the_client_and_screen_filters_take_the_events_their_lists_depend_on() {
        assert!(affects_clients("changefloatingmode>>0x1,1"));
        assert!(
            affects_clients("movewindowv2>>0x1,3,3"),
            "a window changing workspace changes the list"
        );
        assert!(
            !affects_clients("activelayout>>kbd,English (US)"),
            "a keyboard layout does not move a window"
        );
        assert!(
            !affects_clients("createworkspace>>4"),
            "an empty workspace has no windows to list"
        );

        assert!(affects_screens("monitoraddedv2>>1,DP-2,Dell"));
        assert!(affects_screens("focusedmon>>DP-1,3"));
        assert!(
            !affects_screens("openwindow>>0x1,3,kitty,term"),
            "opening a window does not change the output list"
        );
    }

    #[test]
    fn monitor_from_focus_event_reads_the_name_before_the_comma() {
        assert_eq!(
            monitor_from_focus_event("focusedmon>>DP-1,3"),
            Some("DP-1".to_string())
        );
        assert_eq!(monitor_from_focus_event("workspace>>3"), None);
    }
}
