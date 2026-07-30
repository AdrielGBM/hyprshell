//! Which settings live on which page, and how a search finds them.
//!
//! The forms themselves are unchanged — every `*_section` in the parent module still owns one `[toml]` section
//! and still saves it on its own. What this file adds is the *shape* of the application over them: a page is a
//! nav entry and the ordered list of sections it shows, so grouping is a table rather than the order of a
//! forty-item `Vec`.
//!
//! **Search is answered from the schema, not from the widgets.** Every field a form draws is a key on a config
//! struct, and `build.rs` already lifts the doc comment off each one for `hyprshell config schema`. Matching a
//! query against *that* means the search finds `beat_sensitivity` — a key whose label says "Beat sensitivity"
//! and whose explanation says "how far above its recent average the bass has to jump" — without every field
//! having to register itself twice, and without the index going stale when a form gains a row.

use crate::core::schema;

/// A catalogue lookup for a key assembled at runtime.
///
/// `t!` validates its key while it compiles, which it can only do for a literal — and the nav's labels come out
/// of a table. The check is moved rather than lost: `every_label_has_a_translation` asks the catalogue for all
/// of them, and a missing one fails the suite instead of showing a user a raw key.
pub fn label(prefix: &str, name: &str) -> String {
    rsx::i18n::translate(
        &crate::__rsx_i18n::CATALOG,
        &format!("{prefix}.{name}"),
        &[],
    )
}

/// A section's builder: the same signature all forty of them already have.
pub type Build = fn(
    &crate::core::config::Config,
    &std::path::Path,
    crate::shared::theme::NordTheme,
) -> Result<Box<dyn rsx::LayoutItem>, rsx::LayoutError>;

/// One form on a page.
pub struct Section {
    /// Key under `settings.section`, which is also the heading the form draws for itself.
    pub label: &'static str,
    /// The `[toml]` sections this form edits. Drives search, and is what makes a form findable by the name of
    /// a key rather than only by the words on its own label.
    pub keys: &'static [&'static str],
    pub build: Build,
}

/// One nav entry.
pub struct Page {
    /// Key under `settings.page`.
    pub label: &'static str,
    /// Iconify name for the nav row.
    pub icon: &'static str,
    pub sections: &'static [Section],
}

impl Page {
    /// Whether anything on this page answers `query` — what dims a nav entry during a search rather than
    /// letting a user click through to a page with nothing on it.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || label("settings.page", self.label)
                .to_lowercase()
                .contains(&query)
            || self.sections.iter().any(|section| section.matches(&query))
    }
}

impl Section {
    /// `query` is already trimmed and lowercased.
    fn matches(&self, query: &str) -> bool {
        if label("settings.section", self.label)
            .to_lowercase()
            .contains(query)
        {
            return true;
        }
        self.keys
            .iter()
            .any(|key| key.contains(query) || schema::section_mentions(key, query))
    }
}

/// Which forms the page area shows.
///
/// A search deliberately leaves the nav behind and looks everywhere. The alternative — narrowing only the
/// selected page — makes a user who types `beat` and is on the wrong page see nothing at all, and there is no
/// way for them to tell that from "no such setting". Searching is asking the application a question; the nav
/// is for browsing it, and it stays lit so they can see where the answers live.
pub fn visible(selected: usize, query: &str) -> Vec<&'static Section> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return page(selected).sections.iter().collect();
    }
    PAGES
        .iter()
        .flat_map(|page| page.sections.iter())
        .filter(|section| section.matches(&query))
        .collect()
}

macro_rules! section {
    ($label:literal, [$($key:literal),* $(,)?], $build:path) => {
        Section {
            label: $label,
            keys: &[$($key),*],
            build: $build,
        }
    };
}

/// The nav, in the order it is drawn.
///
/// The order is deliberate rather than alphabetical: the first four pages are what a user opens the settings
/// *for* — how it looks, where the bars are, and the two devices whose panels they already know — and the ones
/// they will each visit once come after.
pub const PAGES: &[Page] = &[
    Page {
        label: "appearance",
        icon: "palette",
        sections: &[
            section!("theme", ["theme"], super::theme_section),
            section!("shape", ["shape"], super::shape_section),
            section!("corners", ["corners"], super::corners_section),
            section!("icons", ["icons"], super::icons_section),
            section!("animation", ["animation"], super::animation_section),
        ],
    },
    Page {
        label: "bars",
        icon: "layout-panel-top",
        sections: &[
            section!("bars", ["bars"], super::bars_section),
            section!("panels", ["panels"], super::panels_section),
            section!("popouts", ["popouts"], super::popouts_section),
            section!("osd", ["osd"], super::osd_section),
            section!("clock", ["clock"], super::clock_section),
            section!(
                "active_window",
                ["active_window"],
                super::active_window_section
            ),
            section!("workspaces", ["workspaces"], super::workspaces_section),
            section!(
                "status_icons",
                ["status_icons"],
                super::status_icons_section
            ),
            section!("tray", ["tray"], super::tray_section),
            section!("battery", ["battery"], super::battery_section),
            section!("lock_status", ["lock_status"], super::lock_status_section),
            section!("temperature", ["temperature"], super::temperature_section),
        ],
    },
    Page {
        label: "audio",
        icon: "volume-2",
        sections: &[
            section!("audio", ["audio"], super::audio_section),
            section!("visualiser", ["visualiser"], super::visualiser_section),
            section!("media", ["media"], super::media_section),
            section!("lyrics", ["lyrics"], super::lyrics_section),
        ],
    },
    Page {
        label: "network",
        icon: "wifi",
        sections: &[section!("network", ["network"], super::network_section)],
    },
    Page {
        label: "bluetooth",
        icon: "bluetooth",
        sections: &[section!(
            "bluetooth",
            ["bluetooth"],
            super::bluetooth_section
        )],
    },
    Page {
        label: "applications",
        icon: "layout-grid",
        sections: &[section!("launcher", ["launcher"], super::launcher_section)],
    },
    Page {
        label: "notifications",
        icon: "bell",
        sections: &[
            section!(
                "notifications",
                ["notifications"],
                super::notifications_section
            ),
            section!("toasts", ["toasts"], super::toasts_section),
            section!("sidebar", ["sidebar"], super::sidebar_section),
        ],
    },
    Page {
        label: "lock",
        icon: "lock",
        sections: &[
            section!("lock", ["lock"], super::lock_section),
            section!("idle", ["idle"], super::idle_section),
        ],
    },
    Page {
        label: "wallpaper",
        icon: "image",
        sections: &[
            section!("background", ["background"], super::background_section),
            section!("wallpaper", ["wallpaper"], super::wallpaper_section),
            section!(
                "desktop_clock",
                ["background"],
                super::desktop_clock_section
            ),
            section!(
                "background_visualiser",
                ["background"],
                super::background_visualiser_section
            ),
        ],
    },
    Page {
        label: "language",
        icon: "languages",
        sections: &[
            section!("general", ["general"], super::general_section),
            section!("dashboard", ["dashboard"], super::dashboard_section),
        ],
    },
    Page {
        label: "services",
        icon: "server",
        sections: &[
            section!("weather", ["weather"], super::weather_section),
            section!("gpu", ["gpu"], super::gpu_section),
            section!("brightness", ["brightness"], super::brightness_section),
            section!("paths", ["paths"], super::paths_section),
            section!("screenshot", ["screenshot"], super::screenshot_section),
            section!("recorder", ["recorder"], super::recorder_section),
            section!("utilities", ["utilities"], super::utilities_section),
            section!("keynav", ["keynav"], super::keynav_section),
        ],
    },
    Page {
        label: "about",
        icon: "info",
        sections: &[section!("about", [], super::about_section)],
    },
];

/// The page at `index`, clamped — a stored selection outlives the page list it was made against.
pub fn page(index: usize) -> &'static Page {
    &PAGES[index.min(PAGES.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;

    #[test]
    fn every_config_section_is_reachable_from_some_page() {
        // The failure this catches is silent and permanent: a section added to `Config` with a form written for
        // it, and no nav entry, is a form no user can ever open. Nothing else notices — the shell builds, the
        // schema prints it, and the key simply cannot be edited.
        let defaults = toml::Value::try_from(Config::starter()).expect("serializes");
        let placed: Vec<&str> = PAGES
            .iter()
            .flat_map(|page| page.sections.iter().flat_map(|s| s.keys.iter().copied()))
            .collect();
        let missing: Vec<&str> = defaults
            .as_table()
            .expect("a table")
            .keys()
            // `version` is the schema's own bookkeeping and `modules` is map-valued (K13); neither is a form.
            .filter(|key| !matches!(key.as_str(), "version" | "modules"))
            .filter(|key| !placed.contains(&key.as_str()))
            .map(|key| key.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "config sections with no page: {missing:?}"
        );
    }

    #[test]
    fn a_page_id_and_a_section_label_are_each_used_once() {
        let mut ids: Vec<&str> = PAGES.iter().map(|page| page.label).collect();
        ids.sort_unstable();
        let unique = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), unique, "two pages share a label");

        let mut labels: Vec<&str> = PAGES
            .iter()
            .flat_map(|page| page.sections.iter().map(|s| s.label))
            .collect();
        labels.sort_unstable();
        let unique = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), unique, "the same form is on two pages");
    }

    fn english() {
        crate::shared::services::locale::attach("en".to_string());
    }

    #[test]
    fn a_search_finds_a_form_by_a_key_it_does_not_display() {
        english();
        // The point of indexing the schema: `beat_sensitivity` is a field on the visualiser form, and nothing
        // about the words "Audio visualiser" would ever match it.
        let found = visible(0, "beat_sensitivity");
        assert_eq!(
            found.iter().map(|s| s.label).collect::<Vec<_>>(),
            vec!["visualiser"]
        );
        let audio = PAGES
            .iter()
            .find(|p| p.label == "audio")
            .expect("the audio page");
        assert!(audio.matches("beat_sensitivity"));
        assert!(
            !audio.matches("ddcutil"),
            "a key belonging to another page does not light this one up"
        );
    }

    #[test]
    fn a_search_leaves_the_selected_page_behind() {
        english();
        // A user who types `wifi` while sitting on Appearance has asked a question, not narrowed a page — and
        // an answer that depends on where they happened to be is indistinguishable from "no such setting".
        let from_appearance = visible(0, "ssid");
        let from_services = visible(PAGES.len() - 1, "ssid");
        assert!(!from_appearance.is_empty());
        assert_eq!(
            from_appearance.iter().map(|s| s.label).collect::<Vec<_>>(),
            from_services.iter().map(|s| s.label).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn an_empty_query_shows_the_selected_pages_own_forms() {
        english();
        for (index, page) in PAGES.iter().enumerate() {
            assert!(page.matches(""));
            assert_eq!(visible(index, "").len(), page.sections.len());
            assert_eq!(visible(index, "   ").len(), page.sections.len());
        }
    }

    #[test]
    fn a_query_that_matches_nothing_shows_nothing_rather_than_everything() {
        english();
        assert!(visible(0, "zzzzz-no-such-setting").is_empty());
    }

    #[test]
    fn every_label_has_a_translation() {
        // The check `t!` would have made, moved: these keys are assembled at runtime from the table above, so
        // a page added without its catalogue entry would draw the literal `settings.page.foo` at a user.
        for locale in ["en", "es"] {
            crate::shared::services::locale::attach(locale.to_string());
            for page in PAGES {
                let key = format!("settings.page.{}", page.label);
                assert_ne!(label("settings.page", page.label), key, "{locale}: {key}");
                for section in page.sections {
                    let key = format!("settings.section.{}", section.label);
                    assert_ne!(
                        label("settings.section", section.label),
                        key,
                        "{locale}: {key}"
                    );
                }
            }
        }
        english();
    }

    #[test]
    fn a_selection_past_the_end_still_names_a_page() {
        assert_eq!(page(0).label, PAGES[0].label);
        assert_eq!(page(usize::MAX).label, PAGES[PAGES.len() - 1].label);
    }
}
