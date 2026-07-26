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
                args: "",
                help: "drop every notification from the history",
                run: |_| {
                    crate::shared::services::notifications::clear_all();
                    Ok("cleared".to_string())
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
                args: "",
                help: "the backlight level",
                run: |_| {
                    let level = crate::shared::services::brightness::current()
                        .ok_or("no backlight available")?;
                    Ok(level.to_string())
                },
            },
            Command {
                name: "set",
                args: "<percent>",
                help: "set the backlight level",
                run: |args| {
                    let level = number(args, 0, "percent")?;
                    crate::shared::services::brightness::set(level);
                    crate::modules::osd::show_brightness();
                    Ok(level.to_string())
                },
            },
            Command {
                name: "step",
                args: "<±percent>",
                help: "move the backlight by a delta",
                run: |args| {
                    let delta = number(args, 0, "delta")?;
                    crate::shared::services::brightness::step(delta);
                    crate::modules::osd::show_brightness();
                    Ok(delta.to_string())
                },
            },
            Command {
                name: "up",
                args: "",
                help: "raise the backlight by [brightness] increment",
                run: |_| {
                    let step = crate::shared::services::brightness::settings().step();
                    crate::shared::services::brightness::step(step);
                    crate::modules::osd::show_brightness();
                    Ok(step.to_string())
                },
            },
            Command {
                name: "down",
                args: "",
                help: "lower the backlight by [brightness] increment",
                run: |_| {
                    let step = crate::shared::services::brightness::settings().step();
                    crate::shared::services::brightness::step(-step);
                    crate::modules::osd::show_brightness();
                    Ok((-step).to_string())
                },
            },
        ],
    },
    Target {
        name: "config",
        commands: &[Command {
            name: "path",
            args: "",
            help: "where config.toml is read from",
            run: |_| Ok(crate::Config::default_path().display().to_string()),
        }],
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

/// Runs one request line and renders the reply. `ok`/`err` prefixes let a caller branch on the outcome without
/// parsing the message; the payload follows on the same line when there is one.
pub fn dispatch(line: &str) -> String {
    let mut words = line.split_whitespace();
    let Some(target_name) = words.next() else {
        return "err empty request".to_string();
    };
    let command_name = words.next().unwrap_or("");
    let args: Vec<&str> = words.collect();

    let Some(target) = TARGETS.iter().find(|t| t.name == target_name) else {
        return format!("err unknown target '{target_name}'");
    };
    let Some(command) = target.commands.iter().find(|c| c.name == command_name) else {
        let known: Vec<&str> = target.commands.iter().map(|c| c.name).collect();
        return format!(
            "err unknown command '{command_name}' for '{target_name}' (try: {})",
            known.join(", ")
        );
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
                // Every advertised command must resolve; an "unknown command" reply here means the table and
                // the dispatcher have drifted apart.
                let reply = dispatch(&format!("{} {}", target.name, command.name));
                assert!(
                    !reply.starts_with("err unknown"),
                    "{} {} is advertised but does not dispatch: {reply}",
                    target.name,
                    command.name
                );
            }
        }
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
