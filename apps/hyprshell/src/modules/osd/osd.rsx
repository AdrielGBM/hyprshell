[logic]
use crate::modules::osd::{OsdKind, current_osd_kind, current_osd_radius};
use crate::shared::theme::NordTheme;
use crate::shared::services::{brightness, volume};

const TRACK_W: f32 = 172.0;
const TRACK_H: f32 = 6.0;

fn vol_glyph(muted: bool, level: i32) -> &'static str {
    if muted || level == 0 {
        "volume-x"
    } else if level < 50 {
        "volume-1"
    } else {
        "volume-2"
    }
}

fn osd_tint(dimmed: bool) -> Color {
    let t = use_theme::<NordTheme>();
    if dimmed { t.muted } else { t.text }
}

// A single-shot snapshot of the triggering service, read once — the OSD is transient (shown, then replaced).
let (glyph, frac, dimmed) = match current_osd_kind() {
    OsdKind::Volume => {
        let v = volume::read().unwrap_or(volume::Volume {
            level: 0,
            muted: false,
        });
        let level = v.level.clamp(0, 100);
        (vol_glyph(v.muted, level), level as f32 / 100.0, v.muted)
    }
    OsdKind::Brightness => {
        let level = brightness::read().unwrap_or(0).clamp(0, 100);
        ("sun", level as f32 / 100.0, false)
    }
};
let fill_w = (frac.clamp(0.0, 1.0) * TRACK_W).max(0.0);
let rad = current_osd_radius();
let icon_sz = use_theme::<NordTheme>().icon_size;
let icon = crate::icon_view(move || glyph.to_string(), move || osd_tint(dimmed), icon_sz)?;

[view]
box direction:row align:center justify:center gap:14 pad_x:18 pad_y:14 width:100% height:100% fill:surface radius:rad
    widget "icon"
    box direction:row align:center width:TRACK_W height:TRACK_H fill:muted radius:3
        box width:fill_w height:TRACK_H fill:accent radius:3
