//! What the focused window is called, and how much of that fits on a bar.

use config::ActiveWindowConfig;
use services::hyprland::{self, ActiveWindow};

/// The text the chip shows: the window's title, or its class when it has no title, trimmed to `max_chars` with
/// an ellipsis. A window's title is the one bar value with no natural bound — a browser tab can be a paragraph —
/// so it is truncated here rather than left to squash the rest of the bar.
pub fn label(window: &ActiveWindow, config: &ActiveWindowConfig) -> String {
    let text = if window.title.is_empty() {
        window.class.as_str()
    } else {
        window.title.as_str()
    };
    truncate(text, config.max_chars as usize)
}

/// The label in `compact` mode: the app's class rather than the document title, which is what fits on a narrow
/// bar and stays stable while the user moves around inside one app.
pub fn compact_label(window: &ActiveWindow, config: &ActiveWindowConfig) -> String {
    truncate(&window.class, config.max_chars as usize)
}

/// Cuts `text` to `max` characters, ending in `…` when it had to. Counts characters rather than bytes, so a
/// title with accents or CJK is not cut mid-codepoint.
fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// The chip's leading (or trailing) visual: the focused application's own artwork at `size`, or an empty box
/// when its class has no installed icon.
///
/// A function rather than a bound widget because the view places it on one of two sides depending on
/// `inverted`, and a `widget` binding is a *value* — placeable once. Each `build` site calls this and gets its
/// own node, which is the rule the view DSL documents.
pub fn icon_slot(class: &str, size: f32) -> Result<Box<dyn telar::LayoutItem>, telar::LayoutError> {
    match ui::icon::app_icon_view(class, size)? {
        Some(icon) => Ok(icon),
        None => Ok(telar::box_item(telar::Container::new(
            telar::LayoutStyle::new(),
            vec![],
        )?)),
    }
}

/// Focuses the window the chip is showing — clicking the title takes you back to it, which is what the chip
/// looks like it should do.
pub fn focus_active() {
    let Some(dir) = hyprland::socket_dir() else {
        return;
    };
    let window = hyprland::active_window(&dir);
    if !window.is_empty() {
        hyprland::focus_window(&dir, &window.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(title: &str, class: &str) -> ActiveWindow {
        ActiveWindow {
            title: title.to_string(),
            class: class.to_string(),
            address: "0x1".to_string(),
        }
    }

    #[test]
    fn the_label_prefers_the_title_and_falls_back_to_the_class() {
        let config = ActiveWindowConfig::default();
        assert_eq!(label(&window("Inbox", "thunderbird"), &config), "Inbox");
        assert_eq!(
            label(&window("", "thunderbird"), &config),
            "thunderbird",
            "a window with no title still names itself"
        );
    }

    #[test]
    fn long_titles_are_cut_to_the_configured_width() {
        let config = ActiveWindowConfig {
            max_chars: 10,
            ..ActiveWindowConfig::default()
        };
        assert_eq!(
            label(&window("A very long window title", "x"), &config),
            "A very lo…"
        );
        assert_eq!(
            label(&window("Exactly10!", "x"), &config),
            "Exactly10!",
            "a title that already fits is untouched"
        );
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Cutting by byte offset here would split a multi-byte codepoint and panic.
        assert_eq!(truncate("áéíóúñüàè", 5), "áéíó…");
        assert_eq!(truncate("日本語のタイトル", 4), "日本語…");
        assert_eq!(truncate("anything", 0), "", "a zero width shows nothing");
    }

    #[test]
    fn compact_mode_shows_the_app_not_the_document() {
        let config = ActiveWindowConfig::default();
        assert_eq!(
            compact_label(&window("Docs — Firefox", "firefox"), &config),
            "firefox"
        );
    }
}
