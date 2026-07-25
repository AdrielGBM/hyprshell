//! What is playing, on the bar.

use crate::core::config::{MediaConfig, MediaScroll};
use crate::shared::services::mpris::{self, Playback, Player};

/// The glyph for a player's transport state. Stopped and absent look the same on purpose: a chip that shows a
/// pause symbol for a player that isn't running would invite a click that does nothing.
pub fn glyph(player: &Player) -> &'static str {
    match player.playback {
        Playback::Playing => "pause",
        Playback::Paused => "play",
        Playback::Stopped => "music",
    }
}

/// The chip's text: `artist — title`, trimmed to `max_chars`. Empty when nothing is running, which lets the
/// module collapse to just its icon instead of showing a placeholder.
pub fn label(player: &Player, config: &MediaConfig) -> String {
    if player.is_empty() {
        return String::new();
    }
    truncate(&player.summary(), config.max_chars as usize)
}

/// Cuts `text` to `max` characters, ending in `…` when it had to. Counts characters, not bytes, so a track
/// title with accents or CJK is never cut mid-codepoint.
fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Click toggles playback. Nothing running is a no-op rather than an error: the chip is a readout until a
/// player appears.
pub fn toggle() {
    mpris::play_pause();
}

/// The wheel over the chip, per `[media] scroll`: adjust the volume (the common case — the chip is where your
/// pointer already is when a track is too loud), skip tracks, or nothing.
pub fn scroll(_dx: f32, dy: f32) {
    let mode = crate::core::shell::config()
        .map(|c| c.media.scroll)
        .unwrap_or_default();
    let up = dy > 0.0;
    match mode {
        MediaScroll::Volume => {
            crate::shared::services::volume::step(if up { 5 } else { -5 });
            crate::modules::osd::show_volume();
        }
        MediaScroll::Track => {
            if up {
                mpris::previous();
            } else {
                mpris::next();
            }
        }
        MediaScroll::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(title: &str, artist: &str) -> Player {
        Player {
            bus: "org.mpris.MediaPlayer2.spotify".to_string(),
            identity: "Spotify".to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            playback: Playback::Playing,
            ..Player::default()
        }
    }

    #[test]
    fn the_glyph_offers_the_action_not_the_state() {
        // A playing track offers "pause"; a paused one offers "play".
        assert_eq!(glyph(&playing("x", "y")), "pause");
        let paused = Player {
            playback: Playback::Paused,
            ..playing("x", "y")
        };
        assert_eq!(glyph(&paused), "play");
        assert_eq!(
            glyph(&Player::default()),
            "music",
            "nothing running shows a neutral glyph, not a control"
        );
    }

    #[test]
    fn nothing_running_yields_no_label_so_the_chip_collapses() {
        assert_eq!(label(&Player::default(), &MediaConfig::default()), "");
    }

    #[test]
    fn long_tracks_are_cut_to_the_configured_width() {
        let config = MediaConfig {
            max_chars: 12,
            ..MediaConfig::default()
        };
        assert_eq!(
            label(&playing("A Love Supreme, Pt. I", "John Coltrane"), &config),
            "John Coltra…"
        );
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        assert_eq!(truncate("mañana señor", 6), "mañan…");
        assert_eq!(truncate("東京の夜", 3), "東京…");
        assert_eq!(truncate("short", 40), "short", "what fits is untouched");
    }
}
