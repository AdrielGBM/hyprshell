use telar::motion::Animated;
use telar::{
    LayoutError, LayoutItem, LayoutStyle, RectStyle, StyledContainer, SurfaceToken, open_surface,
    set_theme, surface_content,
};

use config::SurfaceEnv;
use config::{Align, AnimationConfig, DrawerConfig, Edge, Zone};
use ui::placement::Placement;
use util::state::kept;

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

pub use ui::panels::{build as module_panel, wants_keyboard as panel_wants_keyboard};

/// The per-panel-surface context (which module, its config, and the bar-matching corner radius), set on the
/// drawer/float surface's own scope so `drawer_panel.rsx` reads it via `inject` — scoped to the surface, not a
/// global thread-local. Drawer and float are separate surfaces, so each sets its own.
#[derive(Clone)]
struct DrawerCtx {
    module: String,
    config: DrawerConfig,
    radius: f32,
}

fn ctx() -> Option<DrawerCtx> {
    util::state::context::<DrawerCtx>()
}

pub fn set_drawer_ctx(module: String, drawer: DrawerConfig, radius: f32) {
    util::state::set_context(DrawerCtx {
        module,
        config: drawer,
        radius,
    });
}

/// Slides and fades `content` in from the bar edge it hangs off, and back out to it when the surface is asked
/// to close, over `[animation] panel_duration_ms` and the configured easing.
///
/// One progress carries both halves — 1 is off the bar edge and transparent, 0 is settled — so the exit is the
/// entrance reversed rather than a second animation that has to be kept in step with the first.
///
/// The exit only reaches the screen because the driver holds a closing surface mapped for as long as
/// [`on_close`](platform_wayland::on_close) says to. Without that it would animate a surface that was torn
/// down on the loop's next turn, which is exactly what this could not do before.
///
/// Constructed away from its goal and retargeted at once, never at the goal: an `Animated` born settled never
/// registers with the ticker, so nothing would schedule the frames that carry it in — the same trap the
/// workspace indicator hit.
///
/// Kept across rebuilds ([`kept`]), because arriving is something the panel did once: a fresh `Animated` would
/// start at 1 again and slide the panel back in, so every config edit would look like the drawer reopening.
/// The one this finds on a rebuild has already settled at 0, which is exactly where the panel is.
pub fn panel_transition(
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
    let progress = kept("drawer.transition", || {
        let progress = Animated::new(1.0f32, tween);
        progress.retarget(0.0);
        progress
    });
    platform_wayland::on_close(tween.duration, {
        let progress = progress.clone();
        move || progress.retarget(1.0)
    });
    let slide = progress.clone();
    let fade = progress;
    let (dx, dy) = match edge {
        Edge::Top => (0.0, -travel),
        Edge::Bottom => (0.0, travel),
        Edge::Left => (-travel, 0.0),
        Edge::Right => (travel, 0.0),
    };
    // Shrink-wrapped, and that is load-bearing rather than tidy: this box is the node the scaffold measures to decide what counts as "outside the panel", and it is the child the scaffold's `align_items` positions. A `width: 100%` here made both wrong at once — every press in the panel's whole horizontal band read as a press *on* it, and the panel sat at the start of a full-width box instead of at the end of the bar its module lives on.
    Ok(Box::new(
        StyledContainer::new(LayoutStyle::new(), |_| RectStyle::default(), vec![content])?
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
    match config::surface_env() {
        Some(env) => env.config.panel_fill(),
        None => telar::use_theme::<config::theme::NordTheme>().surface,
    }
}

/// The module whose panel the drawer being built shows; read by `drawer_panel.rsx`.
pub fn current_drawer_module() -> String {
    ctx().map(|ctx| ctx.module).unwrap_or_default()
}

/// The drawer size (width / max height) for the drawer being built; read by `drawer_panel.rsx`.
pub fn current_drawer_config() -> DrawerConfig {
    ctx().map(|ctx| ctx.config).unwrap_or_default()
}

/// The bar-matching corner radius of the panel currently being built (drawer or float); read by `drawer_panel.rsx` and by the notification history it hosts, so content rounds its corners like the bar regardless of which panel presents it.
pub fn content_radius() -> f32 {
    ctx().map(|ctx| ctx.radius).unwrap_or(0.0)
}

/// Provides a panel context carrying just the content radius (module/config defaulted) — used by a float
/// presenting the same panel content as a drawer, so its cards carry the bar radius too.
pub fn set_content_radius(radius: f32) {
    set_drawer_ctx(String::new(), DrawerConfig::default(), radius);
}

/// Opens `module_id`'s drawer as a scrimmed surface floating off the bar edge on the bar's own monitor, aligned to `origin` — the zone of the chip that was pressed, or the module's own zone when nothing pressed it; the distance off the bar is the shared [`Config::panel_margin`](config::Config), so every panel keeps the same config-controlled gap. The surface/scrim/slide-in come from the rsx surface host, the panel from `drawer_panel.rsx`. Toggle/close is the caller's job ([`crate::panel::toggle_panel`]) via the returned token.
pub(crate) fn open_drawer(env: &SurfaceEnv, module_id: &str, origin: Option<Zone>) -> SurfaceToken {
    let placement = Placement::sheet(env.edge, panel_wants_keyboard(module_id))
        .align(align_for(
            origin.or_else(|| env.config.zone_of(env.edge, module_id)),
        ))
        .margin(env.config.panel_margin(env.edge))
        .output(env.output.clone())
        .hosted_placement();
    let module = module_id.to_string();
    let edge = env.edge;
    let output = env.output.clone();
    open_surface(
        placement,
        // What is captured is what the drawer *is* — which module, which edge, which screen. Everything the
        // config decides is resolved here, on every build, so a rebuilt drawer is a drawer that followed the
        // edit rather than one still drawing the config it opened under.
        surface_content(move || {
            let config = config::config_for(output.as_deref());
            set_theme(config.resolve_theme());
            set_drawer_ctx(
                module.clone(),
                config.panels.drawer,
                config.panel_radius(edge),
            );
            let panel = crate::drawer_panel().expect("drawer panel build failed");
            panel_transition(panel, edge, &config.animation)
                .expect("drawer transition build failed")
        }),
    )
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    use config::theme::NordTheme;

    fn content() -> Box<dyn LayoutItem> {
        telar::box_item(telar::Container::new(LayoutStyle::new(), vec![]).unwrap())
    }

    #[test]
    fn a_panel_enters_on_every_edge_and_skips_the_wrapper_when_animation_is_off() {
        for edge in Edge::ALL {
            telar::reset_layout_runtime();
            telar::set_theme(NordTheme::new());
            assert!(
                panel_transition(content(), edge, &AnimationConfig::default()).is_ok(),
                "the panel transition builds on {edge:?}"
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
        assert!(panel_transition(content(), Edge::Top, &off).is_ok());
    }

    /// The transition box is the node the scaffold measures, so its width is the drawer's dismiss area.
    ///
    /// It was `width: 100%`, which made two things wrong at once and neither of them visible: a press anywhere
    /// in the panel's horizontal band read as a press *on* the panel, so the only way to dismiss a drawer was to
    /// click above or below it — and the panel was positioned at the start of a full-width box rather than by
    /// the scaffold's own alignment, which is what puts it at the end of the bar its module sits on.
    ///
    /// Building proves none of that; the wrapper builds happily either way. This lays out the real tree the
    /// surface host mounts and presses next to the panel.
    #[test]
    fn a_press_beside_the_panel_dismisses_the_drawer() {
        use std::cell::Cell;
        use std::rc::Rc;
        use telar::{
            AvailableSpace, Component, Event, KeyboardMode, PointerButton, PointerSource,
            SurfaceScaffold, compute_layout,
        };

        const PANEL_WIDTH: f32 = 320.0;
        const SURFACE: f32 = 1280.0;

        for align in [
            telar::SurfaceAlign::Start,
            telar::SurfaceAlign::Center,
            telar::SurfaceAlign::End,
        ] {
            telar::reset_layout_runtime();
            telar::set_theme(NordTheme::new());

            let panel = telar::box_item(
                telar::Container::new(LayoutStyle::new().width(PANEL_WIDTH).height(200.0), vec![])
                    .unwrap(),
            );
            let wrapped = panel_transition(panel, Edge::Top, &AnimationConfig::default()).unwrap();

            let dismissed = Rc::new(Cell::new(0u32));
            let sink = Rc::clone(&dismissed);
            let placement = Placement::sheet(Edge::Top, KeyboardMode::None)
                .margin((8, 8, 8, 8))
                .hosted_placement()
                .align(align);
            let mut scaffold = SurfaceScaffold::new(
                &placement,
                wrapped,
                Some(Rc::new(move || sink.set(sink.get() + 1))),
            )
            .unwrap();
            compute_layout(
                scaffold.layout_node(),
                AvailableSpace::Definite(SURFACE),
                AvailableSpace::Definite(720.0),
            )
            .unwrap();
            scaffold.on_event(&Event::WindowResized {
                width: SURFACE as u32,
                height: 720,
            });

            let press = |x: f64| Event::PointerPressed {
                x,
                y: 100.0,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            };
            // One of the two far edges is always scrim whichever end the panel is aligned to, so pressing both and requiring one to dismiss holds for every alignment without restating the layout.
            let before = dismissed.get();
            scaffold.on_event(&press(16.0));
            scaffold.on_event(&press(SURFACE as f64 - 16.0));
            assert!(
                dismissed.get() > before,
                "{align:?}: a press beside the panel must dismiss the drawer — a full-width wrapper makes the \
                 whole row read as the panel, leaving no way out but clicking past its top or bottom edge"
            );
        }
    }
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
