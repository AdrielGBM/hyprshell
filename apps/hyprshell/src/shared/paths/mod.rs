use std::path::PathBuf;

/// The app's data directory (`$XDG_DATA_HOME/hyprshell`, else `~/.local/share/hyprshell`), where persistent
/// user state lives — notes and notification history.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
        })
        .unwrap_or_else(|| PathBuf::from(".local/share"));
    base.join("hyprshell")
}
