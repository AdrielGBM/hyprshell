use std::cell::RefCell;
use std::time::Duration;

use rsx::{
    AlignItems, App, Color, Component, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ObjectFit, RectStyle, SizeDimension, StyledContainer, Svg, WindowConfig, box_item,
    reset_layout_runtime, set_theme,
};

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, SurfaceHandle, open_surface, request_close,
    timeout,
};

use crate::core::app::SurfaceRoot;
use crate::core::theme::NordTheme;

const OSD_MS: u64 = 1200;
const OSD_W: u32 = 280;
const OSD_H: u32 = 60;
/// Gap from the top edge of the screen.
const OSD_MARGIN: i32 = 16;
const TRACK_W: f32 = 172.0;
const TRACK_H: f32 = 6.0;

/// Which live state an OSD reflects. A single-slot OSD (§6): one at a time, replaced on the next trigger.
#[derive(Clone, Copy)]
pub enum OsdKind {
    Volume,
    Brightness,
}

fn vol_glyph(muted: bool, level: i32) -> &'static str {
    if muted || level == 0 {
        "volume-x"
    } else if level < 50 {
        "volume-1"
    } else {
        "volume-2"
    }
}

/// The icon, 0–1 fill fraction, and whether to dim the icon, read live for `kind`.
fn osd_state(kind: OsdKind) -> (&'static str, f32, bool) {
    match kind {
        OsdKind::Volume => {
            let v = crate::shared::volume::read().unwrap_or(crate::shared::volume::Volume {
                level: 0,
                muted: false,
            });
            let level = v.level.clamp(0, 100);
            (vol_glyph(v.muted, level), level as f32 / 100.0, v.muted)
        }
        OsdKind::Brightness => {
            let level = crate::shared::brightness::read().unwrap_or(0).clamp(0, 100);
            ("sun", level as f32 / 100.0, false)
        }
    }
}

/// Builds an OSD's content: the current icon beside a level bar, read live from the matching service.
fn build_osd(kind: OsdKind, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (glyph, frac, dimmed) = osd_state(kind);
    let frac = frac.clamp(0.0, 1.0);
    let icon_data = crate::icon(glyph);
    let tint = if dimmed { theme.muted } else { theme.text };
    let svg = Svg::new(
        LayoutStyle::new().width(24.0).height(24.0),
        move || icon_data.clone(),
        move || Some(tint),
        || ObjectFit::Contain,
    )?;
    let fill = StyledContainer::new(
        LayoutStyle::new().width((frac * TRACK_W).max(0.0)).height(TRACK_H),
        move |_| RectStyle::filled(theme.accent, TRACK_H / 2.0),
        vec![],
    )?;
    let track = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .width(TRACK_W)
            .height(TRACK_H),
        move |_| RectStyle::filled(theme.muted, TRACK_H / 2.0),
        vec![box_item(fill)],
    )?;
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .gap(14.0)
            .padding_horizontal(18.0)
            .padding_vertical(14.0)
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(theme.surface, 16.0),
        vec![box_item(svg), box_item(track)],
    )?;
    Ok(box_item(panel))
}

/// A transient OSD surface: builds its content, then schedules its own dismissal. Runs on its own
/// surface/thread with an isolated reactive world, like a drawer.
pub struct OsdApp {
    pub kind: OsdKind,
    /// The configured accent (resolved on the bar thread at `show` time, since this surface has no config).
    pub accent: Color,
}

impl App for OsdApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = NordTheme {
            accent: self.accent,
            ..NordTheme::new()
        };
        set_theme(theme);
        // Dismiss itself after the display window; the timer lives on this surface's own loop.
        timeout(Duration::from_millis(OSD_MS), request_close);
        let content = build_osd(self.kind, theme).expect("osd content build failed");
        Box::new(SurfaceRoot::new(content).expect("osd layout failed"))
    }

    fn window_config(&self) -> Option<WindowConfig> {
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }

    fn clear_color(&self) -> Option<Color> {
        // Transparent: only the rounded panel is drawn; the rest of the small surface stays clear.
        None
    }
}

/// A small top-centered overlay surface for the OSD; click-through (input-transparent) since it only
/// reflects state and dismisses on a timer.
fn osd_spec(output: Option<String>) -> LayerConfig {
    LayerConfig {
        output,
        layer: Layer::Overlay,
        anchor: Anchor::TOP,
        exclusive_zone: 0,
        size: (OSD_W, OSD_H),
        margin: (OSD_MARGIN, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-osd"),
        reserve_only: false,
        input_transparent: true,
    }
}

thread_local! {
    // The OSD currently shown from THIS surface's thread. Dropping the handle closes any previous OSD, so a
    // new trigger replaces the old (single-slot).
    static OPEN_OSD: RefCell<Option<SurfaceHandle>> = const { RefCell::new(None) };
}

/// Shows (or replaces) the single-slot OSD for `kind`. Resolves the configured accent here on the bar
/// thread (via `surface_env`), since the OSD surface has no config of its own.
pub fn show(kind: OsdKind) {
    let accent = crate::surface_env()
        .map(|e| NordTheme::new().accent_by_name(&e.config.theme.accent))
        .unwrap_or_else(|| NordTheme::new().accent);
    OPEN_OSD.with(|slot| {
        *slot.borrow_mut() = None; // drop the previous handle → closes whatever OSD was up
        let handle = open_surface(osd_spec(None), OsdApp { kind, accent });
        *slot.borrow_mut() = Some(handle);
    });
}

/// The volume module's click action: toggle mute, then pop the volume OSD.
pub fn volume_action() {
    crate::shared::volume::toggle_mute();
    show(OsdKind::Volume);
}

/// The brightness module's click action: pop the brightness OSD (a peek at the current level).
pub fn brightness_action() {
    show(OsdKind::Brightness);
}
