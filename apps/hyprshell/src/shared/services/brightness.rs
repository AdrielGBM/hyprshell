use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use platform_layershell::EventSender;
use zbus::blocking::Connection;

use crate::core::config::BrightnessConfig;
use crate::shared::services::broadcast::{Broadcast, Service};

const BACKLIGHT_DIR: &str = "/sys/class/backlight";
const LOGIND: &str = "org.freedesktop.login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";
/// Poll used only when `udevadm` can't be spawned, so the level still tracks external changes eventually.
const FALLBACK_POLL: Duration = Duration::from_secs(5);

fn first_backlight_dir() -> Option<PathBuf> {
    fs::read_dir(BACKLIGHT_DIR)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.join("brightness").exists() && p.join("max_brightness").exists())
}

fn read_int(dir: &Path, name: &str) -> Option<i64> {
    fs::read_to_string(dir.join(name)).ok()?.trim().parse().ok()
}

/// The first backlight's brightness as a 0–100 percentage, or `None` when there is no backlight (a desktop) or sysfs is unreadable.
pub fn read() -> Option<i32> {
    let dir = first_backlight_dir()?;
    let current = read_int(&dir, "brightness")?;
    let max = read_int(&dir, "max_brightness")?;
    if max <= 0 {
        return None;
    }
    Some(((current * 100 + max / 2) / max).clamp(0, 100) as i32)
}

static BRIGHTNESS: Service<i32> = Service::new("hyprshell-brightness", run);

/// Registers `tx` for live brightness readings, starting the single shared producer on first use. Called from a
/// bar chip's `watch` producer.
pub fn subscribe(tx: EventSender<i32>) {
    BRIGHTNESS.subscribe(tx);
}

fn run(out: &Arc<Broadcast<i32>>) {
    let Some(level) = read() else {
        // No backlight (a desktop): nothing to report and nothing to watch, so the producer retires instead of
        // spinning. The chip keeps whatever it seeded with.
        return;
    };
    out.publish(level);
    if watch_udev(out).is_none() {
        poll_fallback(out);
    }
}

/// Follows `udevadm monitor` for backlight uevents — the kernel emits one whenever the brightness changes,
/// whoever changed it — so the chip tracks function keys and other tools without polling sysfs.
fn watch_udev(out: &Broadcast<i32>) -> Option<()> {
    let mut child = Command::new("udevadm")
        .args(["monitor", "--udev", "--subsystem-match=backlight"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let mut last = read();
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        // `udevadm monitor` prints a header before any events; only device lines concern us.
        if !line.contains("backlight") {
            continue;
        }
        let current = read();
        if current != last {
            last = current;
            if let Some(level) = current {
                out.publish(level);
            }
        }
    }
    let _ = child.kill();
    Some(())
}

fn poll_fallback(out: &Broadcast<i32>) {
    let mut last = read();
    loop {
        std::thread::sleep(FALLBACK_POLL);
        let current = read();
        if current != last {
            last = current;
            if let Some(level) = current {
                out.publish(level);
            }
        }
    }
}

/// Sets the first backlight to `percent`, via logind's `SetBrightness` — which is permitted for the active
/// session's own seat, so it needs neither root nor a udev rule granting write access to sysfs. Publishes the
/// new level immediately so the chip and OSD don't wait for the uevent to come back around.
pub fn set(percent: i32) {
    let Some(dir) = first_backlight_dir() else {
        return;
    };
    let Some(max) = read_int(&dir, "max_brightness").filter(|m| *m > 0) else {
        return;
    };
    let Some(device) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
        return;
    };
    let percent = percent.clamp(0, 100);
    let absolute = (max * percent as i64 / 100).clamp(0, max) as u32;
    // Published before the D-Bus call so a scroll notch moves the chip and OSD on the same frame; the uevent
    // that follows carries the level the kernel actually applied.
    BRIGHTNESS.publish(percent);
    let _ = std::thread::Builder::new()
        .name("hyprshell-brightness-set".to_string())
        .spawn(move || {
            if write_logind(&device, absolute).is_none() {
                tracing::warn!("brightness: logind SetBrightness failed for '{device}'");
            }
        });
}

fn write_logind(device: &str, absolute: u32) -> Option<()> {
    let conn = Connection::system().ok()?;
    conn.call_method(
        Some(LOGIND),
        SESSION_PATH,
        Some(SESSION_IFACE),
        "SetBrightness",
        &("backlight", device, absolute),
    )
    .ok()?;
    Some(())
}

/// The last known reading, falling back to sysfs before the producer has published anything.
pub fn current() -> Option<i32> {
    BRIGHTNESS.current().or_else(read)
}

/// The running `[brightness]` settings, or the defaults outside a started shell (a unit test, a service thread
/// — [`crate::core::shell::config`] lives on the driver thread, where every caller of this runs).
pub fn settings() -> BrightnessConfig {
    crate::core::shell::config()
        .map(|c| c.brightness)
        .unwrap_or_default()
}

/// Steps the brightness by `delta` percentage points from the current level; the shape a scroll or key-repeat
/// gesture wants. No-op on a machine with no backlight.
pub fn step(delta: i32) {
    if let Some(level) = current() {
        set(level + delta);
    }
}
