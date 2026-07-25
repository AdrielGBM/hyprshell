use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

use platform_layershell::EventSender;

use crate::shared::services::broadcast::{Broadcast, Service};

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

#[derive(Deserialize)]
struct ClientJson {
    #[serde(default)]
    class: String,
    workspace: ClientWorkspaceJson,
}

#[derive(Deserialize)]
struct ClientWorkspaceJson {
    id: i32,
}

#[derive(Deserialize)]
struct ActiveJson {
    id: i32,
}

#[derive(Deserialize)]
struct MonitorJson {
    name: String,
    focused: bool,
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

/// The window classes on each workspace, keyed by workspace id. Costs one extra socket round-trip per change,
/// which is human-paced; the alternative — a second service with its own connection — would cost more.
fn query_clients(dir: &Path) -> HashMap<i32, Vec<String>> {
    let Ok(raw) = request(dir, "j/clients") else {
        return HashMap::new();
    };
    let Ok(clients) = serde_json::from_str::<Vec<ClientJson>>(&raw) else {
        return HashMap::new();
    };
    let mut by_workspace: HashMap<i32, Vec<String>> = HashMap::new();
    for client in clients {
        if client.class.is_empty() {
            continue;
        }
        by_workspace
            .entry(client.workspace.id)
            .or_default()
            .push(client.class);
    }
    by_workspace
}

/// Special workspaces are kept rather than filtered out — a scratchpad is something a bar wants to indicate —
/// and sorted after the numbered ones so their negative ids don't put them at the front.
fn query_snapshot(dir: &Path) -> Option<Snapshot> {
    let workspaces_raw = request(dir, "j/workspaces").ok()?;
    let active_raw = request(dir, "j/activeworkspace").ok()?;
    let mut clients = query_clients(dir);

    let mut workspaces: Vec<Workspace> = serde_json::from_str::<Vec<WorkspaceJson>>(&workspaces_raw)
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

static ACTIVE_WINDOW: Service<ActiveWindow> = Service::new("hyprshell-active-window", run_active_window);

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
    dispatch(dir, &format!("hl.dsp.focus({{ window = \"address:{address}\" }})"));
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

/// Switching the keyboard layout is **not available** on Hyprland's Lua config API.
///
/// `switchxkblayout` was a hyprlang dispatcher and was not carried over: as of 0.56 `hl.dsp` holds only
/// `cursor, dpms, event, exec_cmd, exec_raw, exit, focus, force_idle, force_renderer_reload, global, group,
/// layout, no_op, pass, release_input_capture, send_key_state, send_shortcut, submap, window, workspace`
/// (`hl.dsp.layout` is the *tiling* layout), and `hyprctl keyword` refuses to run under the non-legacy parser.
/// There is no device-level API either — `hl.device` returns nil for a keyboard name.
///
/// So the `kblayout` module reads the active layout and does not offer a click. If a later Hyprland restores a
/// dispatcher, wiring it back is a `dispatch()` call here plus an `.on_click(…)` in the registry — but it must
/// be verified against `hyprctl repl 'for k in pairs(hl.dsp) do … end'` first, not assumed.
pub const LAYOUT_SWITCHING_UNSUPPORTED: () = ();

#[cfg(test)]
mod tests {
    use super::*;

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
    fn monitor_from_focus_event_reads_the_name_before_the_comma() {
        assert_eq!(
            monitor_from_focus_event("focusedmon>>DP-1,3"),
            Some("DP-1".to_string())
        );
        assert_eq!(monitor_from_focus_event("workspace>>3"), None);
    }
}
