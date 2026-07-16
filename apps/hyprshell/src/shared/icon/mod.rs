use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rsx::SvgData;

/// Lucide icons are 24×24 stroke line art (`stroke="currentColor"`) so `svg tint:` can recolor them at render time; embedded here as raw path bodies behind the `icon(name)` seam an eventual CDN cache can plug into unchanged.
const STROKE_WIDTH: f32 = 2.0;

/// (name, inner SVG body). Bodies are the exact Lucide 24×24 path data, sans the outer `<svg>` frame.
const ICONS: &[(&str, &str)] = &[
    (
        "bell",
        r#"<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"#,
    ),
    (
        "battery",
        r#"<rect width="16" height="10" x="2" y="7" rx="2" ry="2"/><line x1="22" x2="22" y1="11" y2="13"/>"#,
    ),
    (
        "battery-charging",
        r#"<path d="M15 7h1a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-2"/><path d="M6 7H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h1"/><path d="m11 7-3 5h4l-3 5"/><line x1="22" x2="22" y1="11" y2="13"/>"#,
    ),
    (
        "volume-2",
        r#"<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/><path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.364 18.364a9 9 0 0 0 0-12.728"/>"#,
    ),
    (
        "volume-1",
        r#"<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/><path d="M16 9a5 5 0 0 1 0 6"/>"#,
    ),
    (
        "volume-x",
        r#"<path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z"/><line x1="22" x2="16" y1="9" y2="15"/><line x1="16" x2="22" y1="9" y2="15"/>"#,
    ),
    (
        "wifi",
        r#"<path d="M12 20h.01"/><path d="M2 8.82a15 15 0 0 1 20 0"/><path d="M5 12.859a10 10 0 0 1 14 0"/><path d="M8.5 16.429a5 5 0 0 1 7 0"/>"#,
    ),
    (
        "sun",
        r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#,
    ),
    (
        "sun-dim",
        r#"<circle cx="12" cy="12" r="4"/><path d="M12 4h.01"/><path d="M20 12h.01"/><path d="M12 20h.01"/><path d="M4 12h.01"/><path d="M17.657 6.343h.01"/><path d="M17.657 17.657h.01"/><path d="M6.343 17.657h.01"/><path d="M6.343 6.343h.01"/>"#,
    ),
];

fn frame(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="{STROKE_WIDTH}" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#
    )
}

thread_local! {
    static CACHE: RefCell<HashMap<&'static str, Arc<SvgData>>> = RefCell::new(HashMap::new());
}

/// Resolves a Lucide icon by name to shared vector data, parsed once per surface thread; an unknown name falls back to a hollow placeholder box so a typo renders visibly rather than panicking.
pub fn icon(name: &str) -> Arc<SvgData> {
    let entry = ICONS.iter().find(|(n, _)| *n == name);
    let key = entry.map(|(n, _)| *n).unwrap_or("__placeholder");
    CACHE.with(|c| {
        if let Some(data) = c.borrow().get(key) {
            return Arc::clone(data);
        }
        let body = entry
            .map(|(_, b)| *b)
            .unwrap_or(r#"<rect width="16" height="16" x="4" y="4" rx="2"/>"#);
        let data = SvgData::from_str(&frame(body))
            .map(Arc::new)
            .unwrap_or_else(|e| panic!("hyprshell: embedded Lucide icon `{name}` failed to parse: {e}"));
        c.borrow_mut().insert(key, Arc::clone(&data));
        data
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_icon_parses() {
        for (name, _) in ICONS {
            // `icon` panics if the embedded SVG fails to parse, so resolving each name is the assertion.
            let _ = icon(name);
        }
    }

    #[test]
    fn unknown_icon_falls_back_without_panicking() {
        let _ = icon("does-not-exist");
    }
}
