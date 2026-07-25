use std::process::Command;
use std::time::Duration;

use platform_layershell::EventSender;

use crate::shared::services::broadcast::{Broadcast, Service};

const SINK: &str = "@DEFAULT_AUDIO_SINK@";

/// How often the shared producer re-reads the sink. PipeWire has no event source we can subscribe to without
/// linking libpipewire/libpulse, so this is a poll — but a single one for the whole shell, not one per bar. The
/// shell's own changes don't wait for it: [`toggle_mute`] and [`set`] publish as soon as `wpctl` returns.
const POLL: Duration = Duration::from_secs(2);

/// The default sink's volume as a percentage and mute state, read via `wpctl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume {
    /// 0–100 (may read above 100 if boosted; callers clamp for display).
    pub level: i32,
    pub muted: bool,
}

/// Parses `wpctl get-volume`'s output: `Volume: 0.20`, or `Volume: 0.20 [MUTED]` when muted. Split out so the
/// format assumption is tested rather than inferred from a positional index at the call site.
fn parse(text: &str) -> Option<Volume> {
    let fraction: f32 = text
        .split_whitespace()
        .find_map(|word| word.parse::<f32>().ok())?;
    Some(Volume {
        level: (fraction * 100.0).round() as i32,
        muted: text.contains("[MUTED]"),
    })
}

/// Reads the default sink's volume, or `None` when PipeWire/`wpctl` is unavailable.
pub fn read() -> Option<Volume> {
    let out = Command::new("wpctl")
        .args(["get-volume", SINK])
        .output()
        .ok()?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

static VOLUME: Service<Volume> = Service::new("hyprshell-volume", run);

fn run(out: &Broadcast<Volume>) {
    let mut last = None;
    loop {
        let current = read();
        if let Some(v) = current
            && current != last
        {
            out.publish(v);
        }
        last = current;
        std::thread::sleep(POLL);
    }
}

/// Registers `tx` for live volume readings, starting the single shared producer on first use. Called from a bar
/// chip's `watch` producer.
pub fn subscribe(tx: EventSender<Volume>) {
    VOLUME.subscribe(tx);
}

/// The last known reading, with no subprocess — what a UI handler steps from.
pub fn current() -> Option<Volume> {
    VOLUME.current()
}

/// Steps the volume by `delta` percentage points from the last known level.
pub fn step(delta: i32) {
    if let Some(v) = current() {
        set(v.level + delta);
    }
}

/// Runs a `wpctl` mutation off the UI thread — a blocking `fork`/`exec` in a click handler would stall the
/// frame — and publishes the resulting reading so every chip updates immediately instead of at the next poll.
fn apply(args: Vec<String>) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-volume-set".to_string())
        .spawn(move || {
            let _ = Command::new("wpctl").args(&args).status();
            if let Some(v) = read() {
                VOLUME.publish(v);
            }
        });
}

pub fn toggle_mute() {
    apply(vec!["set-mute".into(), SINK.into(), "toggle".into()]);
}

/// Sets the default sink's volume to `level` percent (clamped to 0–150). Publishes the target optimistically
/// before `wpctl` has run, so a scroll notch moves the chip and the OSD on the same frame instead of a
/// round-trip later; the reading that follows the command reconciles whatever the sink actually accepted.
pub fn set(level: i32) {
    let level = level.clamp(0, 150);
    let muted = current().is_some_and(|v| v.muted);
    VOLUME.publish(Volume { level, muted });
    apply(vec!["set-volume".into(), SINK.into(), format!("{level}%")]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_output_with_and_without_mute() {
        assert_eq!(
            parse("Volume: 0.20\n"),
            Some(Volume {
                level: 20,
                muted: false
            })
        );
        assert_eq!(
            parse("Volume: 0.55 [MUTED]\n"),
            Some(Volume {
                level: 55,
                muted: true
            })
        );
        // Boosted sinks read above 1.0; the level is reported as-is and clamped for display by callers.
        assert_eq!(parse("Volume: 1.40\n").unwrap().level, 140);
        assert_eq!(parse(""), None, "no output at all (wpctl missing)");
        assert_eq!(parse("no numbers here"), None);
    }
}
