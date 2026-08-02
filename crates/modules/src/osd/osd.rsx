[logic]
use crate::osd::{OsdKind, current_osd_kind, current_osd_radius};
use ::config::theme::NordTheme;
use ::services::{brightness, volume};
use ::ui::glyph;

const TRACK_W: f32 = 172.0;
const TRACK_H: f32 = 6.0;

fn osd_tint(dimmed: bool) -> Color {
    let t = use_theme::<NordTheme>();
    if dimmed { t.muted } else { t.text }
}

// A single-shot snapshot — the OSD is transient, and every trigger (click, scroll, key) replaces it with a
// freshly built one. It reads the shared services' cached value rather than the system: `volume::read()` forks
// `wpctl`, which has no business running while a surface is being laid out.
let (glyph, frac, dimmed) = match current_osd_kind() {
    OsdKind::Volume => {
        let v = volume::current().unwrap_or(volume::Volume {
            level: 0,
            muted: false,
        });
        (
            glyph::volume(v),
            v.level.clamp(0, 100) as f32 / 100.0,
            v.muted,
        )
    }
    OsdKind::Brightness => {
        let level = brightness::osd_level().unwrap_or(0).clamp(0, 100);
        (glyph::brightness(), level as f32 / 100.0, false)
    }
    OsdKind::Microphone => {
        let v = volume::current_mic().unwrap_or(volume::Volume {
            level: 0,
            muted: true,
        });
        (
            glyph::microphone(v),
            v.level.clamp(0, 100) as f32 / 100.0,
            v.muted,
        )
    }
};
let fill_w = (frac.clamp(0.0, 1.0) * TRACK_W).max(0.0);
let rad = current_osd_radius();
let icon_sz = use_theme::<NordTheme>().icon_size;
let icon = ui::icon::icon_view(move || glyph.to_string(), move || osd_tint(dimmed), icon_sz)?;

[view]
box direction:row align:center justify:center gap:14 pad_x:18 pad_y:14 width:100% height:100% fill:surface radius:rad
    widget "icon"
    box direction:row align:center width:TRACK_W height:TRACK_H fill:muted radius:3
        box width:fill_w height:TRACK_H fill:accent radius:3
