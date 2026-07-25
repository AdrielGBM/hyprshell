use std::cell::RefCell;
use std::sync::Arc;
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
    Microphone,
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

/// The per-OSD-surface context (which state it reflects, and the bar-matching corner radius), provided into the
/// OSD surface's scope so `osd.rsx` reads it via `inject` — scoped to the surface, not a global thread-local.
#[derive(Clone, Copy)]
struct OsdCtx {
    kind: OsdKind,
    radius: f32,
}

/// The kind the OSD being built reflects; read by `osd.rsx`.
pub fn current_osd_kind() -> OsdKind {
    rsx::try_inject::<OsdCtx>()
        .map(|ctx| ctx.kind)
        .unwrap_or(OsdKind::Volume)
}

/// The corner radius the OSD being built uses (the bar's); read by `osd.rsx`.
pub fn current_osd_radius() -> f32 {
    rsx::try_inject::<OsdCtx>().map(|ctx| ctx.radius).unwrap_or(16.0)
}

/// Builds the OSD's content tree for `kind`/`theme`/`radius` (declared in `osd.rsx`); pub(crate) so the headless visual harness can render it without a real compositor.
pub(crate) fn osd_content(kind: OsdKind, theme: NordTheme, radius: f32) -> Box<dyn LayoutItem> {
    set_theme(theme);
    let _ = rsx::provide(OsdCtx { kind, radius });
    crate::osd().expect("osd content build failed")
}

thread_local! {
    // Dropping the token closes any previous OSD, so a new trigger replaces the old (single-slot).
    static OPEN_OSD: RefCell<Option<SurfaceToken>> = const { RefCell::new(None) };
}

/// Shows (or replaces) the single-slot OSD for `kind`; resolves the configured accent here on the bar thread since the OSD surface has no config of its own.
///
/// The bar surface in scope supplies the monitor when a chip triggered this. Triggered from outside a surface —
/// an IPC call or a keybind — there is none, so it falls back to the running config and the focused monitor
/// rather than to bare defaults, which would flash an unthemed OSD on the wrong screen.
pub fn show(kind: OsdKind) {
    let env = crate::surface_env();
    let config = env
        .as_ref()
        .map(|e| Arc::clone(&e.config))
        .or_else(crate::core::shell::config);
    let theme = config
        .as_ref()
        .map(|c| c.resolve_theme())
        .unwrap_or_else(NordTheme::new);
    let osd = config.as_ref().map(|c| c.osd).unwrap_or_default();
    let output = match env.as_ref() {
        Some(env) => env.output.clone(),
        None => crate::core::shell::focused_output(),
    };
    let radius = config
        .as_ref()
        .map(|c| c.panel_radius(osd.edge))
        .unwrap_or(16.0);
    // The shared panel gap. The surface's exclusive_zone=0 already clears the bar via the compositor, so this is only the extra gap beyond it — same rule the drawer and notifications use.
    let inset = config
        .as_ref()
        .map(|c| c.panel_gap(osd.edge) as i32)
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

/// One wheel notch, in the percentage points a level module moves per scroll.
const STEP: i32 = 5;

/// Percentage points to move for a scroll delta: a step per notch, in the scrolled direction. `dy` is positive
/// scrolling up (the platform already flips Wayland's axis), which is the direction that raises the level.
fn scroll_step(dy: f32) -> i32 {
    if dy > 0.0 { STEP } else { -STEP }
}

/// Flashes the volume OSD without changing anything — for callers that already moved the level (a keybind
/// routed through IPC) and only want the feedback.
pub fn show_volume() {
    show(OsdKind::Volume);
}

pub fn show_brightness() {
    show(OsdKind::Brightness);
}

pub fn show_microphone() {
    show(OsdKind::Microphone);
}

pub fn mic_action() {
    crate::shared::services::volume::toggle_mic_mute();
    show(OsdKind::Microphone);
}

pub fn mic_scroll(_dx: f32, dy: f32) {
    crate::shared::services::volume::step_mic(scroll_step(dy));
    show(OsdKind::Microphone);
}

pub fn volume_action() {
    crate::shared::services::volume::toggle_mute();
    show(OsdKind::Volume);
}

pub fn volume_scroll(_dx: f32, dy: f32) {
    crate::shared::services::volume::step(scroll_step(dy));
    show(OsdKind::Volume);
}

pub fn brightness_action() {
    show(OsdKind::Brightness);
}

pub fn brightness_scroll(_dx: f32, dy: f32) {
    crate::shared::services::brightness::step(scroll_step(dy));
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
