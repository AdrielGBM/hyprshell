use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use platform_layershell::EventSender;
use zbus::blocking::{Connection, fdo::PropertiesProxy};

const SUPPLY_DIR: &str = "/sys/class/power_supply";
const UPOWER: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

/// A battery reading from sysfs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    /// Charge level 0–100.
    pub level: i32,
    pub charging: bool,
}

fn first_battery_dir() -> Option<PathBuf> {
    let entries = fs::read_dir(SUPPLY_DIR).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .find(|p| fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery"))
}

/// Reads the first battery's level and charging state, or `None` when there is no battery (a desktop) or
/// sysfs is unreadable. `Full` and `Charging` both count as charging (the cable is in).
pub fn read() -> Option<Battery> {
    let dir = first_battery_dir()?;
    let level = fs::read_to_string(dir.join("capacity"))
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()?
        .clamp(0, 100);
    let status = fs::read_to_string(dir.join("status")).unwrap_or_default();
    let charging = matches!(status.trim(), "Charging" | "Full");
    Some(Battery { level, charging })
}

/// Streams battery updates to `tx`, blocking on UPower's `PropertiesChanged` signal for the aggregate
/// DisplayDevice — sub-second on plug/unplug, the way waybar/astal do it, instead of a slow poll. UPower is
/// only the trigger; sysfs holds the authoritative values and is current when it fires. Falls back to a slow
/// sysfs poll if UPower/DBus is unavailable (a headless box, or no `upowerd`). Runs on the `watch` producer
/// thread and returns when the channel closes.
pub fn stream(tx: EventSender<Battery>) {
    // Push the current value immediately so the bar doesn't wait for the first change.
    if let Some(b) = read()
        && !tx.send(b)
    {
        return;
    }
    if watch_upower(&tx).is_none() {
        poll_fallback(&tx);
    }
}

fn watch_upower(tx: &EventSender<Battery>) -> Option<()> {
    let conn = Connection::system().ok()?;
    let props = PropertiesProxy::builder(&conn)
        .destination(UPOWER)
        .ok()?
        .path(DISPLAY_DEVICE)
        .ok()?
        .build()
        .ok()?;
    let changes = props.receive_properties_changed().ok()?;
    for _ in changes {
        match read() {
            Some(b) if tx.send(b) => {}
            _ => return Some(()),
        }
    }
    Some(())
}

/// Belt-and-suspenders when UPower is missing: the pre-existing 30 s sysfs poll.
fn poll_fallback(tx: &EventSender<Battery>) {
    loop {
        std::thread::sleep(Duration::from_secs(30));
        match read() {
            Some(b) if tx.send(b) => {}
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live check that the UPower DBus plumbing (bus, destination, path) is correct — gated behind an env
    // var and a running `upowerd`, so it never runs in a headless CI. Run with:
    //   HYPRSHELL_TEST_UPOWER=1 cargo test -p hyprshell --lib upower -- --nocapture
    #[test]
    fn upower_connection_reads_percentage() {
        if std::env::var("HYPRSHELL_TEST_UPOWER").is_err() {
            return;
        }
        let conn = Connection::system().expect("system bus");
        let props = PropertiesProxy::builder(&conn)
            .destination(UPOWER)
            .unwrap()
            .path(DISPLAY_DEVICE)
            .unwrap()
            .build()
            .expect("build DisplayDevice proxy");
        let pct = props
            .get(
                "org.freedesktop.UPower.Device".try_into().unwrap(),
                "Percentage",
            )
            .expect("read Percentage");
        eprintln!("UPower DisplayDevice Percentage = {pct:?}");
    }
}
