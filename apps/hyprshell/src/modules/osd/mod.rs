use std::cell::{Cell, RefCell};
use std::time::Duration;

use rsx::{
    LayoutItem, SurfaceAlign, SurfaceAnchor, SurfacePlacement, SurfaceRole, SurfaceSize,
    SurfaceToken, open_surface, set_theme,
};

use crate::core::config::{Align, Edge};
use crate::shared::theme::NordTheme;

const OSD_W: u32 = 280;
const OSD_H: u32 = 60;

/// Which live state an OSD reflects. A single-slot OSD (§6): one at a time, replaced on the next trigger.
#[derive(Clone, Copy)]
pub enum OsdKind {
    Volume,
    Brightness,
}

fn osd_anchor(edge: Edge) -> SurfaceAnchor {
    match edge {
        Edge::Top => SurfaceAnchor::Top,
        Edge::Bottom => SurfaceAnchor::Bottom,
        Edge::Left => SurfaceAnchor::Left,
        Edge::Right => SurfaceAnchor::Right,
    }
}

fn osd_align(align: Align) -> SurfaceAlign {
    match align {
        Align::Start => SurfaceAlign::Start,
        Align::Center => SurfaceAlign::Center,
        Align::End => SurfaceAlign::End,
    }
}

thread_local! {
    // Set on the OSD thread before its `.rsx` content builds, so `osd.rsx` can read it — the context seam for parameterless `.rsx` modules, like `surface_env`.
    static OSD_KIND: Cell<OsdKind> = const { Cell::new(OsdKind::Volume) };
    // The bar-matching corner radius for the OSD being built; read by `osd.rsx`.
    static OSD_RADIUS: Cell<f32> = const { Cell::new(16.0) };
}

pub fn set_osd_kind(kind: OsdKind) {
    OSD_KIND.with(|k| k.set(kind));
}

/// The kind the OSD being built reflects; read by `osd.rsx`.
pub fn current_osd_kind() -> OsdKind {
    OSD_KIND.with(|k| k.get())
}

/// The corner radius the OSD being built uses (the bar's); read by `osd.rsx`.
pub fn current_osd_radius() -> f32 {
    OSD_RADIUS.with(|r| r.get())
}

/// Builds the OSD's content tree for `kind`/`theme`/`radius` (declared in `osd.rsx`); pub(crate) so the headless visual harness can render it without a real compositor.
pub(crate) fn osd_content(kind: OsdKind, theme: NordTheme, radius: f32) -> Box<dyn LayoutItem> {
    set_theme(theme);
    set_osd_kind(kind);
    OSD_RADIUS.with(|r| r.set(radius));
    crate::osd().expect("osd content build failed")
}

thread_local! {
    // Dropping the token closes any previous OSD, so a new trigger replaces the old (single-slot).
    static OPEN_OSD: RefCell<Option<SurfaceToken>> = const { RefCell::new(None) };
}

/// Shows (or replaces) the single-slot OSD for `kind`; resolves the configured accent here on the bar thread since the OSD surface has no config of its own.
pub fn show(kind: OsdKind) {
    let env = crate::surface_env();
    let theme = env
        .as_ref()
        .map(|e| e.config.resolve_theme())
        .unwrap_or_else(NordTheme::new);
    let osd = env.as_ref().map(|e| e.config.osd).unwrap_or_default();
    let output = env.as_ref().and_then(|e| e.output.clone());
    let radius = env
        .as_ref()
        .map(|e| e.config.panel_radius(osd.edge))
        .unwrap_or(16.0);
    // The shared panel gap. The surface's exclusive_zone=0 already clears the bar via the compositor, so this is only the extra gap beyond it — same rule the drawer and notifications use.
    let inset = env
        .as_ref()
        .map(|e| e.config.panel_gap(osd.edge) as i32)
        .unwrap_or(crate::core::config::DEFAULT_PANEL_GAP as i32);
    let mut placement = SurfacePlacement::new(SurfaceRole::Osd, osd_anchor(osd.edge))
        .align(osd_align(osd.align))
        .input_transparent(true)
        .size(SurfaceSize::Fixed(OSD_W, OSD_H))
        .inset(inset)
        .output(output);
    // 0 ms disables auto-dismiss; the OSD then stays until replaced by the next trigger.
    if osd.timeout_ms > 0 {
        placement = placement.timeout(Duration::from_millis(osd.timeout_ms));
    }
    OPEN_OSD.with(|slot| {
        *slot.borrow_mut() = None; // drop the previous token → closes whatever OSD was up
        let token = open_surface(placement, Box::new(move || osd_content(kind, theme, radius)));
        *slot.borrow_mut() = Some(token);
    });
}

pub fn volume_action() {
    crate::shared::services::volume::toggle_mute();
    show(OsdKind::Volume);
}

pub fn brightness_action() {
    show(OsdKind::Brightness);
}

#[cfg(test)]
mod tests {
    use super::{OsdKind, osd_content};
    use crate::shared::theme::NordTheme;
    use crate::test_support::render_png;
    use rsx::{App, Color, Component, SurfaceRoot, WindowConfig, reset_layout_runtime};

    /// The OSD content wrapped in a full-surface root — the same tree the surface host mounts, without a compositor.
    struct OsdPreviewApp {
        kind: OsdKind,
        accent: Color,
    }

    impl App for OsdPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            Box::new(
                SurfaceRoot::new(osd_content(
                    self.kind,
                    NordTheme {
                        accent: self.accent,
                        ..NordTheme::new()
                    },
                    16.0,
                ))
                .expect("osd surface root"),
            )
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            None
        }
    }

    /// Renders the OSD surface. Gated on its own env var; `HYPRSHELL_VISUAL_OSD_KIND=brightness` for the sun.
    #[test]
    fn visual_osd_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_OSD_OUT") else {
            eprintln!("set RSX_VISUAL_OSD_OUT to render the OSD; skipping");
            return;
        };
        let kind = match std::env::var("HYPRSHELL_VISUAL_OSD_KIND").as_deref() {
            Ok("brightness") => OsdKind::Brightness,
            _ => OsdKind::Volume,
        };
        render_png(
            OsdPreviewApp {
                kind,
                accent: NordTheme::new().accent_by_name("teal"),
            },
            280,
            60,
            &out,
        );
    }
}
