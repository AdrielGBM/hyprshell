use std::cell::RefCell;

use rsx::{
    LayoutError, LayoutItem, SurfaceAlign, SurfaceAnchor, SurfacePlacement, SurfaceToken,
    open_surface, set_theme,
};

use crate::core::config::{DrawerConfig, Edge, Zone};
use crate::shared::module::surface_env;
use crate::shared::theme::NordTheme;

const PANEL_GAP: i32 = 8;

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
        other => {
            tracing::warn!("no panel registered for module '{other}'");
            crate::clock_panel()
        }
    }
}

thread_local! {
    // Set on the drawer thread before its `.rsx` content builds, so `drawer_panel.rsx` can read it — the context seam for parameterless `.rsx` modules, like `surface_env`.
    static DRAWER_CTX: RefCell<(String, DrawerConfig)> =
        RefCell::new((String::new(), DrawerConfig::default()));
}

pub fn set_drawer_ctx(module: String, drawer: DrawerConfig) {
    DRAWER_CTX.with(|c| *c.borrow_mut() = (module, drawer));
}

/// The module whose panel the drawer being built shows; read by `drawer_panel.rsx`.
pub fn current_drawer_module() -> String {
    DRAWER_CTX.with(|c| c.borrow().0.clone())
}

/// The drawer size (width / max height) for the drawer being built; read by `drawer_panel.rsx`.
pub fn current_drawer_config() -> DrawerConfig {
    DRAWER_CTX.with(|c| c.borrow().1)
}

thread_local! {
    // The drawer currently open from THIS bar surface, if any. Dropping the token closes the drawer's surface.
    static OPEN_DRAWER: RefCell<Option<(String, SurfaceToken)>> = const { RefCell::new(None) };
}

/// Toggles the drawer for `module_id`: opens as a scrimmed surface hugging the bar edge, closes if already open (including one dismissed via its scrim, so the next click reopens it); surface/scrim/slide-in come from the rsx surface host, the panel from `drawer_panel.rsx`.
pub fn toggle_drawer(module_id: &str) {
    let Some(env) = surface_env() else { return };
    OPEN_DRAWER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let already_open = slot
            .as_ref()
            .is_some_and(|(id, token)| id == module_id && !token.is_closing());
        *slot = None; // drops the previous token → closes whatever drawer was open
        if !already_open {
            let accent = NordTheme::new().accent_by_name(&env.config.theme.accent);
            let placement = SurfacePlacement::drawer(anchor_for(env.edge))
                .align(align_for(env.config.zone_of(env.edge, module_id)))
                .inset(env.bar_size as i32 + PANEL_GAP);
            let module = module_id.to_string();
            let drawer = env.config.drawer;
            let token = open_surface(
                placement,
                Box::new(move || {
                    let theme = NordTheme {
                        accent,
                        ..NordTheme::new()
                    };
                    set_theme(theme);
                    set_drawer_ctx(module, drawer);
                    crate::drawer_panel().expect("drawer panel build failed")
                }),
            );
            *slot = Some((module_id.to_string(), token));
        }
    });
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
            set_drawer_ctx("clock".to_string(), DrawerConfig::default());
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
