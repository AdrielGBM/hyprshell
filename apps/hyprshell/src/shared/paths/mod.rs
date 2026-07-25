use std::ffi::OsString;
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

/// The XDG resolution rule, over values rather than the environment: the variable when it names a non-empty
/// path, else `$HOME` joined with `fallback`, else `fallback` relative. Taking the values as arguments keeps
/// this testable without mutating process-wide environment, which would race every other test in the binary.
fn resolve_base(var: Option<OsString>, home: Option<OsString>, fallback: &str) -> PathBuf {
    var.map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home.map(|h| PathBuf::from(h).join(fallback)))
        .unwrap_or_else(|| PathBuf::from(fallback))
}

/// `$VAR` when it names a non-empty path, else `$HOME` joined with `fallback`.
fn xdg_base(var: &str, fallback: &str) -> PathBuf {
    resolve_base(std::env::var_os(var), std::env::var_os("HOME"), fallback)
}

/// The app's data directory (`$XDG_DATA_HOME/hyprshell`, else `~/.local/share/hyprshell`), where persistent
/// user state lives — notes and notification history.
pub fn data_dir() -> PathBuf {
    xdg_base("XDG_DATA_HOME", ".local/share").join("hyprshell")
}

/// The app's state directory (`$XDG_STATE_HOME/hyprshell`, else `~/.local/state/hyprshell`): machine-written
/// state the user never edits — what the shell remembers across restarts, as opposed to the config they own.
pub fn state_dir() -> PathBuf {
    xdg_base("XDG_STATE_HOME", ".local/state").join("hyprshell")
}

/// The app's cache directory (`$XDG_CACHE_HOME/hyprshell`, else `~/.cache/hyprshell`): regenerable artefacts
/// (icons, cover art, thumbnails) that are safe to delete.
pub fn cache_dir() -> PathBuf {
    xdg_base("XDG_CACHE_HOME", ".cache").join("hyprshell")
}

/// The app's runtime directory, where the IPC socket lives. `$XDG_RUNTIME_DIR/hyprshell` when the session
/// provides one (tmpfs, cleaned on logout — where a socket belongs), else a `/tmp` path scoped to the user so
/// two users on one machine never collide.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let uid = std::env::var("UID").unwrap_or_else(|_| "user".to_string());
            PathBuf::from(format!("/tmp/hyprshell-{uid}"))
        })
        .join("hyprshell")
}

/// Creates `dir` (and its parents) and returns it, so a caller can chain straight into a file path.
pub fn ensure_dir(dir: PathBuf) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("could not create {}: {e}", dir.display());
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    #[test]
    fn resolve_base_prefers_the_variable_then_home() {
        assert_eq!(
            resolve_base(os("/xdg/state"), os("/home/tester"), ".local/state"),
            PathBuf::from("/xdg/state")
        );
        assert_eq!(
            resolve_base(os(""), os("/home/tester"), ".local/state"),
            PathBuf::from("/home/tester/.local/state"),
            "an empty variable falls back to $HOME, not to an empty path"
        );
        assert_eq!(
            resolve_base(None, os("/home/tester"), ".cache"),
            PathBuf::from("/home/tester/.cache")
        );
        assert_eq!(
            resolve_base(None, None, ".cache"),
            PathBuf::from(".cache"),
            "no HOME at all still yields a usable relative path rather than panicking"
        );
    }

    #[test]
    fn expand_tilde_only_touches_a_leading_tilde() {
        assert_eq!(
            expand_tilde(Path::new("/etc/x~y")),
            PathBuf::from("/etc/x~y"),
            "a tilde mid-path is a literal character"
        );
        assert_eq!(
            expand_tilde(Path::new("relative/path")),
            PathBuf::from("relative/path")
        );
        // The `~/…` expansion itself depends on $HOME, which this binary's other tests also read; asserting on
        // it would mean mutating process-wide state, so only the prefix rule is covered here.
        assert!(expand_tilde(Path::new("~/Pictures")).ends_with("Pictures"));
    }
}
