//! `hyprshell shell`, `config`, `scheme` and `session` — the shell talking about itself.

use std::time::Duration;

use super::args::*;
use super::{Command, Target};
use crate::core::ipc::request_quit;

pub(crate) const SHELL: Target = Target {
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
                config::request_reload();
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
                use services::hyprland;
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
                use services::hyprland;
                let dir = hyprland::socket_dir().ok_or("not running under Hyprland")?;
                let rows: Vec<String> = hyprland::current_clients()
                    .unwrap_or_else(|| hyprland::clients(&dir))
                    .iter()
                    .map(|c| format!("{}\t{}\t{}\t{}", c.address, c.workspace, c.class, c.title))
                    .collect();
                Ok(rows.join("\n"))
            },
        },
        Command {
            name: "dpms",
            args: "<on|off>",
            help: "switch every monitor's output on or off",
            run: |args| {
                use services::hyprland;
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
                request_quit();
                Ok("bye".to_string())
            },
        },
    ],
};

pub(crate) const SESSION: Target = Target {
    name: "session",
    commands: &[
        Command {
            name: "list",
            args: "",
            help: "which session actions this machine supports",
            run: |_| {
                let ids: Vec<&str> = services::session::available()
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
                use services::session;
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
};

pub(crate) const SCHEME: Target = Target {
    name: "scheme",
    commands: &[
        Command {
            name: "status",
            args: "",
            help: "the palette in use: theme, mode, variant and the wallpaper it came from",
            run: |_| {
                use config::scheme;
                let config = config::config().ok_or("the shell is not running")?;
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
                let config = config::config().ok_or("the shell is not running")?;
                Ok(palette_rows(&config.resolve_theme()))
            },
        },
        Command {
            name: "list",
            args: "",
            help: "the palettes `scheme set` accepts",
            run: |_| {
                let mut names: Vec<&str> = config::theme::BUILT_IN_THEMES.to_vec();
                names.push("custom");
                names.push(config::scheme::DYNAMIC);
                Ok(names.join("\t"))
            },
        },
        Command {
            name: "set",
            args: "<name>",
            help: "switch palette, `dynamic` for one built from the wallpaper",
            run: |args| {
                use config::scheme::{self, Choice};
                scheme::apply(Choice::Palette, arg(args, 0, "name")?)
            },
        },
        Command {
            name: "mode",
            args: "[dark|light|auto|toggle]",
            help: "read or set the light/dark mode",
            run: |args| {
                use config::scheme::{self, Mode};
                let config = config::config().ok_or("the shell is not running")?;
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
                        .ok_or_else(|| format!("expected dark|light|auto|toggle, got '{other}'"))?
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
                use config::scheme::{self, Variant};
                let Some(wanted) = args.first().copied() else {
                    let config = config::config().ok_or("the shell is not running")?;
                    return Ok(config.theme.variant.clone());
                };
                let variant = Variant::from_id(wanted).ok_or_else(|| {
                    let known: Vec<&str> = Variant::ALL.iter().map(|v| v.id()).collect();
                    format!(
                        "unknown variant '{wanted}', expected one of {}",
                        known.join("|")
                    )
                })?;
                scheme::apply(scheme::Choice::Variant, variant.id())
            },
        },
        Command {
            name: "refresh",
            args: "",
            help: "re-derive a dynamic palette from the current wallpaper",
            run: |_| {
                let config = config::config().ok_or("the shell is not running")?;
                if !config.theme.is_dynamic() {
                    return Err("[theme] name is not 'dynamic'".to_string());
                }
                config::scheme::refresh(&config, Duration::ZERO);
                Ok("refreshing".to_string())
            },
        },
        Command {
            name: "export",
            args: "",
            help: "write the palette out for the rest of the desktop, ignoring [theme.export] enabled",
            run: |_| {
                use config::scheme;
                let config = config::config().ok_or("the shell is not running")?;
                let current = scheme::current().ok_or("no dynamic palette has been derived yet")?;
                let export = config::SchemeExportConfig {
                    enabled: true,
                    ..config.theme.export.clone()
                };
                let dir = export.resolved_dir();
                scheme::export_scheme(&current, &export);
                Ok(dir.display().to_string())
            },
        },
    ],
};

pub(crate) const CONFIG: Target = Target {
    name: "config",
    commands: &[
        Command {
            name: "path",
            args: "",
            help: "where config.toml is read from",
            run: |_| Ok(config::Config::default_path().display().to_string()),
        },
        Command {
            name: "schema",
            args: "[section]",
            help: "the annotated default config, or one section of it",
            run: |args| {
                let section = args.first().copied().filter(|s| !s.is_empty());
                config::schema::render(section)
            },
        },
    ],
};
