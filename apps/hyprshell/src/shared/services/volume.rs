use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use platform_layershell::EventSender;

use crate::core::config::AudioConfig;
use crate::shared::services::broadcast::{Broadcast, Service};

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

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

/// Reads a `wpctl` node's volume, or `None` when PipeWire/`wpctl` is unavailable.
fn read_node(node: &str) -> Option<Volume> {
    let out = Command::new("wpctl")
        .args(["get-volume", node])
        .output()
        .ok()?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// Reads the default sink's volume, or `None` when PipeWire/`wpctl` is unavailable.
pub fn read() -> Option<Volume> {
    read_node(SINK)
}

/// Reads the default source (microphone), or `None` when there isn't one.
pub fn read_mic() -> Option<Volume> {
    read_node(SOURCE)
}

static VOLUME: Service<Volume> = Service::new("hyprshell-volume", run);
static MIC: Service<Volume> = Service::new("hyprshell-mic", run_mic);

/// One poll loop per node, started only when something subscribes: a shell with no microphone chip never runs
/// the microphone poller.
fn poll_into(out: &Arc<Broadcast<Volume>>, read: fn() -> Option<Volume>) {
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

fn run(out: &Arc<Broadcast<Volume>>) {
    poll_into(out, read);
}

fn run_mic(out: &Arc<Broadcast<Volume>>) {
    poll_into(out, read_mic);
}

/// Registers `tx` for live volume readings, starting the single shared producer on first use. Called from a bar
/// chip's `watch` producer.
pub fn subscribe(tx: EventSender<Volume>) {
    VOLUME.subscribe(tx);
}

/// Registers `tx` for live microphone readings, starting the microphone poller on first use.
pub fn subscribe_mic(tx: EventSender<Volume>) {
    MIC.subscribe(tx);
}

/// The last known reading, with no subprocess — what a UI handler steps from.
pub fn current() -> Option<Volume> {
    VOLUME.current()
}

pub fn current_mic() -> Option<Volume> {
    MIC.current()
}

/// The running `[audio]` settings, or the defaults outside a started shell (a unit test, a service thread —
/// [`crate::core::shell::config`] lives on the driver thread, which is where every caller of this runs).
pub fn settings() -> AudioConfig {
    crate::core::shell::config()
        .map(|c| c.audio)
        .unwrap_or_default()
}

/// Steps the volume by `delta` percentage points from the last known level.
pub fn step(delta: i32) {
    if let Some(v) = current() {
        set(v.level + delta);
    }
}

pub fn step_mic(delta: i32) {
    if let Some(v) = current_mic() {
        set_mic(v.level + delta);
    }
}

/// Runs a `wpctl` mutation off the UI thread — a blocking `fork`/`exec` in a click handler would stall the
/// frame — and publishes the resulting reading so every chip updates immediately instead of at the next poll.
fn apply(args: Vec<String>, node: &'static str) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-volume-set".to_string())
        .spawn(move || {
            let _ = Command::new("wpctl").args(&args).status();
            match read_node(node) {
                Some(v) if node == SINK => VOLUME.publish(v),
                Some(v) => MIC.publish(v),
                None => {}
            }
        });
}

pub fn toggle_mute() {
    apply(vec!["set-mute".into(), SINK.into(), "toggle".into()], SINK);
}

pub fn toggle_mic_mute() {
    apply(
        vec!["set-mute".into(), SOURCE.into(), "toggle".into()],
        SOURCE,
    );
}

/// Sets the default sink's volume to `level` percent, clamped to `[audio] max_volume`. Publishes the target
/// optimistically before `wpctl` has run, so a scroll notch moves the chip and the OSD on the same frame instead
/// of a round-trip later; the reading that follows the command reconciles whatever the sink actually accepted.
pub fn set(level: i32) {
    let level = level.clamp(0, settings().ceiling());
    let muted = current().is_some_and(|v| v.muted);
    VOLUME.publish(Volume { level, muted });
    apply(
        vec!["set-volume".into(), SINK.into(), format!("{level}%")],
        SINK,
    );
}

/// A microphone has no reason to be boosted past its own maximum, so this clamps to 0–100 rather than the
/// sink's 0–150.
pub fn set_mic(level: i32) {
    let level = level.clamp(0, 100);
    let muted = current_mic().is_some_and(|v| v.muted);
    MIC.publish(Volume { level, muted });
    apply(
        vec!["set-volume".into(), SOURCE.into(), format!("{level}%")],
        SOURCE,
    );
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
