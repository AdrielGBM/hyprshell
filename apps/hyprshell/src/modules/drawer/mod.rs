use std::cell::RefCell;

use rsx::{
    LayoutError, LayoutItem, SurfaceAlign, SurfaceAnchor, SurfacePlacement, SurfaceToken,
    open_surface, set_theme,
};

use crate::core::config::{DrawerConfig, Edge, Zone};
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
        "notifications" => crate::modules::notifications::bell_panel(),
        other => {
            tracing::warn!("no panel registered for module '{other}'");
            crate::clock_panel()
        }
    }
}

#[derive(Clone)]
struct DrawerCtx {
    module: String,
    config: DrawerConfig,
    radius: f32,
}

thread_local! {
    // Set on the drawer thread before its `.rsx` content builds, so `drawer_panel.rsx` can read it — the context seam for parameterless `.rsx` modules, like `surface_env`.
    static DRAWER_CTX: RefCell<DrawerCtx> = RefCell::new(DrawerCtx {
        module: String::new(),
        config: DrawerConfig::default(),
        radius: 0.0,
    });
}

pub fn set_drawer_ctx(module: String, drawer: DrawerConfig, radius: f32) {
    DRAWER_CTX.with(|c| {
        *c.borrow_mut() = DrawerCtx {
            module,
            config: drawer,
            radius,
        }
    });
}

/// The module whose panel the drawer being built shows; read by `drawer_panel.rsx`.
pub fn current_drawer_module() -> String {
    DRAWER_CTX.with(|c| c.borrow().module.clone())
}

/// The drawer size (width / max height) for the drawer being built; read by `drawer_panel.rsx`.
pub fn current_drawer_config() -> DrawerConfig {
    DRAWER_CTX.with(|c| c.borrow().config)
}

/// The bar-matching corner radius of the panel currently being built (drawer or float); read by `drawer_panel.rsx` and by the notification history it hosts, so content rounds its corners like the bar regardless of which panel presents it.
pub fn content_radius() -> f32 {
    DRAWER_CTX.with(|c| c.borrow().radius)
}

/// Sets just the content radius on this surface thread, leaving module/config — used by a float presenting the same panel content as a drawer, so its cards carry the bar radius too.
pub fn set_content_radius(radius: f32) {
    DRAWER_CTX.with(|c| c.borrow_mut().radius = radius);
}

/// Opens `module_id`'s drawer as a scrimmed surface floating off the bar edge on the bar's own monitor, aligned to the same end of the bar as the module; the distance off the bar is the shared [`Config::panel_margin`](crate::Config), so every panel keeps the same config-controlled gap. The surface/scrim/slide-in come from the rsx surface host, the panel from `drawer_panel.rsx`. Toggle/close is the caller's job ([`crate::toggle_panel`]) via the returned token.
pub(crate) fn open_drawer(env: &SurfaceEnv, module_id: &str) -> SurfaceToken {
    let theme = env.config.resolve_theme();
    let placement = SurfacePlacement::drawer(anchor_for(env.edge))
        .align(align_for(env.config.zone_of(env.edge, module_id)))
        .margin(env.config.panel_margin(env.edge))
        .output(env.output.clone());
    let module = module_id.to_string();
    let drawer = env.config.panels.drawer;
    let radius = env.config.panel_radius(env.edge);
    open_surface(
        placement,
        Box::new(move || {
            set_theme(theme);
            set_drawer_ctx(module, drawer, radius);
            crate::drawer_panel().expect("drawer panel build failed")
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::set_drawer_ctx;
    use crate::core::config::DrawerConfig;
    use crate::shared::theme::NordTheme;
    use crate::test_support::render_png;
    use rsx::{
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
        let Ok(out) = std::env::var("RSX_VISUAL_DRAWER_OUT") else {
            eprintln!("set RSX_VISUAL_DRAWER_OUT to render the drawer; skipping");
            return;
        };
        render_png(DrawerPreviewApp, 520, 420, &out);
    }
}
