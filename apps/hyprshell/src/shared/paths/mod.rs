use std::path::{Path, PathBuf};

/// Expands a leading `~` (bare or `~/…`) to `$HOME`, leaving every other path untouched. User-authored config paths (e.g. a wallpaper) commonly use `~`, which the OS doesn't resolve on its own.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(rest),
        None => path.to_path_buf(),
    }
}

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
