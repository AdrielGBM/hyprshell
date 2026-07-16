use std::fs;
use std::path::PathBuf;

const BACKLIGHT_DIR: &str = "/sys/class/backlight";

fn first_backlight_dir() -> Option<PathBuf> {
    fs::read_dir(BACKLIGHT_DIR)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.join("brightness").exists() && p.join("max_brightness").exists())
}

/// The first backlight's brightness as a 0–100 percentage, or `None` when there is no backlight (a desktop) or sysfs is unreadable.
pub fn read() -> Option<i32> {
    let dir = first_backlight_dir()?;
    let read_int = |name: &str| -> Option<i64> {
        fs::read_to_string(dir.join(name)).ok()?.trim().parse().ok()
    };
    let current = read_int("brightness")?;
    let max = read_int("max_brightness")?;
    if max <= 0 {
        return None;
    }
    Some(((current * 100 + max / 2) / max).clamp(0, 100) as i32)
}
