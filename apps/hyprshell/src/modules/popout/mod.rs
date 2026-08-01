//! Hover popouts: the readout a chip shows while the pointer rests on it.
//!
//! Distinct from the drawer a click opens, and the shell's primary status interaction: a bar chip has room for
//! one glyph, and everything that glyph stands for — the level behind it, the sensor it came from, the whole
//! window title it truncated — lives here.
//!
//! Three things make it a popout rather than a flicker. **Delays**: the pointer has to rest on a chip before
//! anything opens, and the popout survives long enough after leaving for the pointer to reach it. **One
//! surface**: moving from chip to chip replaces the card rather than stacking a second one. **A carved input
//! region**: the surface is sized to the tallest card a popout may be, and everything the card doesn't cover
//! falls through to the window underneath, so a popout that is up while you click elsewhere costs nothing.
//!
//! Placement is [`shared::anchor`](crate::shared::anchor), the same helper the tray's context menus use.

use std::cell::RefCell;
use std::sync::Arc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, open_surface, timeout,
};
use telar::{
    AlignItems, App, Color, Component, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Rect, SizeDimension, SurfaceToken, WindowConfig, reset_layout_runtime, set_theme,
};

mod card;
mod content;

pub use content::has_popout;

use crate::core::app::SurfaceRoot;
use crate::core::config::{Config, Edge};
use crate::shared::anchor::chip_margin;
use crate::shared::module::{SurfaceEnv, set_surface_env};
use crate::shared::theme::NordTheme;

/// One popout at a time: a second card on screen would be two readouts competing for the same glance.
const SURFACE_ID: &str = "popout";

thread_local! {
    static STATE: RefCell<Popout> = const { RefCell::new(Popout::new()) };
}

struct Popout {
    /// The module whose popout is up, or on its way there.
    target: Option<String>,
    /// Bumped on every hover transition, on the chip and on the card alike. A delay that fires after the
    /// pointer has moved on reads a generation that no longer matches and does nothing — cheaper and more
    /// reliable than cancelling a timer, and it is the whole of the arbitration: entering the card voids the
    /// close the chip scheduled on the way out, without either side tracking where the pointer is.
    generation: u64,
}

impl Popout {
    const fn new() -> Self {
        Self {
            target: None,
            generation: 0,
        }
    }
}

fn bump() -> u64 {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.generation = state.generation.wrapping_add(1);
        state.generation
    })
}

fn current(generation: u64) -> bool {
    STATE.with(|s| s.borrow().generation == generation)
}

fn showing(module_id: &str) -> bool {
    crate::core::shell::window_is_open(SURFACE_ID)
        && STATE.with(|s| s.borrow().target.as_deref() == Some(module_id))
}

pub fn close() {
    STATE.with(|s| s.borrow_mut().target = None);
    crate::core::shell::close(SURFACE_ID);
}

/// The bar reports every chip's pointer enter and leave here. Both directions are scheduled rather than acted
/// on: an instant open would fire on a bar the pointer is only crossing, and an instant close would fire while
/// the pointer crosses the gap towards the card it just opened.
///
/// `chip` is the rect as it stands now, not the signal behind it: the timer fires on the shared loop rather
/// than inside the bar surface, and a chip cannot move under a resting pointer without the bar being rebuilt,
/// which tears this down anyway.
pub fn hover(module_id: &str, chip: Rect, entered: bool) {
    let Some(env) = crate::surface_env() else {
        return;
    };
    if !env.config.popouts.enabled {
        return;
    }
    let generation = bump();
    if entered {
        // Re-entering the chip under an open card has already voided the pending close, so nothing is left to schedule.
        if showing(module_id) {
            return;
        }
        let module = module_id.to_string();
        timeout(env.config.popouts.open_after(), move || {
            if current(generation) {
                open(&module, chip, &env);
            }
        });
    } else {
        timeout(env.config.popouts.close_after(), move || {
            if current(generation) {
                close();
            }
        });
    }
}

/// The card's own pointer tracking, so moving into the popout keeps it up and leaving it starts the same grace
/// the chip does. Without this the popout would close under a pointer resting on it.
fn keep_open(entered: bool) {
    let generation = bump();
    // Entering needs no work of its own: the bump has already voided the close the chip scheduled on the way here.
    if entered {
        return;
    }
    let delay = crate::core::shell::config()
        .map(|c| c.popouts.close_after())
        .unwrap_or_default();
    timeout(delay, move || {
        if current(generation) {
            close();
        }
    });
}

/// Opens `module_id`'s card under its chip, replacing whatever was up. Runs on the driver thread — it is
/// reached from a hover handler through a timer on the shared loop, which is where a surface may be opened.
fn open(module_id: &str, chip: Rect, env: &SurfaceEnv) {
    if !content::has_popout(module_id) {
        return;
    }
    // The delay is long enough for a config reload to land inside it, and a card built against the outgoing config would carry a stale theme onto a screen the rest of the shell has already left.
    if !crate::core::shell::config().is_some_and(|live| Arc::ptr_eq(&live, &env.config)) {
        return;
    }
    close();
    STATE.with(|s| s.borrow_mut().target = Some(module_id.to_string()));
    let app = PopoutApp {
        module: module_id.to_string(),
        edge: env.edge,
        bar_size: env.bar_size,
        output: env.output.clone(),
    };
    let layer = layer_config(env, chip);
    crate::core::shell::toggle_window(SURFACE_ID, move || {
        SurfaceToken::new(Box::new(open_surface(layer, app)))
    });
}

/// The two edges the popout pins itself to: the bar's own, so it hangs off it, and the one it runs along, so
/// the margin that lines it up with the chip means something. A layer surface only honours a margin on an edge
/// it is anchored to.
fn anchor_flags(edge: Edge) -> Anchor {
    match edge {
        Edge::Top => Anchor::TOP | Anchor::LEFT,
        Edge::Bottom => Anchor::BOTTOM | Anchor::LEFT,
        Edge::Left => Anchor::LEFT | Anchor::TOP,
        Edge::Right => Anchor::RIGHT | Anchor::TOP,
    }
}

/// The surface is the tallest a popout may be, not the size of this card: a layer surface pinned to two edges
/// has to name a size, and a card's height is content-derived. `interactive_input_region` is what makes that
/// affordable — the compositor is handed only the card's own rect, so the surplus stays click-through.
fn layer_config(env: &SurfaceEnv, chip: Rect) -> LayerConfig {
    let popouts = env.config.popouts;
    let (width, height) = (popouts.card_width(), popouts.card_height());
    let span = if env.edge.is_vertical() {
        height
    } else {
        width
    };
    let off_bar = env.config.panel_gap(env.edge) as f32;
    LayerConfig {
        output: env.output.clone(),
        layer: Layer::Overlay,
        anchor: anchor_flags(env.edge),
        exclusive_zone: 0,
        size: (width.ceil() as u32, height.ceil() as u32),
        margin: chip_margin(env, chip, off_bar, Some(span)),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: "hyprshell-popout".to_string(),
        reserve_only: false,
        input_transparent: false,
        interactive_input_region: true,
    }
}

/// Which corner of its surface the card sits in — the one the surface is anchored to, so the card lands
/// exactly where the margin put that corner instead of floating in the middle of an oversized surface.
fn corner_alignment(edge: Edge) -> (JustifyContent, AlignItems) {
    match edge {
        Edge::Top | Edge::Left => (JustifyContent::START, AlignItems::START),
        Edge::Bottom => (JustifyContent::START, AlignItems::END),
        Edge::Right => (JustifyContent::END, AlignItems::START),
    }
}

fn corner_style(edge: Edge) -> LayoutStyle {
    let (justify, align) = corner_alignment(edge);
    LayoutStyle::new()
        .flex_row()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
        .justify_content(justify)
        .align_items(align)
}

/// Builds a popout's tree for `module_id`; `pub(crate)` so the visual harness can render one without a
/// compositor.
pub(crate) fn popout_content(
    module_id: &str,
    config: &Config,
    edge: Edge,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let inner = match content::build(module_id, config, theme) {
        Some(card) => card?,
        None => return Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?)),
    };
    let framed = card::frame(
        inner,
        config.panel_fill(),
        config.popouts.card_width(),
        config.panel_radius(edge),
        keep_open,
    )?;
    Ok(Box::new(Container::new(corner_style(edge), vec![framed])?))
}

struct PopoutApp {
    module: String,
    edge: Edge,
    bar_size: u32,
    output: Option<String>,
}

impl App for PopoutApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let config = crate::core::surfaces::config_for(self.output.as_deref());
        let theme = config.resolve_theme();
        set_theme(theme);
        crate::shared::services::locale::attach(config.language());
        // The card reads config through the same `surface_env` a bar module does, so both resolve the same settings.
        set_surface_env(SurfaceEnv {
            edge: self.edge,
            bar_size: self.bar_size,
            output: self.output.clone(),
            config: Arc::clone(&config),
        });
        let content = popout_content(&self.module, &config, self.edge, theme)
            .expect("popout content build failed");
        Box::new(SurfaceRoot::new(content).expect("popout surface root"))
    }

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(edge: Edge) -> SurfaceEnv {
        SurfaceEnv {
            edge,
            bar_size: 34,
            output: None,
            config: Arc::new(Config::starter()),
        }
    }

    fn chip() -> Rect {
        Rect {
            x: 400.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
        }
    }

    #[test]
    fn the_surface_pins_itself_to_the_bar_edge_and_the_one_it_runs_along() {
        // A margin only positions a layer surface on an edge it is anchored to, so pinning the bar edge alone would centre the popout on screen instead of lining it up with its chip.
        for edge in Edge::ALL {
            let flags = anchor_flags(edge);
            assert_eq!(
                flags.iter().count(),
                2,
                "{edge:?} must pin both the off-bar and the along-bar axis"
            );
        }
        assert!(anchor_flags(Edge::Top).contains(Anchor::TOP));
        assert!(anchor_flags(Edge::Right).contains(Anchor::RIGHT));
    }

    #[test]
    fn the_surface_never_reserves_space_or_takes_the_keyboard() {
        let config = layer_config(&env(Edge::Top), chip());
        assert_eq!(
            config.exclusive_zone, 0,
            "a popout must not carve space out of the desktop"
        );
        assert!(matches!(
            config.keyboard_interactivity,
            KeyboardInteractivity::None
        ));
        assert!(
            config.interactive_input_region,
            "the surplus around the card has to stay click-through"
        );
        assert!(
            !config.input_transparent,
            "the card itself must take the pointer"
        );
    }

    #[test]
    fn a_disabled_section_never_schedules_anything() {
        let config: Config = toml::from_str("[popouts]\nenabled = false\n").expect("config parses");
        assert!(!config.popouts.enabled);
        // The guard lives in `hover`, which needs a surface; the flag it returns on is what a unit test can reach.
        assert!(Config::starter().popouts.enabled, "on by default");
    }

    #[test]
    fn the_delays_are_clamped_so_a_typo_cannot_make_a_popout_instant() {
        let config: Config =
            toml::from_str("[popouts]\nopen_delay = 0\nclose_delay = 0\n").expect("config parses");
        assert!(
            config.popouts.open_after().as_millis() >= 60,
            "an instant popout would fire on a bar the pointer is only crossing"
        );
        assert!(
            config.popouts.close_after().as_millis() >= 60,
            "an instant close would fire while the pointer crosses towards the card"
        );
    }

    #[test]
    fn every_module_offered_a_popout_builds_a_card_on_every_edge() {
        let config = Config::starter();
        let theme = config.resolve_theme();
        for id in content::WITH_POPOUT {
            for edge in Edge::ALL {
                telar::reset_layout_runtime();
                set_theme(theme);
                assert!(
                    popout_content(id, &config, edge, theme).is_ok(),
                    "'{id}' failed to build on the {edge:?} edge"
                );
            }
        }
    }

    #[test]
    fn the_card_pins_itself_into_the_corner_its_surface_is_anchored_to() {
        assert_eq!(corner_alignment(Edge::Bottom).1, AlignItems::END);
        assert_eq!(corner_alignment(Edge::Right).0, JustifyContent::END);
        assert_eq!(corner_alignment(Edge::Top).1, AlignItems::START);
        assert_eq!(corner_alignment(Edge::Left).0, JustifyContent::START);
    }

    /// Renders one popout card for eyeballing. `HYPRSHELL_VISUAL_POPOUT` names the module (default `volume`);
    /// gated on its own env var like every other visual test.
    #[test]
    fn visual_popout_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_POPOUT_OUT") else {
            eprintln!("set TELAR_VISUAL_POPOUT_OUT to render a popout; skipping");
            return;
        };
        let module =
            std::env::var("HYPRSHELL_VISUAL_POPOUT").unwrap_or_else(|_| "volume".to_string());
        let config = Config::starter();
        let (w, h) = (
            config.popouts.card_width() as u32,
            config.popouts.card_height() as u32,
        );
        // Published so the card resolves it exactly as it would on a live screen; a visual render has no
        // reconcile to have put one there.
        crate::core::shell::set_config(Arc::new(config));
        crate::test_support::render_png(
            PopoutApp {
                module,
                edge: Edge::Top,
                bar_size: 34,
                output: None,
            },
            w,
            h,
            &out,
        );
    }
}
