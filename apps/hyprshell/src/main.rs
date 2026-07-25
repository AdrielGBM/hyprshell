use std::process::ExitCode;

const USAGE: &str = "\
hyprshell — a Wayland shell for Hyprland

Usage:
  hyprshell [run]                 start the shell
  hyprshell <target> <cmd> [args] send a command to the running shell
  hyprshell toggle <module>       shorthand for `panel toggle <module>`
  hyprshell launcher              shorthand for `launcher toggle`
  hyprshell --list                list every command the shell answers
  hyprshell --help | --version

Examples:
  hyprshell panel toggle clock
  hyprshell volume step -5
  hyprshell notifs dnd toggle

Bind them in hyprland.conf:
  bind = SUPER, N, exec, hyprshell panel toggle notifications
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("run") => {
            init_tracing();
            hyprshell::run();
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("hyprshell {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--list" | "-s") => {
            print!("{}", hyprshell::ipc_describe());
            ExitCode::SUCCESS
        }
        _ => send(&args),
    }
}

/// Forwards a command to the running shell and mirrors its verdict into the exit code, so a keybind or script
/// can tell a refused command from one that worked without parsing the reply.
fn send(args: &[String]) -> ExitCode {
    // `hyprshell toggle x` is the one shorthand worth having: opening a panel is what a keybind almost always
    // wants, and `panel toggle x` in every hyprland.conf line is noise.
    let request = match args.first().map(String::as_str) {
        Some("toggle") => format!("panel {}", args.join(" ")),
        // The launcher is the one surface people bind a key to before anything else.
        Some("launcher") if args.len() == 1 => "launcher toggle".to_string(),
        _ => args.join(" "),
    };
    match hyprshell::ipc_call(&request) {
        Ok(reply) => match reply.strip_prefix("ok") {
            Some(payload) => {
                let payload = payload.trim();
                if !payload.is_empty() {
                    println!("{payload}");
                }
                ExitCode::SUCCESS
            }
            None => {
                eprintln!(
                    "hyprshell: {}",
                    reply.strip_prefix("err ").unwrap_or(&reply)
                );
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("hyprshell: no running shell to talk to ({e})");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
}
