//! The glyph and tint each service's state reads as.
//!
//! One home for it because the same state is drawn in four places — its own bar chip, the OSD, a hover popout
//! and the status cluster — and four copies of "which icon means muted" drift. They already had: the volume
//! chip and the OSD carried separate copies of the same three-way glyph, and the battery was tinted by three
//! different rules depending on which surface you looked at.

use rsx::Color;

use crate::shared::services::network::{Network, NetworkKind};
use crate::shared::services::volume::Volume;
use crate::shared::theme::NordTheme;

/// Muted wins over the level, because it is the state that matters at a glance; below that the glyph tracks
/// how far the sink is turned down.
pub fn volume(v: Volume) -> &'static str {
    if v.muted || v.level == 0 {
        "volume-x"
    } else if v.level < 50 {
        "volume-1"
    } else {
        "volume-2"
    }
}

pub fn microphone(v: Volume) -> &'static str {
    if v.muted || v.level == 0 {
        "mic-off"
    } else {
        "mic"
    }
}

pub fn brightness() -> &'static str {
    "sun"
}

/// A wired port, an off symbol when down, or a Wi-Fi arc whose fill tracks the signal strength.
pub fn network(net: Network) -> &'static str {
    match net.kind {
        NetworkKind::Ethernet => "ethernet-port",
        NetworkKind::Disconnected => "wifi-off",
        NetworkKind::Wifi => match net.signal {
            s if s >= 70 => "wifi",
            s if s >= 45 => "wifi-high",
            s if s >= 20 => "wifi-low",
            _ => "wifi-zero",
        },
    }
}

pub fn battery(charging: bool) -> &'static str {
    if charging {
        "battery-charging"
    } else {
        "battery"
    }
}

/// Charging reads green, a low charge warns, and anything else takes the surrounding foreground so the icon
/// sits with its neighbours. `fg` is what the caller would otherwise paint with — a chip's own foreground,
/// which follows the container variant, or a panel's text token.
pub fn battery_tint(level: i32, charging: bool, theme: NordTheme, fg: Color) -> Color {
    if charging {
        theme.green
    } else if level <= 15 {
        theme.red
    } else if level <= 30 {
        theme.yellow
    } else {
        fg
    }
}

pub fn caps_lock() -> &'static str {
    "mdi:apple-keyboard-caps"
}

pub fn num_lock() -> &'static str {
    "mdi:numeric"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muting_wins_over_the_level_in_both_directions() {
        let loud_but_muted = Volume {
            level: 90,
            muted: true,
        };
        assert_eq!(volume(loud_but_muted), "volume-x");
        assert_eq!(microphone(loud_but_muted), "mic-off");
        // A sink at zero is muted in every way that matters to someone reading the bar.
        let silent = Volume {
            level: 0,
            muted: false,
        };
        assert_eq!(volume(silent), "volume-x");
        assert_eq!(microphone(silent), "mic-off");
    }

    #[test]
    fn the_wifi_arc_fills_with_the_signal() {
        let at = |signal| {
            network(Network {
                kind: NetworkKind::Wifi,
                signal,
            })
        };
        assert_eq!(at(90), "wifi");
        assert_eq!(at(50), "wifi-high");
        assert_eq!(at(30), "wifi-low");
        assert_eq!(at(5), "wifi-zero");
        assert_eq!(
            network(Network {
                kind: NetworkKind::Ethernet,
                signal: 0
            }),
            "ethernet-port",
            "a wired link ignores the signal field it does not have"
        );
    }

    #[test]
    fn charging_outranks_a_low_charge_in_the_battery_tint() {
        let theme = NordTheme::new();
        assert_eq!(battery_tint(5, true, theme, theme.text), theme.green);
        assert_eq!(battery_tint(5, false, theme, theme.text), theme.red);
        assert_eq!(battery_tint(25, false, theme, theme.text), theme.yellow);
        assert_eq!(
            battery_tint(80, false, theme, theme.text),
            theme.text,
            "a healthy battery takes the surrounding foreground rather than a colour of its own"
        );
    }
}
