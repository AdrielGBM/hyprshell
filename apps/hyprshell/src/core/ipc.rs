//! The shell's command surface: one Unix socket, a flat `<target> <command> [args…]` protocol.
//!
//! Everything the shell can be told to do from outside — a Hyprland keybind, a script, another shell — arrives
//! here. Commands run on the driver thread, the same thread every surface lives on, so a handler can open a
//! panel or publish to a service exactly as a click handler would.
//!
//! The protocol is one request line in, one reply line out, so `hyprshell panel toggle clock` is also
//! `printf 'panel toggle clock\n' | socat - UNIX-CONNECT:$sock`. Replies are prefixed `ok` or `err` so a script
//! can branch without parsing prose.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use platform_layershell::EventSender;

use crate::core::shell;
use crate::shared::paths;
use crate::shared::services::pipewire::NodeKind;

/// How long the socket thread waits for the driver thread to answer before giving up. Long enough for a command
/// that opens a surface, short enough that a wedged UI thread doesn't hang a script forever.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// The socket's file name for a compositor instance. Keyed by the Hyprland instance signature so two
/// compositors on one login session get one socket each instead of fighting over a shared name; outside
/// Hyprland the name is still stable, so the CLI can find a shell running under any compositor.
fn socket_name(instance: Option<String>) -> String {
    let instance = instance.filter(|s| !s.is_empty());
    format!("{}.sock", instance.as_deref().unwrap_or("default"))
}

/// The IPC socket: `$XDG_RUNTIME_DIR/hyprshell/<instance>.sock`.
pub fn socket_path() -> PathBuf {
    paths::runtime_dir().join(socket_name(
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok(),
    ))
}

/// One request in flight: the raw line, and where its reply goes. The socket thread blocks on `reply` while the
/// driver thread runs the handler, which is what makes a command synchronous from the caller's point of view.
pub struct Request {
    line: String,
    reply: mpsc::Sender<String>,
}

impl Request {
    /// A request nobody is waiting on the answer to — a global shortcut, a keypress. [`handle`] still sends its
    /// reply, into a receiver that has already been dropped, which is a no-op.
    ///
    /// Constructed here rather than by making the fields public: a `Request` carries a live reply channel the
    /// socket path depends on, and the only two ways to make one should be "from a client" and "from nobody".
    pub fn unattended(line: impl Into<String>) -> Self {
        let (reply, _) = mpsc::channel();
        Self {
            line: line.into(),
            reply,
        }
    }
}

/// A single command: what it's called, how to spell its arguments (for `--list`), what it does.
struct Command {
    name: &'static str,
    args: &'static str,
    help: &'static str,
    run: fn(&[&str]) -> Result<String, String>,
}

struct Target {
    name: &'static str,
    commands: &'static [Command],
}

/// Every command the shell answers. One table, so `--list` can't drift from what actually dispatches.
static TARGETS: &[Target] = &[
    Target {
        name: "shell",
        commands: &[
            Command {
                name: "ping",
                args: "",
                help: "check the shell is alive",
                run: |_| Ok("pong".to_string()),
            },
            Command {
                name: "version",
                args: "",
                help: "the running build's version",
                run: |_| Ok(env!("CARGO_PKG_VERSION").to_string()),
            },
            Command {
                name: "reload",
                args: "",
                help: "re-read config.toml and rebuild every surface",
                run: |_| {
                    shell::request_reload();
                    Ok("reloaded".to_string())
                },
            },
            Command {
                name: "outputs",
                args: "",
                help: "list the compositor's monitors",
                run: |_| {
                    let names: Vec<String> = platform_layershell::outputs()
                        .into_iter()
                        .filter_map(|o| o.name)
                        .collect();
                    Ok(names.join("\t"))
                },
            },
            Command {
                name: "screens",
                args: "",
                help: "the compositor's monitors with mode, scale and make",
                run: |_| {
                    use crate::shared::services::hyprland;
                    let dir = hyprland::socket_dir().ok_or("not running under Hyprland")?;
                    let rows: Vec<String> = hyprland::current_screens()
                        .unwrap_or_else(|| hyprland::screens(&dir))
                        .iter()
                        .map(|s| {
                            format!(
                                "{}\t{}x{}@{:.2}\t{:.2}x\t{} {}",
                                s.name, s.width, s.height, s.refresh, s.scale, s.make, s.model
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "clients",
                args: "",
                help: "every open window: address, workspace, class and title",
                run: |_| {
                    use crate::shared::services::hyprland;
                    let dir = hyprland::socket_dir().ok_or("not running under Hyprland")?;
                    let rows: Vec<String> = hyprland::current_clients()
                        .unwrap_or_else(|| hyprland::clients(&dir))
                        .iter()
                        .map(|c| {
                            format!("{}\t{}\t{}\t{}", c.address, c.workspace, c.class, c.title)
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "dpms",
                args: "<on|off>",
                help: "switch every monitor's output on or off",
                run: |args| {
                    use crate::shared::services::hyprland;
                    let on = match arg(args, 0, "state")? {
                        "on" => true,
                        "off" => false,
                        other => return Err(format!("expected on|off, got '{other}'")),
                    };
                    let dir = hyprland::socket_dir().ok_or("not running under Hyprland")?;
                    if hyprland::set_dpms(&dir, on) {
                        Ok(on_off(on).to_string())
                    } else {
                        Err("the compositor did not change its DPMS state".to_string())
                    }
                },
            },
            Command {
                name: "quit",
                args: "",
                help: "shut the shell down",
                run: |_| {
                    shell::request_quit();
                    Ok("bye".to_string())
                },
            },
        ],
    },
    Target {
        name: "lock",
        commands: &[
            Command {
                name: "on",
                args: "",
                help: "lock the session",
                run: |_| {
                    use crate::shared::services::lock;
                    // Refused up front rather than after the screen is covered: a lock this shell cannot
                    // undo is the one failure the user has no way out of.
                    lock::can_lock()?;
                    lock::lock();
                    Ok("locking".to_string())
                },
            },
            Command {
                name: "off",
                args: "",
                help: "unlock the session",
                run: |_| {
                    crate::shared::services::lock::unlock();
                    Ok("unlocking".to_string())
                },
            },
            Command {
                name: "toggle",
                args: "",
                help: "lock the session, or unlock it if it is locked",
                run: |_| {
                    use crate::shared::services::lock;
                    if lock::current().wanted {
                        lock::unlock();
                        Ok("unlocking".to_string())
                    } else {
                        lock::can_lock()?;
                        lock::lock();
                        Ok("locking".to_string())
                    }
                },
            },
            Command {
                name: "status",
                args: "",
                help: "whether the session is locked, and whether this machine can lock at all",
                run: |_| {
                    use crate::shared::services::lock;
                    let state = lock::current();
                    let supported = match lock::can_lock() {
                        Ok(()) => "supported".to_string(),
                        Err(reason) => reason,
                    };
                    // Three columns because the middle one is the honest answer: between asking and being
                    // granted, the desktop may still be on screen.
                    Ok(format!(
                        "{}\t{}\t{supported}",
                        on_off(state.locked),
                        on_off(state.wanted)
                    ))
                },
            },
        ],
    },
    Target {
        name: "idle",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "whether the idle timers are armed, and what is holding them off",
                run: |_| {
                    use crate::shared::services::idle;
                    let config = shell::config().map(|c| c.idle.clone()).unwrap_or_default();
                    let held = idle::inhibited_by(&config).map(|r| r.id()).unwrap_or("-");
                    Ok(format!("{}\t{held}", on_off(config.enabled)))
                },
            },
            Command {
                name: "inhibit",
                args: "<on|off|toggle>",
                help: "hold the idle timers off by hand",
                run: |args| {
                    use crate::shared::services::idle;
                    match arg(args, 0, "state")? {
                        "on" => idle::set_manual_inhibit(true),
                        "off" => idle::set_manual_inhibit(false),
                        "toggle" => idle::toggle_manual_inhibit(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok(on_off(idle::manual_inhibit()).to_string())
                },
            },
        ],
    },
    Target {
        name: "panel",
        commands: &[
            Command {
                name: "toggle",
                args: "<module>",
                help: "open a module's panel, or close it if it is up",
                run: |args| {
                    let module = arg(args, 0, "module")?;
                    crate::toggle_panel(module);
                    Ok(module.to_string())
                },
            },
            Command {
                name: "open",
                args: "<module>",
                help: "open a module's panel (idempotent)",
                run: |args| {
                    let module = arg(args, 0, "module")?;
                    crate::modules::panel::open_panel(module);
                    Ok(module.to_string())
                },
            },
            Command {
                name: "close",
                args: "<module>",
                help: "close a module's panel",
                run: |args| {
                    let module = arg(args, 0, "module")?;
                    crate::modules::panel::close_panel(module);
                    Ok(module.to_string())
                },
            },
            Command {
                name: "list",
                args: "",
                help: "which panels are open right now",
                run: |_| Ok(shell::open_ids().join("\t")),
            },
        ],
    },
    Target {
        name: "launcher",
        commands: &[
            Command {
                name: "toggle",
                args: "",
                help: "open the application launcher, or close it if it is up",
                run: |_| {
                    crate::modules::launcher::toggle();
                    Ok("toggled".to_string())
                },
            },
            Command {
                name: "close",
                args: "",
                help: "close the launcher",
                run: |_| {
                    crate::core::shell::close(crate::modules::launcher::ID);
                    Ok("closed".to_string())
                },
            },
        ],
    },
    Target {
        name: "audio",
        commands: &[
            Command {
                name: "sinks",
                args: "",
                help: "output devices: id, level, mute, and which is the default",
                run: |_| Ok(list_nodes(NodeKind::Sink)),
            },
            Command {
                name: "sources",
                args: "",
                help: "input devices: id, level, mute, and which is the default",
                run: |_| Ok(list_nodes(NodeKind::Source)),
            },
            Command {
                name: "streams",
                args: "",
                help: "applications playing audio, with their own level",
                run: |_| Ok(list_nodes(NodeKind::OutputStream)),
            },
            Command {
                name: "default",
                args: "<id>",
                help: "make a device the default sink or source",
                run: |args| {
                    let id = node_id(args)?;
                    crate::shared::services::volume::set_default(id);
                    Ok(id.to_string())
                },
            },
            Command {
                name: "set",
                args: "<id> <percent>",
                help: "set one device's or application's level",
                run: |args| {
                    let id = node_id(args)?;
                    let level = number(args, 1, "percent")?;
                    crate::shared::services::volume::set_node(id, level);
                    Ok(level.to_string())
                },
            },
            Command {
                name: "mute",
                args: "<id>",
                help: "toggle one device's or application's mute",
                run: |args| {
                    let id = node_id(args)?;
                    crate::shared::services::volume::toggle_node_mute(id);
                    Ok(id.to_string())
                },
            },
        ],
    },
    Target {
        name: "dashboard",
        commands: &[
            Command {
                name: "toggle",
                args: "",
                help: "open the dashboard, or close it if it is up",
                run: |_| {
                    crate::toggle_panel(crate::modules::dashboard::ID);
                    Ok("toggled".to_string())
                },
            },
            Command {
                name: "open",
                args: "[tab]",
                help: "open the dashboard, optionally on a named page",
                run: |args| {
                    if let Some(name) = args.first() {
                        set_dashboard_tab(name)?;
                    }
                    crate::modules::panel::open_panel(crate::modules::dashboard::ID);
                    Ok(crate::modules::dashboard::tab().id().to_string())
                },
            },
            Command {
                name: "close",
                args: "",
                help: "close the dashboard",
                run: |_| {
                    crate::modules::panel::close_panel(crate::modules::dashboard::ID);
                    Ok("closed".to_string())
                },
            },
            Command {
                name: "tab",
                args: "[dash|media|performance|weather]",
                help: "read or switch the page the dashboard shows",
                run: |args| {
                    if let Some(name) = args.first() {
                        set_dashboard_tab(name)?;
                    }
                    Ok(crate::modules::dashboard::tab().id().to_string())
                },
            },
        ],
    },
    Target {
        name: "apps",
        commands: &[
            Command {
                name: "count",
                args: "",
                help: "how many desktop entries are known",
                run: |_| Ok(crate::shared::services::apps::all().len().to_string()),
            },
            Command {
                name: "reload",
                args: "",
                help: "re-scan the application directories",
                run: |_| Ok(crate::shared::services::apps::reload().to_string()),
            },
            Command {
                name: "search",
                args: "<query>",
                help: "the launcher's ranking for a query, best first",
                run: |args| {
                    use crate::modules::launcher;
                    let query = args.join(" ");
                    let config = crate::core::shell::config()
                        .map(|c| c.launcher.clone())
                        .unwrap_or_default();
                    let names: Vec<String> = launcher::results(
                        crate::shared::services::apps::all(),
                        &query,
                        &config,
                    )
                    .into_iter()
                    .map(|a| a.name)
                    .collect();
                    Ok(names.join("\t"))
                },
            },
        ],
    },
    Target {
        name: "notifs",
        commands: &[
            Command {
                name: "clear",
                args: "[app]",
                help: "drop one application's notifications, or the whole history",
                run: |args| {
                    use crate::shared::services::notifications as notifs;
                    let app = args.join(" ");
                    match app.trim() {
                        "" => notifs::clear_all(),
                        app => notifs::clear_app(app),
                    }
                    Ok("cleared".to_string())
                },
            },
            Command {
                name: "mute",
                args: "<app> [on|off|toggle]",
                help: "read or set whether an application's notifications may pop",
                run: |args| {
                    use crate::shared::services::notifications as notifs;
                    let (app, rest) = args.split_first().ok_or("missing argument <app>")?;
                    let current = notifs::is_app_muted(app);
                    let next = match rest.first().copied() {
                        None => return Ok(on_off(current).to_string()),
                        Some("on") => true,
                        Some("off") => false,
                        Some("toggle") => !current,
                        Some(other) => return Err(format!("expected on|off|toggle, got '{other}'")),
                    };
                    notifs::set_app_muted(app, next);
                    Ok(on_off(next).to_string())
                },
            },
            Command {
                name: "muted",
                args: "",
                help: "the applications whose notifications are muted",
                run: |_| {
                    let muted = crate::shared::services::notifications::snapshot_now()
                        .map(|s| s.muted_apps.clone())
                        .unwrap_or_default();
                    Ok(muted.join("\t"))
                },
            },
            Command {
                name: "dnd",
                args: "<on|off|toggle>",
                help: "read or set do-not-disturb",
                run: |args| {
                    let current = crate::shared::services::notifications::snapshot_now()
                        .map(|s| s.dnd)
                        .unwrap_or(false);
                    let next = match args.first().copied() {
                        None => return Ok(on_off(current).to_string()),
                        Some("on") => true,
                        Some("off") => false,
                        Some("toggle") => !current,
                        Some(other) => return Err(format!("expected on|off|toggle, got '{other}'")),
                    };
                    crate::shared::services::notifications::set_dnd(next);
                    Ok(on_off(next).to_string())
                },
            },
            Command {
                name: "center",
                args: "[open|close|toggle]",
                help: "the notification centre: history and quick toggles on a full-height surface",
                run: |args| {
                    use crate::modules::sidebar;
                    match args.first().copied().unwrap_or("toggle") {
                        "open" => sidebar::open(),
                        "close" => sidebar::close(),
                        "toggle" => sidebar::toggle(),
                        other => {
                            return Err(format!("expected open|close|toggle, got '{other}'"));
                        }
                    }
                    Ok(on_off(sidebar::is_open()).to_string())
                },
            },
        ],
    },
    Target {
        name: "screenshot",
        commands: &[
            Command {
                name: "screen",
                args: "",
                help: "capture every monitor, composed into one image",
                run: |_| {
                    use crate::shared::services::screenshot::Target;
                    crate::modules::capture::screenshot(Target::Screen);
                    Ok("capturing".to_string())
                },
            },
            Command {
                name: "output",
                args: "[name]",
                help: "capture one monitor, the focused one by default",
                run: |args| {
                    use crate::shared::services::screenshot::Target;
                    match reading_output(args.first().copied())? {
                        Some(name) => crate::modules::capture::screenshot(Target::Output(name)),
                        None => crate::modules::capture::screenshot(Target::Screen),
                    }
                    Ok("capturing".to_string())
                },
            },
            Command {
                name: "region",
                args: "",
                help: "pick a region with the pointer, then capture it",
                run: |_| {
                    crate::modules::capture::screenshot_region();
                    Ok("picking".to_string())
                },
            },
            Command {
                name: "cancel",
                args: "",
                help: "close the region picker without capturing",
                run: |_| {
                    crate::modules::capture::close_picker();
                    Ok("cancelled".to_string())
                },
            },
            Command {
                name: "last",
                args: "",
                help: "where the last capture went, or why it failed",
                run: |_| {
                    use crate::shared::services::screenshot;
                    match screenshot::current() {
                        Some(Ok(shot)) => Ok(match shot.path {
                            Some(path) => path.display().to_string(),
                            None => "clipboard".to_string(),
                        }),
                        Some(Err(reason)) => Err(reason),
                        None => Ok(String::new()),
                    }
                },
            },
        ],
    },
    Target {
        name: "record",
        commands: &[
            Command {
                name: "start",
                args: "[screen|output|region]",
                help: "start recording; a region opens the picker first",
                run: |args| {
                    match args.first().copied().unwrap_or("screen") {
                        "screen" => crate::modules::capture::record_screen(),
                        "output" => crate::modules::capture::record_output(),
                        "region" => crate::modules::capture::record_region(),
                        other => {
                            return Err(format!("expected screen|output|region, got '{other}'"));
                        }
                    }
                    Ok("recording".to_string())
                },
            },
            Command {
                name: "stop",
                args: "",
                help: "stop the recording, letting the encoder close its file",
                run: |_| {
                    crate::shared::services::recorder::stop();
                    Ok("stopping".to_string())
                },
            },
            Command {
                name: "toggle",
                args: "",
                help: "stop the recording, or start one of the whole screen",
                run: |_| {
                    crate::modules::capture::toggle_recording();
                    Ok("toggled".to_string())
                },
            },
            Command {
                name: "pause",
                args: "",
                help: "suspend or resume the recording, on a backend that can",
                run: |_| {
                    let paused = crate::shared::services::recorder::toggle_pause()?;
                    Ok(on_off(paused).to_string())
                },
            },
            Command {
                name: "status",
                args: "",
                help: "whether something is being recorded, for how long, and where",
                run: |_| {
                    use crate::shared::services::recorder;
                    let state = recorder::current();
                    let backend = state
                        .backend
                        .map(|backend| backend.program().to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let file = state
                        .path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_default();
                    // Four columns, most useful first, so a bar script can cut one out.
                    Ok(format!(
                        "{}\t{}\t{backend}\t{file}",
                        on_off(state.active),
                        recorder::format_elapsed(state.elapsed())
                    ))
                },
            },
            Command {
                name: "list",
                args: "",
                help: "the recordings, newest first",
                run: |_| {
                    use crate::shared::services::recorder;
                    let config = shell::config().ok_or("the shell is not running")?;
                    let rows: Vec<String> = recorder::recordings(
                        &config.recordings_dir(),
                        config.recorder.entries(),
                    )
                    .into_iter()
                    .map(|entry| format!("{}\t{}", entry.size_label(), entry.path.display()))
                    .collect();
                    Ok(rows.join("\n"))
                },
            },
        ],
    },
    Target {
        name: "toast",
        commands: &[
            Command {
                name: "show",
                args: "<text…>",
                help: "show an in-shell toast, for a script that wants to say something",
                run: |args| {
                    use crate::shared::services::toaster::{self, Event};
                    let text = args.join(" ");
                    if text.trim().is_empty() {
                        return Err("missing argument <text>".to_string());
                    }
                    // Under the config-reload event, which is the one that means "the shell itself is talking";
                    // a script's toast should be switchable off by the same key.
                    toaster::post(Event::ConfigLoaded, "info", text.clone(), String::new());
                    Ok(text)
                },
            },
            Command {
                name: "clear",
                args: "",
                help: "dismiss every toast on screen",
                run: |_| {
                    crate::shared::services::toaster::clear();
                    Ok("cleared".to_string())
                },
            },
        ],
    },
    Target {
        name: "volume",
        commands: &[
            Command {
                name: "get",
                args: "",
                help: "the default sink's level, and whether it is muted",
                run: |_| {
                    let v = crate::shared::services::volume::current()
                        .ok_or("no audio sink available")?;
                    Ok(format!("{} {}", v.level, on_off(v.muted)))
                },
            },
            Command {
                name: "set",
                args: "<percent>",
                help: "set the default sink's level",
                run: |args| {
                    let level = number(args, 0, "percent")?;
                    crate::shared::services::volume::set(level);
                    crate::modules::osd::show_volume();
                    Ok(level.to_string())
                },
            },
            Command {
                name: "step",
                args: "<±percent>",
                help: "move the level by a delta",
                run: |args| {
                    let delta = number(args, 0, "delta")?;
                    crate::shared::services::volume::step(delta);
                    crate::modules::osd::show_volume();
                    Ok(delta.to_string())
                },
            },
            Command {
                name: "up",
                args: "",
                help: "raise the level by [audio] increment",
                run: |_| {
                    let step = crate::shared::services::volume::settings().step();
                    crate::shared::services::volume::step(step);
                    crate::modules::osd::show_volume();
                    Ok(step.to_string())
                },
            },
            Command {
                name: "down",
                args: "",
                help: "lower the level by [audio] increment",
                run: |_| {
                    let step = crate::shared::services::volume::settings().step();
                    crate::shared::services::volume::step(-step);
                    crate::modules::osd::show_volume();
                    Ok((-step).to_string())
                },
            },
            Command {
                name: "mute",
                args: "",
                help: "toggle mute on the default sink",
                run: |_| {
                    crate::shared::services::volume::toggle_mute();
                    crate::modules::osd::show_volume();
                    Ok("toggled".to_string())
                },
            },
        ],
    },
    Target {
        name: "mic",
        commands: &[
            Command {
                name: "get",
                args: "",
                help: "the default source's level, and whether it is muted",
                run: |_| {
                    let v = crate::shared::services::volume::current_mic()
                        .ok_or("no audio source available")?;
                    Ok(format!("{} {}", v.level, on_off(v.muted)))
                },
            },
            Command {
                name: "set",
                args: "<percent>",
                help: "set the default source's level",
                run: |args| {
                    let level = number(args, 0, "percent")?;
                    crate::shared::services::volume::set_mic(level);
                    crate::modules::osd::show_microphone();
                    Ok(level.to_string())
                },
            },
            Command {
                name: "step",
                args: "<±percent>",
                help: "move the source level by a delta",
                run: |args| {
                    let delta = number(args, 0, "delta")?;
                    crate::shared::services::volume::step_mic(delta);
                    crate::modules::osd::show_microphone();
                    Ok(delta.to_string())
                },
            },
            Command {
                name: "up",
                args: "",
                help: "raise the source level by [audio] increment",
                run: |_| {
                    let step = crate::shared::services::volume::settings().step();
                    crate::shared::services::volume::step_mic(step);
                    crate::modules::osd::show_microphone();
                    Ok(step.to_string())
                },
            },
            Command {
                name: "down",
                args: "",
                help: "lower the source level by [audio] increment",
                run: |_| {
                    let step = crate::shared::services::volume::settings().step();
                    crate::shared::services::volume::step_mic(-step);
                    crate::modules::osd::show_microphone();
                    Ok((-step).to_string())
                },
            },
            Command {
                name: "mute",
                args: "",
                help: "toggle mute on the default source",
                run: |_| {
                    crate::modules::osd::mic_action();
                    Ok("toggled".to_string())
                },
            },
        ],
    },
    Target {
        name: "weather",
        commands: &[
            Command {
                name: "now",
                args: "",
                help: "the current conditions: place, temperature and sky",
                run: |_| {
                    use crate::shared::services::weather;
                    let w = weather::current().ok_or("no weather reading yet")?;
                    Ok(format!(
                        "{}\t{:.1}\t{}",
                        w.place,
                        w.temperature,
                        w.condition().id()
                    ))
                },
            },
            Command {
                name: "forecast",
                args: "",
                help: "one line per day: date, high, low and chance of rain",
                run: |_| {
                    use crate::shared::services::weather;
                    let w = weather::current().ok_or("no weather reading yet")?;
                    let rows: Vec<String> = w
                        .days
                        .iter()
                        .map(|d| {
                            format!(
                                "{}\t{:.0}\t{:.0}\t{}%",
                                d.date, d.high, d.low, d.precipitation
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
        ],
    },
    Target {
        name: "gamemode",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "whether GameMode is active, how many clients hold it, and whether the shell is one",
                run: |_| {
                    use crate::shared::services::gamemode;
                    let state = gamemode::current().filter(|s| s.available);
                    let state = state.ok_or("gamemoded is not running")?;
                    Ok(format!(
                        "{}\t{}\t{}",
                        on_off(state.active),
                        state.clients,
                        on_off(state.held)
                    ))
                },
            },
            Command {
                name: "set",
                args: "<on|off|toggle>",
                help: "hold GameMode from the shell, or drop the shell's hold",
                run: |args| {
                    use crate::shared::services::gamemode;
                    match arg(args, 0, "state")? {
                        "on" => gamemode::set_held(true),
                        "off" => gamemode::set_held(false),
                        "toggle" => gamemode::toggle(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok("ok".to_string())
                },
            },
        ],
    },
    Target {
        name: "wifi",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "the radio state, the network joined and its signal",
                run: |_| {
                    use crate::shared::services::network;
                    // `enabled = false` and "no NetworkManager" both yield nothing here, and telling a user the
                    // daemon is missing when they switched the section off themselves sends them hunting.
                    let wifi = network::current_wifi()
                        .ok_or("[network] enabled is false, or NetworkManager is not running")?;
                    let wifi = if wifi.available {
                        wifi
                    } else {
                        return Err("NetworkManager is not running".to_string());
                    };
                    let (ssid, strength) = match wifi.active() {
                        Some(point) => (point.ssid.clone(), point.strength),
                        None => (String::new(), 0),
                    };
                    Ok(format!("{}\t{ssid}\t{strength}", on_off(wifi.enabled)))
                },
            },
            Command {
                name: "list",
                args: "",
                help: "networks in range: ssid, signal, security, saved",
                run: |_| {
                    use crate::shared::services::network;
                    let wifi = network::current_wifi().ok_or("NetworkManager is not running")?;
                    let rows: Vec<String> = wifi
                        .networks()
                        .iter()
                        .map(|p| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                p.ssid,
                                p.strength,
                                p.security.id(),
                                if p.saved { "saved" } else { "new" }
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "radio",
                args: "<on|off|toggle>",
                help: "switch the wireless radio",
                run: |args| {
                    use crate::shared::services::network;
                    match arg(args, 0, "state")? {
                        "on" => network::set_wifi_enabled(true),
                        "off" => network::set_wifi_enabled(false),
                        "toggle" => network::toggle_wifi(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok("ok".to_string())
                },
            },
            Command {
                name: "scan",
                args: "",
                help: "look for networks now",
                run: |_| {
                    crate::shared::services::network::request_scan();
                    Ok("scanning".to_string())
                },
            },
            Command {
                name: "connect",
                args: "<ssid> [password]",
                help: "join a network; the password is only needed the first time",
                run: |args| {
                    use crate::shared::services::network;
                    let ssid = arg(args, 0, "ssid")?;
                    let wifi = network::current_wifi().ok_or("NetworkManager is not running")?;
                    let point = wifi
                        .networks()
                        .into_iter()
                        .find(|p| p.ssid == ssid)
                        .ok_or_else(|| format!("'{ssid}' is not in range"))?;
                    network::connect(&point.path, args.get(1).map(|p| p.to_string()));
                    Ok(ssid.to_string())
                },
            },
            Command {
                name: "disconnect",
                args: "",
                help: "leave the current network, keeping it saved",
                run: |_| {
                    crate::shared::services::network::disconnect();
                    Ok("ok".to_string())
                },
            },
            Command {
                name: "forget",
                args: "<ssid>",
                help: "delete a saved network",
                run: |args| {
                    let ssid = arg(args, 0, "ssid")?;
                    crate::shared::services::network::forget(ssid);
                    Ok(ssid.to_string())
                },
            },
        ],
    },
    Target {
        name: "vpn",
        commands: &[
            Command {
                name: "list",
                args: "",
                help: "every tunnel: id, state, kind and name",
                run: |_| {
                    use crate::shared::services::vpn;
                    let state = vpn::current().ok_or("no VPN service")?;
                    let rows: Vec<String> = state
                        .tunnels
                        .iter()
                        .map(|t| {
                            format!(
                                "{}\t{}\t{}\t{}",
                                t.id,
                                if t.active { "up" } else { "down" },
                                t.kind,
                                t.name
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "up",
                args: "<id>",
                help: "bring a tunnel up",
                run: |args| {
                    let id = arg(args, 0, "id")?;
                    crate::shared::services::vpn::set_active(id, true);
                    Ok(id.to_string())
                },
            },
            Command {
                name: "down",
                args: "<id>",
                help: "bring a tunnel down",
                run: |args| {
                    let id = arg(args, 0, "id")?;
                    crate::shared::services::vpn::set_active(id, false);
                    Ok(id.to_string())
                },
            },
            Command {
                name: "toggle",
                args: "",
                help: "drop the active tunnel, or raise the first configured one",
                run: |_| {
                    crate::shared::services::vpn::toggle();
                    Ok("ok".to_string())
                },
            },
        ],
    },
    Target {
        name: "bluetooth",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "the adapter's power, scan state and how many devices are connected",
                run: |_| {
                    use crate::shared::services::bluetooth;
                    let bt = bluetooth::current().filter(|bt| bt.available);
                    let bt = bt.ok_or("no bluetooth adapter")?;
                    Ok(format!(
                        "{}\t{}\t{}",
                        on_off(bt.powered),
                        on_off(bt.discovering),
                        bt.connected_count()
                    ))
                },
            },
            Command {
                name: "devices",
                args: "",
                help: "every known device: path, state and name",
                run: |_| {
                    use crate::shared::services::bluetooth;
                    let bt = bluetooth::current().ok_or("no bluetooth adapter")?;
                    let rows: Vec<String> = bt
                        .devices
                        .iter()
                        .map(|d| {
                            let state = if d.connected {
                                "connected"
                            } else if d.paired {
                                "paired"
                            } else {
                                "available"
                            };
                            format!("{}\t{}\t{}", d.path, state, d.label())
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "power",
                args: "<on|off|toggle>",
                help: "switch the adapter on or off",
                run: |args| {
                    use crate::shared::services::bluetooth;
                    match arg(args, 0, "state")? {
                        "on" => bluetooth::set_powered(true),
                        "off" => bluetooth::set_powered(false),
                        "toggle" => bluetooth::toggle_powered(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok("ok".to_string())
                },
            },
            Command {
                name: "scan",
                args: "<on|off|toggle>",
                help: "start or stop looking for devices (a scan stops itself)",
                run: |args| {
                    use crate::shared::services::bluetooth;
                    match arg(args, 0, "state")? {
                        "on" => bluetooth::set_discovering(true),
                        "off" => bluetooth::set_discovering(false),
                        "toggle" => bluetooth::toggle_discovering(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok("ok".to_string())
                },
            },
            Command {
                name: "connect",
                args: "<device-path>",
                help: "connect a device, pairing it first if it is new",
                run: |args| {
                    let path = arg(args, 0, "device-path")?;
                    crate::shared::services::bluetooth::connect(path);
                    Ok(path.to_string())
                },
            },
            Command {
                name: "disconnect",
                args: "<device-path>",
                help: "disconnect a device",
                run: |args| {
                    let path = arg(args, 0, "device-path")?;
                    crate::shared::services::bluetooth::disconnect(path);
                    Ok(path.to_string())
                },
            },
            Command {
                name: "forget",
                args: "<device-path>",
                help: "remove a pairing entirely",
                run: |args| {
                    let path = arg(args, 0, "device-path")?;
                    crate::shared::services::bluetooth::forget(path);
                    Ok(path.to_string())
                },
            },
        ],
    },
    Target {
        name: "media",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "the active player, its state and what it is playing",
                run: |_| {
                    use crate::shared::services::mpris;
                    let p = mpris::current().ok_or("no media player is running")?;
                    Ok(format!("{:?}\t{}\t{}", p.playback, p.identity, p.summary()))
                },
            },
            Command {
                name: "get",
                args: "<title|artist|album|player|status|art>",
                help: "one field of the active player",
                run: |args| {
                    use crate::shared::services::mpris;
                    let field = arg(args, 0, "field")?;
                    let p = mpris::current().ok_or("no media player is running")?;
                    Ok(match field {
                        "title" => p.title,
                        "artist" => p.artist,
                        "album" => p.album,
                        "player" => p.identity,
                        "status" => format!("{:?}", p.playback),
                        "art" => p.art_url,
                        other => return Err(format!("unknown field '{other}'")),
                    })
                },
            },
            Command {
                name: "play-pause",
                args: "",
                help: "toggle playback on the active player",
                run: |_| {
                    crate::shared::services::mpris::play_pause();
                    Ok("toggled".to_string())
                },
            },
            Command {
                name: "next",
                args: "",
                help: "skip to the next track",
                run: |_| {
                    crate::shared::services::mpris::next();
                    Ok("next".to_string())
                },
            },
            Command {
                name: "previous",
                args: "",
                help: "skip to the previous track",
                run: |_| {
                    crate::shared::services::mpris::previous();
                    Ok("previous".to_string())
                },
            },
            Command {
                name: "stop",
                args: "",
                help: "stop the active player",
                run: |_| {
                    crate::shared::services::mpris::stop();
                    Ok("stopped".to_string())
                },
            },
            Command {
                name: "seek",
                args: "<±seconds>",
                help: "move the playhead, if the player can seek",
                run: |args| {
                    use crate::shared::services::mpris;
                    let seconds = number(args, 0, "seconds")?;
                    let player = mpris::current().ok_or("no media player is running")?;
                    if !player.can_seek {
                        return Err(format!("{} cannot seek", player.identity));
                    }
                    mpris::seek(seconds as i64 * 1_000_000);
                    Ok(seconds.to_string())
                },
            },
            Command {
                name: "shuffle",
                args: "<on|off|toggle>",
                help: "set the shuffle state",
                run: |args| {
                    use crate::shared::services::mpris;
                    match arg(args, 0, "state")? {
                        "on" => mpris::set_shuffle(true),
                        "off" => mpris::set_shuffle(false),
                        "toggle" => mpris::toggle_shuffle(),
                        other => return Err(format!("expected on|off|toggle, got '{other}'")),
                    }
                    Ok("ok".to_string())
                },
            },
            Command {
                name: "loop",
                args: "<off|track|playlist|cycle>",
                help: "set the repeat mode",
                run: |args| {
                    use crate::shared::services::mpris::{self, LoopStatus};
                    match arg(args, 0, "mode")? {
                        "off" | "none" => mpris::set_loop(LoopStatus::Off),
                        "track" => mpris::set_loop(LoopStatus::Track),
                        "playlist" => mpris::set_loop(LoopStatus::Playlist),
                        "cycle" => mpris::cycle_loop(),
                        other => {
                            return Err(format!(
                                "expected off|track|playlist|cycle, got '{other}'"
                            ));
                        }
                    }
                    Ok("ok".to_string())
                },
            },
        ],
    },
    Target {
        name: "session",
        commands: &[
            Command {
                name: "list",
                args: "",
                help: "which session actions this machine supports",
                run: |_| {
                    let ids: Vec<&str> = crate::shared::services::session::available()
                        .iter()
                        .map(|a| a.id())
                        .collect();
                    Ok(ids.join("\t"))
                },
            },
            Command {
                name: "do",
                args: "<lock|logout|suspend|hibernate|reboot|shutdown>",
                help: "perform a session action",
                run: |args| {
                    use crate::shared::services::session;
                    let id = arg(args, 0, "action")?;
                    let action = session::Action::from_id(id)
                        .ok_or_else(|| format!("unknown session action '{id}'"))?;
                    if !session::is_available(action) {
                        return Err(format!("this machine cannot '{id}'"));
                    }
                    session::perform(action);
                    Ok(id.to_string())
                },
            },
        ],
    },
    // No `next`: Hyprland's Lua API has no keyboard-layout dispatcher to call
    // (`hyprland::LAYOUT_SWITCHING_UNSUPPORTED`), and advertising a command that always errors is worse than
    // not having one.
    Target {
        name: "keyboard",
        commands: &[Command {
            name: "layout",
            args: "",
            help: "the main keyboard's active layout",
            run: |_| {
                use crate::shared::services::hyprland;
                let layout = hyprland::socket_dir()
                    .and_then(|dir| hyprland::keyboard_layout(&dir))
                    .ok_or("no keyboard reported")?;
                Ok(layout.name)
            },
        }],
    },
    Target {
        name: "brightness",
        commands: &[
            Command {
                name: "get",
                args: "[output]",
                help: "the brightness of a screen (no output means the primary one)",
                run: |args| {
                    use crate::shared::services::brightness;
                    let level = match args.first().copied() {
                        Some(output) => {
                            let output = dimmable_output(output)?;
                            brightness::current_output(&output)
                                .ok_or_else(|| format!("'{output}' reports no brightness"))?
                        }
                        None => brightness::current().ok_or("no controllable display")?,
                    };
                    Ok(level.to_string())
                },
            },
            Command {
                name: "refresh",
                args: "",
                help: "detect displays again, for a monitor plugged in since startup",
                run: |_| {
                    crate::shared::services::brightness::refresh();
                    Ok("detecting".to_string())
                },
            },
            Command {
                name: "list",
                args: "",
                help: "every controllable display: output, level, kind and label",
                run: |_| {
                    use crate::shared::services::brightness::Kind;
                    let rows: Vec<String> = crate::shared::services::brightness::snapshot()
                        .displays
                        .iter()
                        .map(|display| {
                            let kind = match display.kind {
                                Kind::Internal { .. } => "internal",
                                Kind::External { .. } => "external",
                            };
                            format!(
                                "{}\t{}\t{kind}\t{}",
                                display.output, display.level, display.label
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "set",
                args: "<percent> [output|all]",
                help: "set the brightness of a screen (no output means the primary one)",
                run: |args| {
                    let level = number(args, 0, "percent")?;
                    for output in dimmable_targets(args.get(1).copied())? {
                        crate::shared::services::brightness::set_output(&output, level);
                    }
                    crate::modules::osd::show_brightness();
                    // The applied value, not the requested one: `set 150` puts the panel at 100, and a script that
                    // reads the reply back is owed the level the screen is actually at.
                    Ok(level.clamp(0, 100).to_string())
                },
            },
            Command {
                name: "step",
                args: "<±percent> [output|all]",
                help: "move a screen's brightness by a delta",
                run: |args| {
                    let delta = number(args, 0, "delta")?;
                    for output in dimmable_targets(args.get(1).copied())? {
                        crate::shared::services::brightness::step_output(&output, delta);
                    }
                    crate::modules::osd::show_brightness();
                    Ok(delta.to_string())
                },
            },
            Command {
                name: "up",
                args: "[output|all]",
                help: "raise the brightness by [brightness] increment",
                run: |args| {
                    let step = crate::shared::services::brightness::settings().step();
                    for output in dimmable_targets(args.first().copied())? {
                        crate::shared::services::brightness::step_output(&output, step);
                    }
                    crate::modules::osd::show_brightness();
                    Ok(step.to_string())
                },
            },
            Command {
                name: "down",
                args: "[output|all]",
                help: "lower the brightness by [brightness] increment",
                run: |args| {
                    let step = crate::shared::services::brightness::settings().step();
                    for output in dimmable_targets(args.first().copied())? {
                        crate::shared::services::brightness::step_output(&output, -step);
                    }
                    crate::modules::osd::show_brightness();
                    Ok((-step).to_string())
                },
            },
        ],
    },
    Target {
        name: "wallpaper",
        commands: &[
            Command {
                name: "get",
                args: "[output]",
                help: "the image a screen is showing (no output means the focused one)",
                run: |args| {
                    use crate::shared::services::wallpaper;
                    let config = shell::config().ok_or("the shell is not running")?;
                    let output = reading_output(args.first().copied())?;
                    wallpaper::current_image(&config, output.as_deref())
                        .map(|path| path.display().to_string())
                        .ok_or_else(|| "no wallpaper is set".to_string())
                },
            },
            Command {
                name: "list",
                args: "",
                help: "every image in the library: folder, name and path",
                run: |_| {
                    let rows: Vec<String> = crate::shared::services::wallpaper::all()
                        .iter()
                        .map(|entry| {
                            format!(
                                "{}\t{}\t{}",
                                if entry.folder.is_empty() { "-" } else { &entry.folder },
                                entry.name,
                                entry.path.display()
                            )
                        })
                        .collect();
                    Ok(rows.join("\n"))
                },
            },
            Command {
                name: "reload",
                args: "",
                help: "re-scan the wallpaper folder",
                run: |_| Ok(crate::shared::services::wallpaper::reload().to_string()),
            },
            Command {
                name: "set",
                args: "<path> [output]",
                help: "put an image on every screen, or on one of them",
                run: |args| {
                    use crate::shared::services::wallpaper;
                    let path = crate::shared::paths::expand_tilde(std::path::Path::new(arg(
                        args, 0, "path",
                    )?));
                    // Checked here rather than left to the surface: a `set` that answered `ok` and changed
                    // nothing because the file is gone is the one reply a script cannot act on.
                    if !path.is_file() {
                        return Err(format!("'{}' is not a file", path.display()));
                    }
                    wallpaper::set(&path, target_output(args.get(1).copied())?.as_deref());
                    refresh_scheme();
                    Ok(path.display().to_string())
                },
            },
            Command {
                name: "random",
                args: "[output]",
                help: "pick one from the library at random",
                run: |args| {
                    use crate::shared::services::wallpaper;
                    let config = shell::config().ok_or("the shell is not running")?;
                    let output = target_output(args.first().copied())?;
                    let showing = wallpaper::current_image(&config, output.as_deref());
                    // Named, because the folder is the thing that is wrong nine times out of ten — it defaults
                    // to `$XDG_PICTURES_DIR/Wallpapers` and a user whose collection is one directory over has
                    // no way to tell an empty folder from the wrong one.
                    let picked = wallpaper::random(showing.as_deref()).ok_or_else(|| {
                        format!(
                            "no images in {} (set [paths] wallpapers)",
                            config.wallpaper_dir().display()
                        )
                    })?;
                    wallpaper::set(&picked, output.as_deref());
                    refresh_scheme();
                    Ok(picked.display().to_string())
                },
            },
            Command {
                name: "clear",
                args: "[output]",
                help: "drop the runtime choice, putting [background] back in charge",
                run: |args| {
                    let output = target_output(args.first().copied())?;
                    crate::shared::services::wallpaper::clear(output.as_deref());
                    refresh_scheme();
                    Ok("cleared".to_string())
                },
            },
        ],
    },
    Target {
        name: "scheme",
        commands: &[
            Command {
                name: "status",
                args: "",
                help: "the palette in use: theme, mode, variant and the wallpaper it came from",
                run: |_| {
                    use crate::shared::scheme;
                    let config = shell::config().ok_or("the shell is not running")?;
                    let (mode, variant) = config.scheme_selection();
                    let source = match scheme::current() {
                        Some(current) => current.source.display().to_string(),
                        None => "-".to_string(),
                    };
                    Ok(format!(
                        "{}\t{}\t{}\t{source}",
                        config.theme.name,
                        mode.id(),
                        variant.id()
                    ))
                },
            },
            Command {
                name: "colors",
                args: "",
                help: "every token of the palette now on screen, as name and hex",
                run: |_| {
                    let config = shell::config().ok_or("the shell is not running")?;
                    Ok(palette_rows(&config.resolve_theme()))
                },
            },
            Command {
                name: "list",
                args: "",
                help: "the palettes `scheme set` accepts",
                run: |_| {
                    let mut names: Vec<&str> = crate::BUILT_IN_THEMES.to_vec();
                    names.push("custom");
                    names.push(crate::shared::scheme::DYNAMIC);
                    Ok(names.join("\t"))
                },
            },
            Command {
                name: "set",
                args: "<name>",
                help: "switch palette, `dynamic` for one built from the wallpaper",
                run: |args| {
                    use crate::shared::scheme::{self, Choice};
                    scheme::apply(Choice::Palette, arg(args, 0, "name")?)
                },
            },
            Command {
                name: "mode",
                args: "[dark|light|auto|toggle]",
                help: "read or set the light/dark mode",
                run: |args| {
                    use crate::shared::scheme::{self, Mode};
                    let config = shell::config().ok_or("the shell is not running")?;
                    let Some(wanted) = args.first().copied() else {
                        return Ok(config.theme.mode.clone());
                    };
                    let next = match wanted {
                        "auto" => "auto".to_string(),
                        // Toggling resolves the *effective* mode first: `auto` is not a third state to flip
                        // through, it is "whatever the palette is", and a user pressing a toggle means the
                        // other one.
                        "toggle" => match Mode::of(&config.resolve_theme()) {
                            Mode::Dark => Mode::Light,
                            Mode::Light => Mode::Dark,
                        }
                        .id()
                        .to_string(),
                        other => Mode::from_id(other)
                            .ok_or_else(|| {
                                format!("expected dark|light|auto|toggle, got '{other}'")
                            })?
                            .id()
                            .to_string(),
                    };
                    scheme::apply(scheme::Choice::Mode, &next)
                },
            },
            Command {
                name: "variant",
                args: "[name]",
                help: "read or set how much colour a dynamic scheme carries",
                run: |args| {
                    use crate::shared::scheme::{self, Variant};
                    let Some(wanted) = args.first().copied() else {
                        let config = shell::config().ok_or("the shell is not running")?;
                        return Ok(config.theme.variant.clone());
                    };
                    let variant = Variant::from_id(wanted).ok_or_else(|| {
                        let known: Vec<&str> = Variant::ALL.iter().map(|v| v.id()).collect();
                        format!("unknown variant '{wanted}', expected one of {}", known.join("|"))
                    })?;
                    scheme::apply(scheme::Choice::Variant, variant.id())
                },
            },
            Command {
                name: "refresh",
                args: "",
                help: "re-derive a dynamic palette from the current wallpaper",
                run: |_| {
                    let config = shell::config().ok_or("the shell is not running")?;
                    if !config.theme.is_dynamic() {
                        return Err("[theme] name is not 'dynamic'".to_string());
                    }
                    crate::shared::scheme::refresh(&config, Duration::ZERO);
                    Ok("refreshing".to_string())
                },
            },
            Command {
                name: "export",
                args: "",
                help: "write the palette out for the rest of the desktop, ignoring [theme.export] enabled",
                run: |_| {
                    use crate::shared::scheme;
                    let config = shell::config().ok_or("the shell is not running")?;
                    let current = scheme::current()
                        .ok_or("no dynamic palette has been derived yet")?;
                    let export = crate::core::config::SchemeExportConfig {
                        enabled: true,
                        ..config.theme.export.clone()
                    };
                    let dir = export.resolved_dir();
                    scheme::export_scheme(&current, &export);
                    Ok(dir.display().to_string())
                },
            },
        ],
    },
    Target {
        name: "config",
        commands: &[
            Command {
                name: "path",
                args: "",
                help: "where config.toml is read from",
                run: |_| Ok(crate::Config::default_path().display().to_string()),
            },
            Command {
                name: "schema",
                args: "[section]",
                help: "the annotated default config, or one section of it",
                run: |args| {
                    let section = args.first().copied().filter(|s| !s.is_empty());
                    crate::core::schema::render(section)
                },
            },
        ],
    },
];

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn arg<'a>(args: &'a [&'a str], index: usize, name: &str) -> Result<&'a str, String> {
    args.get(index)
        .copied()
        .ok_or_else(|| format!("missing argument <{name}>"))
}

fn number(args: &[&str], index: usize, name: &str) -> Result<i32, String> {
    let raw = arg(args, index, name)?;
    raw.parse()
        .map_err(|_| format!("<{name}> must be a whole number, got '{raw}'"))
}

/// One row per node, tab-separated so a script can cut columns: id, level, mute, whether it is the default,
/// and the label last because it is the only field that can contain spaces.
fn list_nodes(kind: NodeKind) -> String {
    use crate::shared::services::pipewire;
    let Some(graph) = pipewire::current() else {
        return String::new();
    };
    let default = match kind {
        NodeKind::Source => graph.default_source().map(|node| node.id),
        _ => graph.default_sink().map(|node| node.id),
    };
    graph
        .of_kind(kind)
        .map(|node| {
            format!(
                "{}\t{}\t{}\t{}\t{}",
                node.id,
                node.level,
                on_off(node.muted),
                if Some(node.id) == default { "default" } else { "-" },
                node.label()
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn node_id(args: &[&str]) -> Result<u32, String> {
    let raw = arg(args, 0, "id")?;
    raw.parse()
        .map_err(|_| format!("<id> must be a PipeWire node id, got '{raw}'"))
}

/// Which screen a wallpaper command means: the one named, else the focused one.
///
/// Resolved to a name rather than left as `None`, because `None` means "every screen" to the service and
/// "wherever the user is looking" to a keybind — and a `wallpaper random` bound to a key should change the
/// screen in front of them, not all of them.
///
/// A name that is not a monitor is refused. Accepting one writes an entry into the persisted assignment that
/// no surface will ever read, and answers `ok` while changing nothing — which is how a stray `--features` from
/// a dev harness ended up saved as a screen. It costs one lookup to make a typo say so.
/// Which screen a wallpaper command changes: the one named, or every screen when none is.
///
/// "Every screen" and not "the focused one", because that is what each command's own help says it does, and
/// because the focused-screen reading made `wallpaper clear` unable to do the one thing it exists for: it
/// removed the focused monitor's entry, answered `cleared`, and left every other entry — including one written
/// under a name no monitor has — sitting in the state file with no way left to reach it.
fn target_output(named: Option<&str>) -> Result<Option<String>, String> {
    named.map(validated).transpose()
}

/// Which screen a wallpaper command *reads*: the one named, else the focused one. A reading has to be about
/// some screen, so here the focused one is the only sensible default.
fn reading_output(named: Option<&str>) -> Result<Option<String>, String> {
    match named {
        Some(name) => validated(name).map(Some),
        None => Ok(shell::focused_output()),
    }
}

/// Which displays a brightness *mutation* means: the one named, `all` of them, or — with nothing named — the
/// primary one.
///
/// Deliberately not the wallpaper rule, where an unnamed mutation means every screen. A wallpaper is one desktop
/// look; brightness is per-panel hardware, and `brightness up` is overwhelmingly a laptop's function key, which
/// means *this* panel. `all` is there for the desk, and both are in the command's help so neither is a surprise.
fn dimmable_targets(named: Option<&str>) -> Result<Vec<String>, String> {
    use crate::shared::services::brightness;
    let snapshot = brightness::snapshot();
    match named {
        Some(name) if name.eq_ignore_ascii_case("all") => {
            let outputs: Vec<String> = snapshot
                .displays
                .iter()
                .map(|display| display.output.clone())
                .collect();
            if outputs.is_empty() {
                return Err("no controllable display".to_string());
            }
            Ok(outputs)
        }
        Some(name) => Ok(vec![dimmable_output(name)?]),
        None => snapshot
            .primary()
            .map(|display| vec![display.output.clone()])
            .ok_or_else(|| "no controllable display".to_string()),
    }
}

/// `name` if a display on it can be dimmed, else an error naming the ones that can.
///
/// Checked against the brightness snapshot rather than against the compositor's outputs: those are two different
/// sets. A monitor with no DDC support is an output that cannot be dimmed, and a DDC monitor whose connector could
/// not be resolved answers to `i2c-6` — a name no compositor has ever heard of.
fn dimmable_output(name: &str) -> Result<String, String> {
    use crate::shared::services::brightness;
    let snapshot = brightness::snapshot();
    if let Some(display) = snapshot.get(name) {
        return Ok(display.output.clone());
    }
    if snapshot.is_empty() {
        return Err(format!(
            "'{name}' has no controllable brightness (nothing on this machine does)"
        ));
    }
    let known: Vec<&str> = snapshot
        .displays
        .iter()
        .map(|display| display.output.as_str())
        .collect();
    Err(format!(
        "'{name}' has no controllable brightness (this machine has: {})",
        known.join(", ")
    ))
}

fn validated(name: &str) -> Result<String, String> {
    let screens: Vec<String> = platform_layershell::outputs()
        .into_iter()
        .filter_map(|output| output.name)
        .collect();
    known_screen(name, &screens)
}

/// `name` if it is one of `screens`, else an error naming the real ones.
fn known_screen(name: &str, screens: &[String]) -> Result<String, String> {
    if screens.iter().any(|screen| screen == name) {
        return Ok(name.to_string());
    }
    if screens.is_empty() {
        return Err(format!("'{name}' is not a monitor (none are connected)"));
    }
    Err(format!(
        "'{name}' is not a monitor (this session has: {})",
        screens.join(", ")
    ))
}

/// Re-derives the dynamic palette after a wallpaper change, once the transition to the new image has finished.
/// A no-op unless `[theme] name = "dynamic"`, so every wallpaper command can call it blind.
fn refresh_scheme() {
    crate::shared::scheme::refresh_current();
}

/// The palette as one `name<TAB>#rrggbb` row per token, which is what a script recolouring something else needs.
fn palette_rows(theme: &crate::NordTheme) -> String {
    let hex = |color: rsx::Color| {
        let [r, g, b, _] = color.to_rgba8();
        format!("#{r:02x}{g:02x}{b:02x}")
    };
    [
        ("base", theme.base),
        ("surface", theme.surface),
        ("overlay", theme.overlay),
        ("muted", theme.muted),
        ("subtle", theme.subtle),
        ("text", theme.text),
        ("accent", theme.accent),
        ("blue", theme.blue),
        ("cyan", theme.cyan),
        ("teal", theme.teal),
        ("red", theme.red),
        ("orange", theme.orange),
        ("yellow", theme.yellow),
        ("green", theme.green),
        ("purple", theme.purple),
        ("success", theme.success),
        ("warning", theme.warning),
        ("error", theme.error),
        ("info", theme.info),
        ("highlight_low", theme.highlight_low),
        ("highlight_med", theme.highlight_med),
        ("highlight_high", theme.highlight_high),
    ]
    .iter()
    .map(|(name, color)| format!("{name}\t{}", hex(*color)))
    .collect::<Vec<String>>()
    .join("\n")
}

/// Switches the dashboard's page by its config id, refusing an unknown one by name — a keybind bound to a page
/// that was renamed should say so rather than silently leaving the dashboard where it was.
fn set_dashboard_tab(name: &str) -> Result<(), String> {
    use crate::core::config::DashboardTab;
    let tab = DashboardTab::from_id(name).ok_or_else(|| {
        let known: Vec<&str> = DashboardTab::ALL.iter().map(|t| t.id()).collect();
        format!("unknown tab '{name}', expected one of {}", known.join("|"))
    })?;
    crate::modules::dashboard::set_tab(tab);
    Ok(())
}

/// Runs one request line and renders the reply. `ok`/`err` prefixes let a caller branch on the outcome without
/// parsing the message; the payload follows on the same line when there is one.
/// Looks a request line up in the command table without running anything, yielding the command and its
/// arguments, or the `err …` reply the caller should send back.
///
/// Split out from [`dispatch`] so that "is this command wired up" can be answered *without executing it*. The
/// listing test used to answer that by dispatching every advertised command with no arguments — which for any
/// command that needs none is not a lookup, it is the command. `wifi disconnect` and `vpn toggle` both take no
/// arguments, so running the test suite dropped the machine off the network; `volume up` and `brightness down`
/// had been quietly moving the user's settings for far longer.
/// Whether `line` names a command the shell answers, **without running it**.
///
/// The distinction is the whole reason [`resolve`] is split out of [`dispatch`]: anything that wants to check a
/// request line — the global-shortcut table, a future config validator — must be able to do so without
/// performing it. Half this table changes the machine.
pub fn resolves(line: &str) -> bool {
    resolve(line).is_ok()
}

fn resolve(line: &str) -> Result<(&'static Command, Vec<&str>), String> {
    let mut words = line.split_whitespace();
    let Some(target_name) = words.next() else {
        return Err("empty request".to_string());
    };
    let command_name = words.next().unwrap_or("");
    let args: Vec<&str> = words.collect();

    let Some(target) = TARGETS.iter().find(|t| t.name == target_name) else {
        return Err(format!("unknown target '{target_name}'"));
    };
    let Some(command) = target.commands.iter().find(|c| c.name == command_name) else {
        let known: Vec<&str> = target.commands.iter().map(|c| c.name).collect();
        return Err(format!(
            "unknown command '{command_name}' for '{target_name}' (try: {})",
            known.join(", ")
        ));
    };
    Ok((command, args))
}

pub fn dispatch(line: &str) -> String {
    let (command, args) = match resolve(line) {
        Ok(found) => found,
        Err(message) => return format!("err {message}"),
    };
    match (command.run)(&args) {
        Ok(payload) if payload.is_empty() => "ok".to_string(),
        Ok(payload) => format!("ok {payload}"),
        Err(message) => format!("err {message}"),
    }
}

/// The column the help text starts at in `--list`.
const HELP_COLUMN: usize = 28;

/// Every target and command, one per line, for `hyprshell --list`.
pub fn describe() -> String {
    let mut out = String::new();
    for target in TARGETS {
        out.push_str(&format!("target {}\n", target.name));
        for command in target.commands {
            let signature = if command.args.is_empty() {
                command.name.to_string()
            } else {
                format!("{} {}", command.name, command.args)
            };
            // A signature wider than the column gets its help on the next line rather than running into it.
            if signature.len() >= HELP_COLUMN {
                out.push_str(&format!(
                    "  {signature}\n  {:HELP_COLUMN$}{}\n",
                    "", command.help
                ));
            } else {
                out.push_str(&format!("  {signature:HELP_COLUMN$}{}\n", command.help));
            }
        }
    }
    out
}

/// The socket producer: binds, then hands every request line to the driver thread and writes back its reply.
/// Runs on its own thread via `platform_layershell::watch`, so a slow or hostile client never blocks the UI.
pub fn serve(tx: EventSender<Request>) {
    let path = socket_path();
    paths::ensure_dir(path.parent().map(PathBuf::from).unwrap_or_default());
    // A socket left behind by a killed shell would refuse the bind; the caller has already established that
    // nothing is listening on it (see `another_instance_is_running`), so removing it is safe here.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("IPC unavailable: cannot bind {}: {e}", path.display());
            return;
        }
    };
    tracing::info!("IPC listening on {}", path.display());

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !handle_client(stream, &tx) {
            break; // the driver is gone (the shell is shutting down)
        }
    }
    let _ = std::fs::remove_file(&path);
}

/// Serves one connection, which may carry several request lines. Returns `false` once the driver stops
/// answering, which is the socket thread's signal to wind itself down.
fn handle_client(stream: UnixStream, tx: &EventSender<Request>) -> bool {
    let Ok(mut out) = stream.try_clone() else {
        return true;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return true };
        if line.trim().is_empty() {
            continue;
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        if !tx.send(Request {
            line,
            reply: reply_tx,
        }) {
            return false;
        }
        let reply = match reply_rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(reply) => reply,
            Err(_) => "err the shell did not answer in time".to_string(),
        };
        if writeln!(out, "{reply}").is_err() {
            return true; // client hung up mid-request; the shell carries on
        }
    }
    true
}

/// Runs a request that arrived over the socket. Called on the driver thread by the `watch` consumer.
pub fn handle(request: Request) {
    let reply = dispatch(&request.line);
    let _ = request.reply.send(reply);
}

/// Sends one request to a running shell and returns its reply. The client half of the protocol, used by the CLI.
pub fn call(line: &str) -> std::io::Result<String> {
    let mut stream = UnixStream::connect(socket_path())?;
    writeln!(stream, "{line}")?;
    stream.flush()?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim_end().to_string())
}

/// Whether a shell is already listening on this session's socket. A stale socket file from a killed process
/// fails to connect, so this answers "is one actually running", not "does the file exist".
pub fn another_instance_is_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_targets_and_commands_report_what_is_available() {
        assert_eq!(dispatch("nope ping"), "err unknown target 'nope'");
        let reply = dispatch("panel wiggle");
        assert!(reply.starts_with("err unknown command 'wiggle'"), "{reply}");
        assert!(reply.contains("toggle"), "it lists the real commands: {reply}");
        assert_eq!(dispatch("   "), "err empty request");
    }

    #[test]
    fn a_command_with_no_arguments_reports_which_one_is_missing() {
        assert_eq!(dispatch("panel toggle"), "err missing argument <module>");
        assert_eq!(
            dispatch("volume set abc"),
            "err <percent> must be a whole number, got 'abc'"
        );
    }

    #[test]
    fn replies_carry_an_ok_or_err_prefix() {
        assert_eq!(dispatch("shell ping"), "ok pong");
        assert_eq!(
            dispatch("shell version"),
            format!("ok {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn describe_covers_every_target_and_is_what_dispatch_accepts() {
        let listing = describe();
        for target in TARGETS {
            assert!(
                listing.contains(&format!("target {}", target.name)),
                "'{}' missing from the listing",
                target.name
            );
            for command in target.commands {
                // Resolved, never run. Half of this table changes the machine — the network it is on, the
                // volume, the backlight, whether the process is still alive — and a test that proved the
                // wiring by *executing* every entry was doing all of that to whoever ran `cargo test`.
                let line = format!("{} {}", target.name, command.name);
                let (found, _) = resolve(&line).unwrap_or_else(|e| {
                    panic!("{line} is advertised but does not resolve: {e}")
                });
                assert_eq!(
                    found.name, command.name,
                    "'{} {}' resolved to a different command",
                    target.name, command.name
                );
            }
        }
        // The lookup still reports what it cannot find, which is the other half of the contract.
        assert!(resolve("nosuchtarget ping").is_err());
        assert!(resolve("shell nosuchcommand").is_err());
        assert!(resolve("").is_err());
    }

    #[test]
    fn a_command_that_changes_the_machine_is_never_run_by_the_suite() {
        // A standing guard on the test above: these take no arguments, so dispatching one "just to check it
        // resolves" performs it. Listed by name so that adding another argumentless mutation is a decision
        // someone makes here rather than something a green test run hides.
        const ARGUMENTLESS_MUTATIONS: &[(&str, &str)] = &[
            ("shell", "quit"),
            ("shell", "reload"),
            ("notifs", "clear"),
            ("wifi", "scan"),
            ("wifi", "disconnect"),
            ("vpn", "toggle"),
            ("volume", "up"),
            ("volume", "down"),
            ("volume", "mute"),
            ("mic", "mute"),
            ("brightness", "up"),
            ("brightness", "down"),
            ("media", "play-pause"),
            ("wallpaper", "reload"),
            ("wallpaper", "random"),
            ("wallpaper", "clear"),
            ("scheme", "refresh"),
            ("scheme", "export"),
            ("screenshot", "screen"),
            ("screenshot", "region"),
            ("screenshot", "cancel"),
            ("record", "start"),
            ("record", "stop"),
            ("record", "toggle"),
            ("record", "pause"),
            ("toast", "clear"),
            ("notifs", "center"),
        ];
        for (target, command) in ARGUMENTLESS_MUTATIONS {
            assert!(
                resolve(&format!("{target} {command}")).is_ok(),
                "'{target} {command}' is listed here but no longer exists"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_a_monitor_is_refused_rather_than_saved() {
        // Not hypothetical: a dev harness appending `--features rsx/dev` to the program's arguments had
        // `wallpaper random` read `--features` as a screen, write it into the persisted assignment, and answer
        // `ok`. Nothing would ever have painted it.
        let screens = vec!["DP-1".to_string(), "HDMI-A-1".to_string()];
        assert_eq!(known_screen("DP-1", &screens), Ok("DP-1".to_string()));

        let refused = known_screen("--features", &screens).expect_err("a flag is not a screen");
        assert!(refused.contains("not a monitor"), "{refused}");
        assert!(refused.contains("DP-1"), "and it says what there is: {refused}");
        // A typo in a real name is the common case and must not read as "no monitors".
        assert!(known_screen("DP1", &screens).is_err());
        assert!(
            known_screen("DP-1", &[]).unwrap_err().contains("none are connected"),
            "no screens at all is its own message, not an empty list"
        );
    }

    #[test]
    fn a_wallpaper_command_with_no_screen_named_means_every_screen() {
        // `clear` defaulting to the focused screen removed one entry, answered `cleared`, and left the rest —
        // including one saved under a name no monitor has, which validation then made unreachable. A command
        // whose help says "every screen" has to mean it.
        assert_eq!(target_output(None), Ok(None));
        // The reading side is the opposite and deliberately so: "which image is showing" needs a screen.
        // Only the `None` branch is asserted here — resolving a name asks the compositor.
        assert!(reading_output(None).is_ok());
    }

    #[test]
    fn socket_name_is_scoped_to_the_compositor_instance() {
        assert_eq!(socket_name(Some("abc123".into())), "abc123.sock");
        assert_eq!(
            socket_name(None),
            "default.sock",
            "outside Hyprland it still has a stable name"
        );
        assert_eq!(
            socket_name(Some(String::new())),
            "default.sock",
            "an empty signature is the same as none, not a bare '.sock'"
        );
    }

    #[test]
    fn a_round_trip_through_the_socket_answers_on_the_driver_thread() {
        // The whole client → socket thread → driver → reply path, minus the driver's real event loop: a stand-in
        // consumer runs `handle` exactly as the `watch` callback does.
        let (tx, rx) = mpsc::channel::<Request>();
        let driver = std::thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                handle(request);
            }
        });

        let (reply_tx, reply_rx) = mpsc::channel();
        tx.send(Request {
            line: "shell ping".to_string(),
            reply: reply_tx,
        })
        .unwrap();
        assert_eq!(reply_rx.recv_timeout(REPLY_TIMEOUT).unwrap(), "ok pong");

        drop(tx);
        driver.join().unwrap();
    }
}
