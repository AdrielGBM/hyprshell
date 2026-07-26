//! Putting text on the Wayland clipboard.
//!
//! Wayland has no way for a client to set the selection without an active seat and a data-source dance that
//! only makes sense for a focused surface, so this hands off to `wl-copy` (wl-clipboard) — the same tool every
//! other shell uses for this. It is optional: with `wl-copy` missing, copying is a no-op with a log line rather
//! than an error the user has to dismiss.

use std::io::Write;
use std::process::{Command, Stdio};

/// Copies `text` to the clipboard, off the UI thread — spawning and feeding a process is blocking work, and a
/// launcher's Enter handler runs on the frame.
pub fn copy(text: &str) {
    let text = text.to_string();
    let _ = std::thread::Builder::new()
        .name("hyprshell-clipboard".to_string())
        .spawn(move || {
            if write_to_wl_copy(&text).is_none() {
                tracing::warn!("clipboard: `wl-copy` is not available; nothing was copied");
            }
        });
}

/// Feeds `text` to `wl-copy` on stdin, so it never appears in the process table the way an argument would.
fn write_to_wl_copy(text: &str) -> Option<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(text.as_bytes()).ok()?;
    // `wl-copy` forks a daemon to own the selection and exits, so waiting here is bounded.
    child.wait().ok()?;
    Some(())
}
