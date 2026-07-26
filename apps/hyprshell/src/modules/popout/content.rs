//! What each chip shows when the pointer rests on it.
//!
//! Every popout here reads a service that already exists and subscribes to it, so the card follows the value
//! while it is up — hovering the volume chip and scrolling it is one gesture, and a card that froze at the
//! level it opened with would be worse than no card. Nothing polls: each `watch` is bound to the popout
//! surface and dies with it.

use rsx::{Color, LayoutError, LayoutItem, ReadSignal, RwSignal, signal};

use super::card::{Card, fixed};
use crate::core::config::Config;
use crate::shared::glyph;
use crate::shared::services::{
    battery, bluetooth, brightness, gpu, hyprland, lockkeys, mpris, netspeed, network, resources,
    volume,
};
use crate::shared::theme::NordTheme;

/// The modules a hover popout is offered for. A module whose click already opens a panel is deliberately
/// included where the popout is the *faster* read of the same state (battery) and left out where the panel is
/// the only sensible presentation (notes, settings, the session menu).
pub const WITH_POPOUT: &[&str] = &[
    "volume",
    "mic",
    "brightness",
    "battery",
    "network",
    "bluetooth",
    "kblayout",
    "lockstatus",
    "activewindow",
    "media",
    "cpu",
    "gpu",
    "memory",
    "temperature",
    "netspeed",
];

pub fn has_popout(module_id: &str) -> bool {
    WITH_POPOUT.contains(&module_id)
}

/// Builds `module_id`'s card. Unknown ids never reach here — [`has_popout`] gates the hover wiring — but an id
/// that slips through gets a warning and no card rather than a mislabelled one.
pub fn build(
    module_id: &str,
    config: &Config,
    theme: NordTheme,
) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
    let card = match module_id {
        "volume" => audio_card(AudioSide::Output, config, theme),
        "mic" => audio_card(AudioSide::Input, config, theme),
        "brightness" => brightness_card(theme),
        "battery" => battery_card(theme),
        "network" => network_card(),
        "bluetooth" => bluetooth_card(theme),
        "kblayout" => keyboard_card(),
        "lockstatus" => lock_card(),
        "activewindow" => window_card(),
        "media" => media_card(),
        "cpu" => cpu_card(theme),
        "gpu" => gpu_card(theme),
        "memory" => memory_card(theme),
        "temperature" => temperature_card(config, theme),
        "netspeed" => netspeed_card(),
        other => {
            tracing::warn!("no popout registered for module '{other}'");
            return None;
        }
    };
    Some(card.build(theme))
}

/// A signal derived from `source` through `map`, recomputed whenever the source changes. The rows of a card
/// are all shaped this way: one subscription per service, several rows off it.
///
/// The value is cloned out before `map` runs, which is not an oversight: reading a signal in place holds the
/// reactive runtime's borrow for as long as the closure does, and these closures translate their own strings —
/// which reads the locale signal and panics on the re-entrant borrow. Reading the locale is also what makes a
/// row re-render on a live language switch, so it has to stay inside `map`.
fn derive<T, U>(source: RwSignal<T>, map: impl Fn(T) -> U + 'static) -> ReadSignal<U>
where
    T: Clone + 'static,
    U: Clone + 'static,
{
    derive_from(source.read_only(), map)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AudioSide {
    Output,
    Input,
}

/// Volume and microphone are the same card: a level, a mute state and the wheel step that moves it. Splitting
/// them would duplicate every row to change one glyph and one string.
fn audio_card(side: AudioSide, config: &Config, theme: NordTheme) -> Card {
    let initial = match side {
        AudioSide::Output => volume::current().unwrap_or(volume::Volume {
            level: 0,
            muted: false,
        }),
        AudioSide::Input => volume::current_mic().unwrap_or(volume::Volume {
            level: 0,
            muted: true,
        }),
    };
    let state = signal(initial);
    let sink = state.clone();
    match side {
        AudioSide::Output => platform_layershell::watch(volume::subscribe, move |v| sink.set(v)),
        AudioSide::Input => platform_layershell::watch(volume::subscribe_mic, move |v| sink.set(v)),
    }

    let ceiling = config.audio.ceiling() as f32;
    let title = match side {
        AudioSide::Output => rsx::t!("popout.volume"),
        AudioSide::Input => rsx::t!("popout.microphone"),
    };
    let glyph = derive(state.clone(), move |v| match side {
        AudioSide::Output => glyph::volume(v).to_string(),
        AudioSide::Input => glyph::microphone(v).to_string(),
    });
    let tint = derive(state.clone(), move |v| {
        if v.muted { theme.muted } else { theme.text }
    });

    Card::titled(title)
        .icon(glyph)
        .icon_tint(tint)
        .subtitle(derive(state.clone(), |v| format!("{}%", v.level)))
        .meter(
            derive(state.clone(), move |v| v.level as f32 / ceiling.max(1.0)),
            derive(state.clone(), move |v| {
                if v.muted { theme.muted } else { theme.accent }
            }),
        )
        .row(
            fixed(rsx::t!("popout.muted")),
            derive(state.clone(), |v| on_off(v.muted)),
        )
        .row(
            fixed(rsx::t!("popout.step")),
            fixed(format!("{}%", config.audio.step())),
        )
}

fn brightness_card(theme: NordTheme) -> Card {
    let level = signal(brightness::current().unwrap_or(0));
    let sink = level.clone();
    platform_layershell::watch(brightness::subscribe, move |percent| sink.set(percent));

    Card::titled(rsx::t!("popout.brightness"))
        .icon(fixed(glyph::brightness()))
        .subtitle(derive(level.clone(), |v| format!("{v}%")))
        .meter(
            derive(level.clone(), |v| v as f32 / 100.0),
            fixed_color(theme.accent),
        )
        .row(
            fixed(rsx::t!("popout.step")),
            fixed(format!(
                "{}%",
                crate::shared::services::brightness::settings().step()
            )),
        )
}

/// The battery card carries what the chip cannot: how long is left, and at what rate. `stream_details` is the
/// same producer the battery panel uses, so hovering and clicking report the same numbers.
fn battery_card(theme: NordTheme) -> Card {
    let details = signal(battery::details());
    let sink = details.clone();
    platform_layershell::watch(battery::stream_details, move |d| sink.set(Some(d)));

    let level = derive(details.clone(), |d| d.map(|d| d.level).unwrap_or(0));
    let charging = derive(details.clone(), |d| {
        d.map(|d| d.state.is_charging()).unwrap_or(false)
    });
    let charging_glyph = charging.clone();
    let charging_tint = charging.clone();
    let level_tint = level.clone();

    Card::titled(rsx::t!("popout.battery"))
        .icon(derive_from(charging_glyph, |c| glyph::battery(c).to_string()))
        .icon_tint(derive_pair(level_tint, charging_tint, move |level, charging| {
            glyph::battery_tint(level, charging, theme, theme.text)
        }))
        .subtitle(derive_from(level.clone(), |l| format!("{l}%")))
        .meter(
            derive_from(level.clone(), |l| l as f32 / 100.0),
            fixed_color(theme.accent),
        )
        .row(
            fixed(rsx::t!("popout.status")),
            derive(details.clone(), |d| match d {
                Some(d) => battery_status(d),
                None => rsx::t!("battery.none"),
            }),
        )
        .row(
            fixed(rsx::t!("popout.rate")),
            derive(details.clone(), |d| match d {
                Some(d) if d.energy_rate > 0.0 => format!("{:.1} W", d.energy_rate),
                _ => rsx::t!("sysinfo.no_reading"),
            }),
        )
}

fn battery_status(d: battery::BatteryDetails) -> String {
    use battery::ChargeState;
    match d.state {
        ChargeState::Charging => match duration_text(d.time_to_full) {
            Some(t) => rsx::t!("battery.until_full", time = t),
            None => rsx::t!("battery.charging"),
        },
        ChargeState::Discharging => match duration_text(d.time_to_empty) {
            Some(t) => rsx::t!("battery.remaining", time = t),
            None => rsx::t!("battery.on_battery"),
        },
        ChargeState::Full => rsx::t!("battery.full"),
        ChargeState::Empty => rsx::t!("battery.empty"),
        ChargeState::Pending => rsx::t!("battery.pending"),
        ChargeState::Unknown => rsx::t!("battery.unknown"),
    }
}

fn duration_text(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    Some(if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    })
}

fn network_card() -> Card {
    let state = signal(network::read());
    let sink = state.clone();
    platform_layershell::watch(network::subscribe, move |net| sink.set(net));

    Card::titled(rsx::t!("popout.network"))
        .icon(derive(state.clone(), |net| glyph::network(net).to_string()))
        .subtitle(derive(state.clone(), |net| kind_label(net.kind)))
        .row(
            fixed(rsx::t!("popout.signal")),
            derive(state.clone(), |net| match net.kind {
                network::NetworkKind::Wifi => format!("{}%", net.signal),
                _ => rsx::t!("sysinfo.no_reading"),
            }),
        )
}

/// `network.rs` reads sysfs for a link verdict and nothing more — no SSID, no interface name — so the card
/// says what the service actually knows. B10 (NetworkManager) is what gives this card something to list.
fn kind_label(kind: network::NetworkKind) -> String {
    match kind {
        network::NetworkKind::Ethernet => rsx::t!("popout.ethernet"),
        network::NetworkKind::Wifi => rsx::t!("popout.wifi"),
        network::NetworkKind::Disconnected => rsx::t!("popout.offline"),
    }
}

/// The chip is one glyph for four states; the popout is where "connected to what, and how much charge is left
/// in it" fits. Which is the question a Bluetooth indicator is actually read for.
fn bluetooth_card(theme: NordTheme) -> Card {
    let state = signal(bluetooth::current().unwrap_or_default());
    let sink = state.clone();
    platform_layershell::watch(bluetooth::subscribe, move |bt| sink.set(bt));

    Card::titled(rsx::t!("bluetooth.title"))
        .icon(derive(state.clone(), |bt| {
            glyph::bluetooth(bt.status()).to_string()
        }))
        .icon_tint(derive(state.clone(), move |bt| {
            glyph::bluetooth_tint(bt.status(), theme, theme.accent, theme.text)
        }))
        .subtitle(derive(state.clone(), |bt| {
            if !bt.available {
                rsx::t!("bluetooth.no_adapter")
            } else if !bt.powered {
                rsx::t!("bluetooth.off")
            } else if bt.discovering {
                rsx::t!("bluetooth.scanning")
            } else {
                rsx::t!("bluetooth.connected_count", count = bt.connected_count().to_string())
            }
        }))
        .row(
            fixed(rsx::t!("popout.status")),
            derive(state.clone(), |bt| {
                if !bt.available {
                    rsx::t!("bluetooth.no_adapter")
                } else if bt.powered {
                    rsx::t!("bluetooth.on")
                } else {
                    rsx::t!("bluetooth.off")
                }
            }),
        )
        .row(
            fixed(rsx::t!("bluetooth.connected")),
            derive(state.clone(), |bt| match bt.primary() {
                Some(device) => device.label(),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
        .row(
            fixed(rsx::t!("popout.battery")),
            derive(state.clone(), |bt| {
                match bt.primary().and_then(|d| d.battery) {
                    Some(level) => format!("{level}%"),
                    None => rsx::t!("sysinfo.no_reading"),
                }
            }),
        )
}

/// The chip shows a two-letter code; the popout is where the layout's full name fits.
fn keyboard_card() -> Card {
    let initial = hyprland::socket_dir()
        .and_then(|dir| hyprland::keyboard_layout(&dir))
        .unwrap_or_default();
    let layout = signal(initial);
    let sink = layout.clone();
    platform_layershell::watch(hyprland::subscribe_keyboard, move |l| sink.set(l));

    Card::titled(rsx::t!("popout.keyboard"))
        .icon(fixed("keyboard"))
        .subtitle(derive(layout.clone(), |l| {
            let name = l.name.trim();
            if name.is_empty() {
                rsx::t!("sysinfo.no_reading")
            } else {
                name.to_string()
            }
        }))
}

fn lock_card() -> Card {
    let keys = signal(lockkeys::current().unwrap_or_else(lockkeys::read));
    let sink = keys.clone();
    platform_layershell::watch(lockkeys::subscribe, move |k| sink.set(k));

    Card::titled(rsx::t!("popout.lock_keys"))
        .icon(derive(keys.clone(), |k| {
            if k.caps { "lock" } else { "lock-open" }.to_string()
        }))
        .row(
            fixed(rsx::t!("popout.caps_lock")),
            derive(keys.clone(), |k| on_off(k.caps)),
        )
        .row(
            fixed(rsx::t!("popout.num_lock")),
            derive(keys.clone(), |k| on_off(k.num)),
        )
}

/// The bar truncates a window title to `max_chars`; the popout carries the whole one, plus the class a title
/// alone doesn't identify.
fn window_card() -> Card {
    let initial = hyprland::socket_dir()
        .map(|dir| hyprland::active_window(&dir))
        .unwrap_or_default();
    let window = signal(initial);
    let sink = window.clone();
    platform_layershell::watch(hyprland::subscribe_active_window, move |w| sink.set(w));

    Card::new(derive(window.clone(), |w| {
        let title = w.title.trim();
        if title.is_empty() {
            rsx::t!("activewindow.none")
        } else {
            title.to_string()
        }
    }))
    .icon(fixed("app-window"))
    .subtitle(derive(window.clone(), |w| non_empty(&w.class)))
}

fn media_card() -> Card {
    let player = signal(mpris::current().unwrap_or_default());
    let sink = player.clone();
    platform_layershell::watch(mpris::subscribe, move |p| sink.set(p));

    Card::new(derive(player.clone(), |p| {
        let title = p.title.trim();
        if title.is_empty() {
            rsx::t!("popout.nothing_playing")
        } else {
            title.to_string()
        }
    }))
    .icon(derive(player.clone(), |p| {
        crate::modules::media::glyph(&p).to_string()
    }))
    .subtitle(derive(player.clone(), |p| p.artist.clone()))
    .row(
        fixed(rsx::t!("popout.album")),
        derive(player.clone(), |p| non_empty(&p.album)),
    )
    .row(
        fixed(rsx::t!("popout.player")),
        derive(player.clone(), |p| non_empty(&p.identity)),
    )
}

fn cpu_card(theme: NordTheme) -> Card {
    let state = resource_signal();
    Card::titled(rsx::t!("sysinfo.cpu"))
        .icon(fixed("cpu"))
        .subtitle(derive(state.clone(), |r| {
            // The model is what identifies the machine, and the popout is the only surface with room for it.
            match r.as_ref().map(|r| r.cpu_model.trim().to_string()) {
                Some(model) if !model.is_empty() => model,
                _ => percent(r.as_ref().map(|r| r.cpu)),
            }
        }))
        .meter(
            derive(state.clone(), |r| {
                r.as_ref().map(|r| r.cpu / 100.0).unwrap_or(0.0)
            }),
            fixed_color(theme.accent),
        )
        .row(
            fixed(rsx::t!("popout.cores")),
            derive(state.clone(), |r| match r {
                Some(r) => r.cores.len().to_string(),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
        .row(
            fixed(rsx::t!("popout.peak")),
            derive(state.clone(), |r| {
                percent(r.as_ref().map(|r| r.cpu_history.peak()))
            }),
        )
        .row(
            fixed(rsx::t!("popout.frequency")),
            derive(state.clone(), |r| match r.and_then(|r| r.cpu_mhz) {
                Some(mhz) if mhz >= 1000.0 => format!("{:.2} GHz", mhz / 1000.0),
                Some(mhz) => format!("{mhz:.0} MHz"),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
}

/// The GPU's card is the CPU's shape with a different set of unknowns: which of usage, temperature and VRAM a
/// card answers is a property of its driver, so each row says "—" rather than a zero it did not measure.
fn gpu_card(theme: NordTheme) -> Card {
    let state = signal(gpu::current().unwrap_or_default());
    let sink = state.clone();
    platform_layershell::watch(gpu::subscribe, move |g| sink.set(g));

    Card::titled(rsx::t!("sysinfo.gpu"))
        .icon(fixed(glyph::gpu()))
        .subtitle(derive(state.clone(), |g| {
            let name = g.name.trim().to_string();
            if name.is_empty() {
                rsx::t!("sysinfo.no_reading")
            } else {
                name
            }
        }))
        .meter(
            derive(state.clone(), |g| g.usage.unwrap_or(0.0) / 100.0),
            fixed_color(theme.accent),
        )
        .row(
            fixed(rsx::t!("popout.usage")),
            derive(state.clone(), |g| percent(g.usage)),
        )
        .row(
            fixed(rsx::t!("popout.sensor")),
            derive(state.clone(), |g| match g.temperature {
                Some(c) => format!("{c:.0} °C"),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
        .row(
            fixed(rsx::t!("popout.vram")),
            derive(state.clone(), |g| match (g.vram_used, g.vram_total) {
                (Some(used), Some(total)) if total > 0 => format!(
                    "{} / {}",
                    resources::format_bytes(used),
                    resources::format_bytes(total)
                ),
                _ => rsx::t!("sysinfo.no_reading"),
            }),
        )
}

fn memory_card(theme: NordTheme) -> Card {
    let state = resource_signal();
    Card::titled(rsx::t!("sysinfo.memory"))
        .icon(fixed("memory-stick"))
        .subtitle(derive(state.clone(), |r| {
            percent(r.as_ref().map(|r| r.memory.used_percent()))
        }))
        .meter(
            derive(state.clone(), |r| {
                r.as_ref()
                    .map(|r| r.memory.used_percent() / 100.0)
                    .unwrap_or(0.0)
            }),
            fixed_color(theme.accent),
        )
        .row(
            fixed(rsx::t!("popout.used")),
            derive(state.clone(), |r| match r {
                Some(r) => format!(
                    "{} / {}",
                    resources::format_bytes(r.memory.used),
                    resources::format_bytes(r.memory.total)
                ),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
        .row(
            fixed(rsx::t!("popout.swap")),
            derive(state.clone(), |r| match r {
                Some(r) if r.memory.swap_total > 0 => format!(
                    "{} / {}",
                    resources::format_bytes(r.memory.swap_used),
                    resources::format_bytes(r.memory.swap_total)
                ),
                _ => rsx::t!("sysinfo.no_reading"),
            }),
        )
        .row(fixed(rsx::t!("popout.disk_io")), disk_row(state))
}

/// Names the sensor the reading came from, which is the one thing `[temperature] sensor` cannot be configured
/// without: the chip shows a number, and only the popout says whose number it is.
fn temperature_card(config: &Config, theme: NordTheme) -> Card {
    let state = resource_signal();
    let settings = config.temperature.clone();
    let unit = settings.unit;
    let (warn, critical) = (settings.warn, settings.critical);
    let wanted = settings.sensor.clone();
    let for_label = wanted.clone();

    let celsius = derive(state.clone(), move |r| {
        r.as_ref().and_then(|r| reading_for(r, &wanted))
    });
    let tint = celsius.clone();
    let meter = celsius.clone();
    let value = celsius.clone();

    Card::titled(rsx::t!("sysinfo.temperature"))
        .icon(fixed("thermometer"))
        .subtitle(derive_from(value, move |c| match c {
            Some(c) => unit.format(c),
            None => rsx::t!("sysinfo.no_reading"),
        }))
        .meter(
            derive_from(meter, move |c| {
                (c.unwrap_or(0.0) / critical.max(1.0)).clamp(0.0, 1.0)
            }),
            derive_from(tint, move |c| match c {
                Some(c) if c >= critical => theme.red,
                Some(c) if c >= warn => theme.yellow,
                _ => theme.accent,
            }),
        )
        .row(
            fixed(rsx::t!("popout.sensor")),
            derive(state.clone(), move |r| {
                sensor_label(r.as_ref(), &for_label)
            }),
        )
        .row(
            fixed(rsx::t!("popout.critical")),
            fixed(unit.format(critical)),
        )
}

/// The configured sensor's reading, or the hottest one — the same fallback the chip uses, so the two never
/// disagree about which sensor is being reported.
fn reading_for(resources: &resources::Resources, wanted: &str) -> Option<f32> {
    if wanted.trim().is_empty() {
        return resources.temperature;
    }
    resources
        .temperature_of(wanted)
        .or(resources.temperature)
}

fn sensor_label(resources: Option<&resources::Resources>, wanted: &str) -> String {
    if !wanted.trim().is_empty() {
        return wanted.trim().to_string();
    }
    let Some(resources) = resources else {
        return rsx::t!("sysinfo.no_reading");
    };
    resources
        .sensors
        .iter()
        .max_by(|a, b| a.celsius.total_cmp(&b.celsius))
        .map(|s| format!("{} {}", s.chip, s.label).trim().to_string())
        .unwrap_or_else(|| rsx::t!("sysinfo.no_reading"))
}

fn netspeed_card() -> Card {
    let state = signal(netspeed::current());
    let sink = state.clone();
    platform_layershell::watch(netspeed::subscribe, move |s| sink.set(Some(s)));

    Card::titled(rsx::t!("popout.throughput"))
        .icon(fixed("arrow-down-up"))
        .row(
            fixed(rsx::t!("popout.down")),
            derive(state.clone(), |s| rate(s.as_ref().map(|s| s.down))),
        )
        .row(
            fixed(rsx::t!("popout.up")),
            derive(state.clone(), |s| rate(s.as_ref().map(|s| s.up))),
        )
        .row(
            fixed(rsx::t!("popout.total")),
            derive(state.clone(), |s| match s {
                Some(s) => format!(
                    "{} / {}",
                    resources::format_bytes(s.total_down),
                    resources::format_bytes(s.total_up)
                ),
                None => rsx::t!("sysinfo.no_reading"),
            }),
        )
}

/// Disk throughput has no chip of its own, so it rides on the memory card — the surface a user checks when the
/// machine feels slow, which is the same question.
fn disk_row(state: RwSignal<Option<resources::Resources>>) -> ReadSignal<String> {
    derive(state, |r| match r {
        Some(r) => format!(
            "{} / {}",
            netspeed::format_rate(r.disk_read),
            netspeed::format_rate(r.disk_write)
        ),
        None => rsx::t!("sysinfo.no_reading"),
    })
}

/// One subscription to the resource service, shared by whichever card asked for it. Three sysinfo popouts read
/// the same snapshot, so they are all the same signal shaped differently.
fn resource_signal() -> RwSignal<Option<resources::Resources>> {
    let state = signal(resources::current());
    let sink = state.clone();
    platform_layershell::watch(resources::subscribe, move |r| sink.set(Some(r)));
    state
}

fn derive_from<T, U>(source: ReadSignal<T>, map: impl Fn(T) -> U + 'static) -> ReadSignal<U>
where
    T: Clone + 'static,
    U: Clone + 'static,
{
    let out = signal(map(source.get()));
    let bound = out.clone();
    rsx::effect(move || bound.set(map(source.get())));
    out.read_only()
}

fn derive_pair<A, B, U>(
    first: ReadSignal<A>,
    second: ReadSignal<B>,
    map: impl Fn(A, B) -> U + 'static,
) -> ReadSignal<U>
where
    A: Clone + 'static,
    B: Clone + 'static,
    U: Clone + 'static,
{
    let out = signal(map(first.get(), second.get()));
    let bound = out.clone();
    rsx::effect(move || bound.set(map(first.get(), second.get())));
    out.read_only()
}

fn fixed_color(color: Color) -> ReadSignal<Color> {
    signal(color).read_only()
}

fn on_off(value: bool) -> String {
    if value {
        rsx::t!("common.on")
    } else {
        rsx::t!("common.off")
    }
}

fn non_empty(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        rsx::t!("sysinfo.no_reading")
    } else {
        text.to_string()
    }
}

fn percent(value: Option<f32>) -> String {
    match value {
        Some(v) => format!("{v:.0}%"),
        None => rsx::t!("sysinfo.no_reading"),
    }
}

fn rate(value: Option<f64>) -> String {
    match value {
        Some(v) => netspeed::format_rate(v),
        None => rsx::t!("sysinfo.no_reading"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_module_offered_a_popout_can_build_one() {
        // `has_popout` gates the hover wiring, so an id listed here with no arm in `build` opens an empty surface.
        for id in WITH_POPOUT {
            assert!(
                matches!(
                    *id,
                    "volume"
                        | "mic"
                        | "brightness"
                        | "battery"
                        | "network"
                        | "bluetooth"
                        | "kblayout"
                        | "lockstatus"
                        | "activewindow"
                        | "media"
                        | "cpu"
                        | "gpu"
                        | "memory"
                        | "temperature"
                        | "netspeed"
                ),
                "'{id}' is offered a popout but `build` has no arm for it"
            );
        }
    }

    #[test]
    fn a_named_sensor_wins_over_the_hottest_and_a_missing_one_falls_back() {
        let r = resources::Resources {
            temperature: Some(40.0),
            sensors: vec![resources::Sensor {
                chip: "k10temp".to_string(),
                label: "Tctl".to_string(),
                celsius: 61.0,
            }],
            ..resources::Resources::default()
        };
        assert_eq!(reading_for(&r, "Tctl"), Some(61.0), "the named sensor is read");
        assert_eq!(
            reading_for(&r, "nonesuch"),
            Some(40.0),
            "an unknown name falls back rather than blanking the card"
        );
        assert_eq!(reading_for(&r, ""), Some(40.0), "unset means the hottest");
    }

    #[test]
    fn the_sensor_row_names_the_configured_sensor_or_the_hottest_one() {
        let r = resources::Resources {
            sensors: vec![
                resources::Sensor {
                    chip: "coretemp".to_string(),
                    label: "Package".to_string(),
                    celsius: 55.0,
                },
                resources::Sensor {
                    chip: "k10temp".to_string(),
                    label: "Tctl".to_string(),
                    celsius: 71.0,
                },
            ],
            ..resources::Resources::default()
        };
        assert_eq!(sensor_label(Some(&r), " Tctl "), "Tctl");
        assert_eq!(sensor_label(Some(&r), ""), "k10temp Tctl");
    }
}
