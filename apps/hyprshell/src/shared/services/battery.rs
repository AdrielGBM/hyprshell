use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use platform_layershell::EventSender;
use zbus::blocking::{Connection, fdo::PropertiesProxy};

const SUPPLY_DIR: &str = "/sys/class/power_supply";
const UPOWER: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

/// A battery reading from sysfs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    /// Charge level 0–100.
    pub level: i32,
    pub charging: bool,
}

/// UPower's charge state, mapped from its numeric `State` property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChargeState {
    Charging,
    Discharging,
    Full,
    Empty,
    Pending,
    Unknown,
}

impl ChargeState {
    fn from_upower(state: u32) -> Self {
        match state {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::Full,
            5 | 6 => Self::Pending,
            _ => Self::Unknown,
        }
    }

    /// A human label for the detail panel; `Discharging` reads as "On battery" since that's the state users recognise.
    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::Discharging => "On battery",
            Self::Full => "Fully charged",
            Self::Empty => "Empty",
            Self::Pending => "Pending",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether the battery icon should show the charging glyph; `Full` counts, mirroring the sysfs `read`.
    pub fn is_charging(self) -> bool {
        matches!(self, Self::Charging | Self::Full)
    }
}

/// The richer reading the detail panel shows, sourced from UPower's DisplayDevice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatteryDetails {
    /// Charge level 0–100.
    pub level: i32,
    pub state: ChargeState,
    /// Seconds until empty while discharging; 0 when unknown or not applicable.
    pub time_to_empty: i64,
    /// Seconds until full while charging; 0 when unknown or not applicable.
    pub time_to_full: i64,
    /// Charge/discharge rate in watts; 0 when unknown.
    pub energy_rate: f64,
}

fn first_battery_dir() -> Option<PathBuf> {
    let entries = fs::read_dir(SUPPLY_DIR).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .find(|p| fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery"))
}

/// Reads the first battery's level and charging state, or `None` when there is no battery (a desktop) or sysfs is unreadable; `Full` and `Charging` both count as charging.
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

/// Streams battery updates to `tx`, blocking on UPower's DisplayDevice `PropertiesChanged` signal for sub-second plug/unplug updates (UPower only triggers; sysfs holds the authoritative values); falls back to a slow sysfs poll if UPower/DBus is unavailable.
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

/// Reads the full battery detail for the panel: UPower's DisplayDevice when available (level, state, time-to-empty/full, power draw), else a sysfs-only reading with no time/rate; `None` on a machine with no battery.
pub fn details() -> Option<BatteryDetails> {
    upower_details().or_else(sysfs_details)
}

fn read_details(props: &PropertiesProxy) -> Option<BatteryDetails> {
    let get_f64 = |name: &str| -> Option<f64> {
        f64::try_from(props.get(DEVICE_IFACE.try_into().ok()?, name).ok()?).ok()
    };
    let get_i64 = |name: &str| -> Option<i64> {
        i64::try_from(props.get(DEVICE_IFACE.try_into().ok()?, name).ok()?).ok()
    };
    let get_u32 = |name: &str| -> Option<u32> {
        u32::try_from(props.get(DEVICE_IFACE.try_into().ok()?, name).ok()?).ok()
    };
    // Percentage is the one property every real battery reports; its absence means DisplayDevice isn't a battery (a desktop), so bail to the sysfs path.
    let level = get_f64("Percentage")?.round().clamp(0.0, 100.0) as i32;
    Some(BatteryDetails {
        level,
        state: ChargeState::from_upower(get_u32("State").unwrap_or(0)),
        time_to_empty: get_i64("TimeToEmpty").unwrap_or(0),
        time_to_full: get_i64("TimeToFull").unwrap_or(0),
        energy_rate: get_f64("EnergyRate").unwrap_or(0.0),
    })
}

fn upower_details() -> Option<BatteryDetails> {
    let conn = Connection::system().ok()?;
    let props = PropertiesProxy::builder(&conn)
        .destination(UPOWER)
        .ok()?
        .path(DISPLAY_DEVICE)
        .ok()?
        .build()
        .ok()?;
    read_details(&props)
}

fn sysfs_details() -> Option<BatteryDetails> {
    let b = read()?;
    Some(BatteryDetails {
        level: b.level,
        state: if b.charging {
            ChargeState::Charging
        } else {
            ChargeState::Discharging
        },
        time_to_empty: 0,
        time_to_full: 0,
        energy_rate: 0.0,
    })
}

/// Streams battery detail to `tx`: seeds immediately, then re-reads UPower's DisplayDevice on each `PropertiesChanged` (sub-second on plug/unplug), falling back to a slow poll when UPower/DBus is unavailable. The detail-panel counterpart to [`stream`].
pub fn stream_details(tx: EventSender<BatteryDetails>) {
    if let Some(d) = details()
        && !tx.send(d)
    {
        return;
    }
    if watch_upower_details(&tx).is_none() {
        poll_details_fallback(&tx);
    }
}

fn watch_upower_details(tx: &EventSender<BatteryDetails>) -> Option<()> {
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
        match read_details(&props) {
            Some(d) if tx.send(d) => {}
            _ => return Some(()),
        }
    }
    Some(())
}

fn poll_details_fallback(tx: &EventSender<BatteryDetails>) {
    loop {
        std::thread::sleep(Duration::from_secs(30));
        match details() {
            Some(d) if tx.send(d) => {}
            _ => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live UPower DBus check, gated behind an env var so it never runs in headless CI: run with `HYPRSHELL_TEST_UPOWER=1 cargo test -p hyprshell --lib upower -- --nocapture`.
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
