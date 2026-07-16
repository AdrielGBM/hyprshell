use std::process::Command;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";

/// The default sink's volume as a percentage and mute state, read via `wpctl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume {
    /// 0–100 (may read above 100 if boosted; callers clamp for display).
    pub level: i32,
    pub muted: bool,
}

/// Reads the default sink's volume, or `None` when PipeWire/`wpctl` is unavailable.
pub fn read() -> Option<Volume> {
    // `wpctl get-volume @DEFAULT_AUDIO_SINK@` prints e.g. "Volume: 0.20" or "Volume: 0.20 [MUTED]".
    let out = Command::new("wpctl").args(["get-volume", SINK]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let muted = text.contains("[MUTED]");
    let fraction: f32 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(Volume {
        level: (fraction * 100.0).round() as i32,
        muted,
    })
}

pub fn toggle_mute() {
    let _ = Command::new("wpctl")
        .args(["set-mute", SINK, "toggle"])
        .status();
}

/// Sets the default sink's volume to `level` percent (clamped to 0–150).
pub fn set(level: i32) {
    let pct = format!("{}%", level.clamp(0, 150));
    let _ = Command::new("wpctl").args(["set-volume", SINK, &pct]).status();
}
