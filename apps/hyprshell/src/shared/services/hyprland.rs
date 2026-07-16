use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use platform_layershell::EventSender;

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub active: i32,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: i32,
    pub windows: u32,
}

#[derive(Deserialize)]
struct WorkspaceJson {
    id: i32,
    windows: u32,
}

#[derive(Deserialize)]
struct ActiveJson {
    id: i32,
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

pub fn focus_workspace(dir: &Path, id: i32) {
    // Hyprland ≥ 0.55 evaluates socket commands as Lua, so `dispatch workspace N` no longer parses; this dispatches `hl.dsp.focus` instead, wrapped by the socket as `hl.dispatch(hl.dsp.focus({ workspace = N }))`.
    let cmd = format!("dispatch hl.dsp.focus({{ workspace = {id} }})");
    match request(dir, &cmd) {
        Ok(resp) if resp.to_ascii_lowercase().contains("error") => {
            tracing::warn!("hyprshell: `{cmd}` -> {resp:?}")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("hyprshell: `{cmd}` failed: {e}"),
    }
}

fn query_snapshot(dir: &Path) -> Option<Snapshot> {
    let workspaces_raw = request(dir, "j/workspaces").ok()?;
    let active_raw = request(dir, "j/activeworkspace").ok()?;

    let mut workspaces: Vec<Workspace> = serde_json::from_str::<Vec<WorkspaceJson>>(&workspaces_raw)
        .ok()?
        .into_iter()
        .filter(|w| w.id > 0) // special workspaces have negative ids
        .map(|w| Workspace {
            id: w.id,
            windows: w.windows,
        })
        .collect();
    workspaces.sort_by_key(|w| w.id);

    let active = serde_json::from_str::<ActiveJson>(&active_raw).ok()?.id;
    Some(Snapshot { workspaces, active })
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

pub fn stream_workspaces(dir: PathBuf, tx: EventSender<Snapshot>) {
    if let Some(snapshot) = query_snapshot(&dir) {
        if !tx.send(snapshot) {
            return;
        }
    }

    let Ok(stream) = UnixStream::connect(dir.join(".socket2.sock")) else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if affects_workspaces(&line) {
            if let Some(snapshot) = query_snapshot(&dir) {
                if !tx.send(snapshot) {
                    break;
                }
            }
        }
    }
}
