use std::fs;
use std::path::Path;

const NET_DIR: &str = "/sys/class/net";
const WIRELESS_STATUS: &str = "/proc/net/wireless";
/// `/proc/net/wireless` reports link quality on a 0–70 scale on most drivers; used to normalise it to a percentage.
const LINK_QUALITY_MAX: f32 = 70.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkKind {
    Ethernet,
    Wifi,
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Network {
    pub kind: NetworkKind,
    /// Wi-Fi signal strength 0–100; 0 for ethernet and disconnected.
    pub signal: i32,
}

/// Reads the current network state from sysfs and `/proc/net/wireless`: a wired link wins when present (it's the active route), otherwise the first associated Wi-Fi interface with its signal strength, otherwise disconnected. Dependency-free — no NetworkManager required.
pub fn read() -> Network {
    let mut wifi: Option<i32> = None;
    let mut ethernet = false;
    if let Ok(entries) = fs::read_dir(NET_DIR) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == "lo" || !is_physical(name) || operstate(name) != "up" {
                continue;
            }
            if is_wireless(name) {
                wifi = Some(wifi_signal(name).unwrap_or(0));
            } else {
                ethernet = true;
            }
        }
    }
    match (ethernet, wifi) {
        (true, _) => Network {
            kind: NetworkKind::Ethernet,
            signal: 0,
        },
        (false, Some(signal)) => Network {
            kind: NetworkKind::Wifi,
            signal,
        },
        (false, None) => Network {
            kind: NetworkKind::Disconnected,
            signal: 0,
        },
    }
}

/// A real NIC has a backing device on a bus; virtual interfaces (`docker0`, `veth*`, VPN `tun*`) don't, so this keeps `read` to physical links and stops a bridge from masquerading as a wired connection.
fn is_physical(iface: &str) -> bool {
    Path::new(NET_DIR).join(iface).join("device").exists()
}

fn is_wireless(iface: &str) -> bool {
    let base = Path::new(NET_DIR).join(iface);
    base.join("wireless").exists() || base.join("phy80211").exists()
}

fn operstate(iface: &str) -> String {
    fs::read_to_string(Path::new(NET_DIR).join(iface).join("operstate"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Wi-Fi signal as a 0–100 percentage from `/proc/net/wireless`' link-quality column, or `None` when the interface isn't listed there.
fn wifi_signal(iface: &str) -> Option<i32> {
    let status = fs::read_to_string(WIRELESS_STATUS).ok()?;
    for line in status.lines() {
        let Some(rest) = line.trim_start().strip_prefix(iface) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        // Columns after "iface:" are: status, link-quality, level, noise.
        let link: f32 = rest
            .split_whitespace()
            .nth(1)?
            .trim_end_matches('.')
            .parse()
            .ok()?;
        return Some((link / LINK_QUALITY_MAX * 100.0).round().clamp(0.0, 100.0) as i32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_never_panics_and_is_self_consistent() {
        let net = read();
        // Only Wi-Fi carries a meaningful signal; the other kinds report zero.
        if net.kind != NetworkKind::Wifi {
            assert_eq!(net.signal, 0);
        }
        assert!((0..=100).contains(&net.signal));
    }
}
