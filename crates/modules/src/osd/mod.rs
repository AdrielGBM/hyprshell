use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use telar::{LayoutItem, SurfaceToken, set_theme};

use config::theme::NordTheme;
use ui::panel::PanelSurface;
use ui::placement::Placement;

const OSD_W: u32 = 280;
const OSD_H: u32 = 60;

/// Which live state an OSD reflects. A single-slot OSD (§6): one at a time, replaced on the next trigger.
#[derive(Clone, Copy)]
pub enum OsdKind {
    Volume,
    Brightness,
    Microphone,
}

/// Which state the OSD being built reflects, provided into its surface's scope so `osd.rsx` reads it via
/// `inject` — scoped to the surface, not a global thread-local.
#[derive(Clone, Copy)]
struct OsdCtx {
    kind: OsdKind,
}

/// The kind the OSD being built reflects; read by `osd.rsx`.
pub fn current_osd_kind() -> OsdKind {
    util::state::context::<OsdCtx>()
        .map(|ctx| ctx.kind)
        .unwrap_or(OsdKind::Volume)
}

/// The corner radius the OSD being built uses — the bar's, like every other panel's; read by `osd.rsx`.
pub fn current_osd_radius() -> f32 {
    ui::panel::content_radius()
}

/// Builds the OSD's content tree for `kind`/`theme` (declared in `osd.rsx`), putting both in scope for it
/// first — which is why the surface calls this rather than the component directly.
pub(crate) fn osd_content(kind: OsdKind, theme: NordTheme) -> Box<dyn LayoutItem> {
    set_theme(theme);
    util::state::set_context(OsdCtx { kind });
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
    let env = ui::module::surface_env();
    let config = env
        .as_ref()
        .map(|e| Arc::clone(&e.config))
        .or_else(config::config);
    let osd = config.as_ref().map(|c| c.osd).unwrap_or_default();
    let output = match env.as_ref() {
        Some(env) => env.output.clone(),
        None => surfaces::shell::focused_output(),
    };
    // The shared panel gap. The surface's exclusive_zone=0 already clears the bar via the compositor, so this is only the extra gap beyond it — same rule the drawer and notifications use.
    let inset = config
        .as_ref()
        .map(|c| c.panel_gap(osd.edge) as i32)
        .unwrap_or(config::DEFAULT_PANEL_GAP as i32);
    // A zero timeout disables the auto-dismiss; the OSD then stays until the next trigger replaces it.
    let placement = Placement::flash(osd.edge, osd.align, Duration::from_millis(osd.timeout_ms))
        .size(OSD_W, OSD_H)
        .inset(inset)
        .output(output);
    OPEN_OSD.with(|slot| {
        *slot.borrow_mut() = None; // drop the previous token → closes whatever OSD was up
        // The screen it landed on is fixed for its short life; its look is not, so it is resolved per build —
        // an OSD still up when the theme changes is rebuilt in the new one rather than left behind in the old.
        let token = PanelSurface::new(placement, move |env| {
            osd_content(kind, env.config.resolve_theme())
        })
        .open();
        *slot.borrow_mut() = Some(token);
    });
}

/// Percentage points to move for a scroll delta: one configured `increment` per notch, in the scrolled
/// direction. `dy` is positive scrolling up (the platform already flips Wayland's axis), which is the direction
/// that raises the level.
fn scroll_step(increment: i32, dy: f32) -> i32 {
    if dy > 0.0 { increment } else { -increment }
}

/// The wheel step for the audio chips, from `[audio] increment`.
fn audio_step(dy: f32) -> i32 {
    scroll_step(services::volume::settings().step(), dy)
}

/// The wheel step for the backlight chip, from `[brightness] increment`.
fn brightness_step(dy: f32) -> i32 {
    scroll_step(services::brightness::settings().step(), dy)
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
    services::volume::toggle_mic_mute();
    show(OsdKind::Microphone);
}

pub fn mic_scroll(_dx: f32, dy: f32) {
    services::volume::step_mic(audio_step(dy));
    show(OsdKind::Microphone);
}

pub fn volume_action() {
    services::volume::toggle_mute();
    show(OsdKind::Volume);
}

pub fn volume_scroll(_dx: f32, dy: f32) {
    services::volume::step(audio_step(dy));
    show(OsdKind::Volume);
}

pub fn brightness_action() {
    show(OsdKind::Brightness);
}

pub fn brightness_scroll(_dx: f32, dy: f32) {
    services::brightness::step(brightness_step(dy));
    show(OsdKind::Brightness);
}
