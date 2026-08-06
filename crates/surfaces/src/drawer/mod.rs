use telar::SurfaceToken;

use config::SurfaceEnv;
use config::{Align, DrawerConfig, Zone};
use ui::panel::PanelSurface;
use ui::placement::Placement;

/// A drawer aligns to the same end of the bar as the chip that opened it (§4).
///
/// The chip that was pressed is the answer, and the config is only the fallback for a panel opened without one —
/// IPC, a keybind. Looking the module up instead would be wrong twice over: an id placed in more than one zone
/// resolves to whichever the search reaches first, and a `[corners]` module is in no zone at all despite being
/// laid out at a very definite end of its bar.
fn align_for(origin: Option<Zone>) -> Align {
    match origin {
        Some(Zone::Start) => Align::Start,
        Some(Zone::End) => Align::End,
        _ => Align::Center,
    }
}

pub use ui::panel::{content_radius, panel_fill, panel_transition};
pub use ui::panels::{build as module_panel, wants_keyboard as panel_wants_keyboard};

/// Which module a drawer shows and how big it may be, set on the drawer surface's own scope so
/// `drawer_panel.rsx` reads it via `inject` — scoped to the surface, not a global thread-local.
///
/// The corner radius is *not* here: it is the bar's, so it comes from the surface's own environment
/// ([`content_radius`]) rather than from a copy every panel surface would have to remember to pass on.
#[derive(Clone)]
struct DrawerCtx {
    module: String,
    config: DrawerConfig,
}

fn ctx() -> Option<DrawerCtx> {
    util::state::context::<DrawerCtx>()
}

pub fn set_drawer_ctx(module: String, drawer: DrawerConfig) {
    util::state::set_context(DrawerCtx {
        module,
        config: drawer,
    });
}

/// The module whose panel the drawer being built shows; read by `drawer_panel.rsx`.
pub fn current_drawer_module() -> String {
    ctx().map(|ctx| ctx.module).unwrap_or_default()
}

/// The drawer size (width / max height) for the drawer being built; read by `drawer_panel.rsx`.
pub fn current_drawer_config() -> DrawerConfig {
    ctx().map(|ctx| ctx.config).unwrap_or_default()
}

/// Opens `module_id`'s drawer as a surface floating off the bar edge on the bar's own monitor, aligned to `origin` — the zone of the chip that was pressed, or the module's own zone when nothing pressed it; the distance off the bar is the shared [`Config::panel_margin`](config::Config), so every panel keeps the same config-controlled gap. The surface/dismiss/slide-in come from the rsx surface host, the panel from `drawer_panel.rsx`. Toggle/close is the caller's job ([`crate::panel::toggle_panel`]) via the returned token.
pub(crate) fn open_drawer(env: &SurfaceEnv, module_id: &str, origin: Option<Zone>) -> SurfaceToken {
    let placement = Placement::sheet(env.edge, panel_wants_keyboard(module_id))
        .align(align_for(
            origin.or_else(|| env.config.zone_of(env.edge, module_id)),
        ))
        .margin(env.config.panel_margin(env.edge))
        .output(env.output.clone());
    let module = module_id.to_string();
    // What is captured is what the drawer *is* — which module it shows. Everything the config decides is
    // resolved per build by the panel surface, so a rebuilt drawer is a drawer that followed the edit rather
    // than one still drawing the config it opened under.
    PanelSurface::new(placement, move |env| {
        set_drawer_ctx(module.clone(), env.config.panels.drawer);
        crate::drawer_panel().expect("drawer panel build failed")
    })
    .animated()
    .open()
}

#[cfg(test)]
mod alignment_tests {
    use super::align_for;
    use config::{Align, Config, Edge, Zone};

    /// The chip that was pressed decides, and the config is only the fallback — because as a lookup it answers
    /// the wrong question twice over.
    #[test]
    fn a_drawer_follows_the_chip_that_opened_it_not_the_config() {
        let cfg: Config = toml::from_str(
            "[shape]\nframe=true\n\
             [bars.top]\nstart=[\"notifications\"]\ncenter=[\"clock\"]\nend=[\"notifications\"]\n\
             [corners]\ntop_left=\"clock\"\n",
        )
        .unwrap();

        // A module placed in more than one zone: the lookup can only name the first, so the chip at the other
        // end of the bar would open its drawer at the wrong end.
        assert_eq!(cfg.zone_of(Edge::Top, "notifications"), Some(Zone::Start));
        assert_eq!(align_for(Some(Zone::End)), Align::End);

        // A `[corners]` module is in no zone at all, though the bar lays it out at a very definite end of
        // itself — routed there as the leading entry of the owning bar's start zone.
        assert_eq!(cfg.zone_of(Edge::Top, "clock"), Some(Zone::Center));
        assert_eq!(cfg.corner_modules_for(Edge::Top).0, Some("clock"));
        assert_eq!(align_for(Some(Zone::Start)), Align::Start);

        // With no chip — IPC, a keybind — the config is all there is, and centring is the honest answer to a
        // module it cannot place.
        assert_eq!(align_for(None), Align::Center);
    }
}
