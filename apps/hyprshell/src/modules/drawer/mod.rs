use telar::motion::Animated;
use telar::{
    LayoutError, LayoutItem, LayoutStyle, RectStyle, SizeDimension, StyledContainer, SurfaceAlign,
    SurfaceAnchor, SurfacePlacement, SurfaceToken, open_surface, set_theme,
};

use crate::core::config::{AnimationConfig, DrawerConfig, Edge, Zone};
use crate::shared::module::SurfaceEnv;

fn anchor_for(edge: Edge) -> SurfaceAnchor {
    match edge {
        Edge::Top => SurfaceAnchor::Top,
        Edge::Bottom => SurfaceAnchor::Bottom,
        Edge::Left => SurfaceAnchor::Left,
        Edge::Right => SurfaceAnchor::Right,
    }
}

/// A drawer aligns to the same end of the bar as the module that opened it (§4); a module in no zone centres.
fn align_for(zone: Option<Zone>) -> SurfaceAlign {
    match zone {
        Some(Zone::Start) => SurfaceAlign::Start,
        Some(Zone::End) => SurfaceAlign::End,
        _ => SurfaceAlign::Center,
    }
}

/// The raw panel content for a module, shared by the drawer and floating-window presentations; unknown modules fall back to the clock panel with a warning.
pub(crate) fn module_panel(module: &str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match module {
        "clock" => crate::clock_panel(),
        "dashboard" => crate::modules::dashboard::dashboard_panel(),
        "battery" => crate::battery_panel(),
        "bluetooth" => crate::modules::bluetooth::bluetooth_panel(),
        "network" => crate::modules::network::network_panel(),
        "mixer" => crate::modules::mixer::mixer_panel(),
        "notifications" => crate::modules::notifications::bell_panel(),
        "notes" => crate::notes_panel(),
        "settings" => crate::modules::settings::settings_panel(),
        "utilities" => crate::modules::utilities::utilities_panel(),
        "windowinfo" => crate::modules::windowinfo::window_panel(),
        "session" | "logo" => crate::modules::session::session_panel(),
        other => {
            tracing::warn!("no panel registered for module '{other}'");
            crate::clock_panel()
        }
    }
}

/// Whether `module`'s panel needs the keyboard — because it hosts editable text, or because it is navigable
/// with the arrow keys.
///
/// Asking for it costs more than an unused capability. A layer surface granted keyboard focus takes it from the
/// focused window, and the compositor re-focuses that window when the panel closes; a layout that follows focus
/// — a scrolling one, say — moves the viewport on the way back. A panel that only displays readings has no use
/// for the keyboard and should never provoke that.
///
/// `session` is here for the second reason: its tiles are a list, and a menu whose most destructive entries are
/// two presses away is exactly the one a user wants to reach without moving their hand to the mouse.
///
/// Kept beside [`module_panel`] so the two lists cannot drift: a panel that gains a text field, or keyboard
/// navigation, must appear here.
pub(crate) fn panel_wants_keyboard(module: &str) -> bool {
    matches!(module, "notes" | "settings" | "session")
}

/// The per-panel-surface context (which module, its config, and the bar-matching corner radius), provided into
/// the drawer/float surface's scope so `drawer_panel.rsx` reads it via `inject` — scoped to the surface, not a
/// global thread-local. Drawer and float are separate surfaces, so each provides its own.
#[derive(Clone)]
struct DrawerCtx {
    module: String,
    config: DrawerConfig,
    radius: f32,
}

pub fn set_drawer_ctx(module: String, drawer: DrawerConfig, radius: f32) {
    let _ = telar::provide(DrawerCtx {
        module,
        config: drawer,
        radius,
    });
}

/// Slides and fades `content` in from the bar edge it hangs off, over `[animation] panel_duration_ms` and the
/// configured easing.
///
/// The *enter* half of the panel transition. Exit is not here and cannot be: closing a surface flags it and
/// the driver tears it down on its next loop turn, so by the time an out-animation would run there is nothing
/// left to draw it on — see `platform-layershell`'s `SurfaceHandle::close`.
///
/// Constructed away from its goal and retargeted at once, never at the goal: an `Animated` born settled never
/// registers with the ticker, so nothing would schedule the frames that carry it in — the same trap the
/// workspace indicator hit.
pub fn enter_transition(
    content: Box<dyn LayoutItem>,
    edge: Edge,
    animation: &AnimationConfig,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tween = animation.panel_tween();
    if tween.duration.is_zero() {
        return Ok(content);
    }
    // The distance is in the panel's own travel, not the screen's: a drawer arrives from the edge it hangs off.
    let travel = 24.0;
    let progress = Animated::new(1.0f32, tween);
    progress.retarget(0.0);
    let slide = progress.clone();
    let fade = progress;
    let (dx, dy) = match edge {
        Edge::Top => (0.0, -travel),
        Edge::Bottom => (0.0, travel),
        Edge::Left => (-travel, 0.0),
        Edge::Right => (travel, 0.0),
    };
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new().width(SizeDimension::Percent(1.0)),
            |_| RectStyle::default(),
            vec![content],
        )?
        .with_transform(move |_| {
            let at = slide.get();
            (at != 0.0).then_some([1.0, 0.0, 0.0, 1.0, dx * at, dy * at])
        })
        .with_opacity(move || 1.0 - fade.get()),
    ))
}

/// The background a panel paints, at `[panels] opacity`. Read from the surface's own config so a per-monitor
/// override reaches it, falling back to the solid theme token outside a surface (a test, a preview render).
pub fn panel_fill() -> telar::Color {
    match crate::shared::module::surface_env() {
        Some(env) => env.config.panel_fill(),
        None => telar::use_theme::<crate::shared::theme::NordTheme>().surface,
    }
}

/// The module whose panel the drawer being built shows; read by `drawer_panel.rsx`.
pub fn current_drawer_module() -> String {
    telar::try_inject::<DrawerCtx>()
        .map(|ctx| ctx.module)
        .unwrap_or_default()
}

/// The drawer size (width / max height) for the drawer being built; read by `drawer_panel.rsx`.
pub fn current_drawer_config() -> DrawerConfig {
    telar::try_inject::<DrawerCtx>()
        .map(|ctx| ctx.config)
        .unwrap_or_default()
}

/// The bar-matching corner radius of the panel currently being built (drawer or float); read by `drawer_panel.rsx` and by the notification history it hosts, so content rounds its corners like the bar regardless of which panel presents it.
pub fn content_radius() -> f32 {
    telar::try_inject::<DrawerCtx>()
        .map(|ctx| ctx.radius)
        .unwrap_or(0.0)
}

/// Provides a panel context carrying just the content radius (module/config defaulted) — used by a float
/// presenting the same panel content as a drawer, so its cards carry the bar radius too.
pub fn set_content_radius(radius: f32) {
    let _ = telar::provide(DrawerCtx {
        module: String::new(),
        config: DrawerConfig::default(),
        radius,
    });
}

/// Opens `module_id`'s drawer as a scrimmed surface floating off the bar edge on the bar's own monitor, aligned to the same end of the bar as the module; the distance off the bar is the shared [`Config::panel_margin`](crate::Config), so every panel keeps the same config-controlled gap. The surface/scrim/slide-in come from the rsx surface host, the panel from `drawer_panel.rsx`. Toggle/close is the caller's job ([`crate::toggle_panel`]) via the returned token.
pub(crate) fn open_drawer(env: &SurfaceEnv, module_id: &str) -> SurfaceToken {
    let theme = env.config.resolve_theme();
    let placement = SurfacePlacement::drawer(anchor_for(env.edge))
        .align(align_for(env.config.zone_of(env.edge, module_id)))
        .margin(env.config.panel_margin(env.edge))
        .keyboard(panel_wants_keyboard(module_id))
        .output(env.output.clone());
    let module = module_id.to_string();
    let drawer = env.config.panels.drawer;
    let radius = env.config.panel_radius(env.edge);
    let animation = env.config.animation.clone();
    let edge = env.edge;
    open_surface(
        placement,
        Box::new(move || {
            set_theme(theme);
            set_drawer_ctx(module.clone(), drawer, radius);
            let panel = crate::drawer_panel().expect("drawer panel build failed");
            enter_transition(panel, edge, &animation).expect("drawer transition build failed")
        }),
    )
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    use crate::shared::theme::NordTheme;

    fn content() -> Box<dyn LayoutItem> {
        telar::box_item(telar::Container::new(LayoutStyle::new(), vec![]).unwrap())
    }

    #[test]
    fn a_panel_enters_on_every_edge_and_skips_the_wrapper_when_animation_is_off() {
        for edge in Edge::ALL {
            telar::reset_layout_runtime();
            telar::set_theme(NordTheme::new());
            assert!(
                enter_transition(content(), edge, &AnimationConfig::default()).is_ok(),
                "the enter transition builds on {edge:?}"
            );
        }

        // Switched off, the panel is handed back untouched rather than wrapped in a box that animates nothing — an extra container around every panel is a layout change nobody asked for.
        telar::reset_layout_runtime();
        telar::set_theme(NordTheme::new());
        let off = AnimationConfig {
            enabled: false,
            ..AnimationConfig::default()
        };
        assert!(off.panel_tween().duration.is_zero());
        assert!(enter_transition(content(), Edge::Top, &off).is_ok());
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::panel_wants_keyboard;

    #[test]
    fn only_panels_that_read_keys_ask_for_the_keyboard() {
        assert!(panel_wants_keyboard("notes"), "notes are edited in place");
        assert!(panel_wants_keyboard("settings"), "settings has text fields");
        assert!(
            panel_wants_keyboard("session"),
            "the session tiles are arrow-navigable, which is the other reason to want the keyboard"
        );
        for display_only in [
            "clock",
            "dashboard",
            "battery",
            "bluetooth",
            "network",
            "notifications",
            "logo",
        ] {
            assert!(
                !panel_wants_keyboard(display_only),
                "'{display_only}' only shows readings; taking keyboard focus from the window would make the \
                 compositor re-focus it on close, moving the viewport under a focus-following layout"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::set_drawer_ctx;
    use crate::core::config::DrawerConfig;
    use crate::shared::theme::NordTheme;
    use crate::test_support::render_png;
    use telar::{
        App, Color, Component, SurfaceAnchor, SurfacePlacement, SurfaceScaffold, WindowConfig,
        reset_layout_runtime, set_theme,
    };

    /// The real drawer panel (`drawer_panel.rsx`) inside a scrimmed scaffold, the same tree the surface host mounts.
    struct DrawerPreviewApp;

    impl App for DrawerPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(NordTheme::new());
            set_drawer_ctx("clock".to_string(), DrawerConfig::default(), 14.0);
            let panel = crate::drawer_panel().expect("drawer panel build failed");
            let placement = SurfacePlacement::drawer(SurfaceAnchor::Top).inset(48);
            Box::new(SurfaceScaffold::new(&placement, panel, None).expect("scaffold build failed"))
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

    /// Renders a drawer (§4): scrimmed scaffold + fixed-width scrollable panel. Gated on its own env var.
    #[test]
    fn visual_drawer_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_DRAWER_OUT") else {
            eprintln!("set TELAR_VISUAL_DRAWER_OUT to render the drawer; skipping");
            return;
        };
        render_png(DrawerPreviewApp, 520, 420, &out);
    }
}
