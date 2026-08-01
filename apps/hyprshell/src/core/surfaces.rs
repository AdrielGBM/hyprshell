//! What the config says should be on screen, and keeping the screen in step with it.
//!
//! The shell puts up four kinds of surface on its own: a wallpaper, a bar per edge, the invisible strip each
//! bar reserves, and the frame ring. Which of them exist, how big they are and where they sit are all answers
//! to the config, and the config is a file the user edits while the shell is running — so this module holds
//! both halves: [`plan`], which reads the config as a set of surfaces, and [`Surfaces`], which owns the live
//! ones and brings them in line with a new plan.
//!
//! **A reload reuses, it does not replace.** Every surface here has a [`Key`] that survives an edit — what it
//! is, and which screen it is on — so a config change reaches the surface that is already up: its layer-shell
//! state is renegotiated in place and its content is built again on the same surface. A surface is created
//! only when the config asks for one that is not there (a bar given its first module, a monitor plugged in)
//! and destroyed only when the config stops asking for it. Nothing blinks in between, which is what makes
//! editing the config with the settings window open bearable.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, OutputDescriptor, SurfaceHandle,
};

use crate::core::app::BarApp;
use crate::core::config::{Config, Edge};
use crate::modules::frame::FrameApp;
use crate::modules::wallpaper::WallpaperApp;

/// The config a live surface builds against.
///
/// A shared cell rather than a plain value because the surface outlives any one config: after an edit it is
/// the same surface, and the only thing that changed is what it should look like. The reconcile writes,
/// the surface's next build reads.
#[derive(Clone)]
pub struct LiveConfig(Rc<RefCell<Arc<Config>>>);

impl LiveConfig {
    pub fn new(config: Arc<Config>) -> Self {
        Self(Rc::new(RefCell::new(config)))
    }

    pub fn get(&self) -> Arc<Config> {
        Arc::clone(&self.0.borrow())
    }

    fn set(&self, config: Arc<Config>) {
        *self.0.borrow_mut() = config;
    }
}

impl From<Arc<Config>> for LiveConfig {
    fn from(config: Arc<Config>) -> Self {
        Self::new(config)
    }
}

/// What a shell-owned surface is for. Its whole identity beyond the screen it is on — two surfaces with the
/// same role on the same output are the same surface, before and after any edit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Role {
    Wallpaper,
    Bar(Edge),
    /// The invisible strip that carves an edge's space out of the screen. A separate surface from the bar it
    /// reserves for, so the two are reconciled independently — an auto-hiding bar keeps its ring's strip while
    /// giving up its own.
    Reserve(Edge),
    Frame,
}

impl Role {
    /// Whether this surface draws anything. A reservation strip is an exclusive zone and a transparent buffer,
    /// so there is nothing in it for a config change to rebuild.
    fn draws(self) -> bool {
        !matches!(self, Role::Reserve(_))
    }
}

/// A surface's identity across a reload.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Key {
    output: Option<String>,
    role: Role,
}

/// One surface the config calls for: what it is, how the compositor should place it, and the config its
/// content resolves against — the global one merged with this monitor's override.
struct Planned {
    key: Key,
    layer: LayerConfig,
    config: Arc<Config>,
}

/// What a reconcile does to the surfaces that stay.
///
/// A config edit changes what a bar draws, so every surface that survives it builds again. A monitor being
/// plugged in does not: rebuilding the other screens' bars for it would throw away their state to redraw
/// exactly what was already there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Content {
    Rebuild,
    Keep,
}

/// The live surfaces, keyed so a reload finds the one it is about.
#[derive(Default)]
pub struct Surfaces {
    live: Vec<(Key, Live)>,
}

struct Live {
    handle: SurfaceHandle,
    /// The layer-shell state the compositor currently has for this surface, to diff the next plan against.
    layer: LayerConfig,
    config: LiveConfig,
}

impl Surfaces {
    /// Brings the screen in line with `config`: renegotiates and rebuilds what is already up, opens what the
    /// config newly asks for, and drops what it no longer does.
    ///
    /// Closing runs first so an edge that lost its bar gives its exclusive zone back before the surfaces that
    /// stay are measured against it — otherwise every one of them would be configured once against the old
    /// zone and again a frame later.
    pub fn reconcile(
        &mut self,
        path: &Path,
        config: &Arc<Config>,
        outputs: &[OutputDescriptor],
        content: Content,
    ) {
        let plan = plan(path, config, outputs);
        let wanted: HashSet<&Key> = plan.iter().map(|planned| &planned.key).collect();
        self.live.retain(|(key, _)| wanted.contains(key));

        // Layer-shell stacks surfaces of one layer in the order they were created, so a surface created under
        // one that already exists would come out on top of it — a wallpaper switched on while the frame ring
        // is up would cover the ring. Whatever the plan puts *after* a newly created surface in the same layer
        // on the same screen is therefore created again, in its place in the order.
        let mut restacking: HashSet<(Option<String>, Layer)> = HashSet::new();
        for planned in plan {
            let stack = (planned.key.output.clone(), planned.layer.layer);
            match self.index_of(&planned.key) {
                Some(index) if !restacking.contains(&stack) => {
                    self.live[index].1.adopt(planned, content)
                }
                Some(index) => {
                    self.live.remove(index);
                    self.open(planned);
                }
                None => {
                    restacking.insert(stack);
                    self.open(planned);
                }
            }
        }
    }

    /// How many surfaces are up. `hyprshell` logs it at startup and the tests read it.
    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    fn index_of(&self, key: &Key) -> Option<usize> {
        self.live.iter().position(|(candidate, _)| candidate == key)
    }

    #[cfg(test)]
    fn roles(&self) -> Vec<Role> {
        self.live.iter().map(|(key, _)| key.role).collect()
    }

    /// The cell a live surface reads its config from, which is the identity a test compares: the same cell
    /// before and after an edit means the same surface.
    #[cfg(test)]
    fn config_of(&self, role: Role) -> Option<LiveConfig> {
        self.live
            .iter()
            .find(|(key, _)| key.role == role)
            .map(|(_, live)| live.config.clone())
    }

    fn open(&mut self, planned: Planned) {
        self.live.push((planned.key.clone(), Live::open(planned)));
    }
}

impl Live {
    fn open(planned: Planned) -> Self {
        let Planned { key, layer, config } = planned;
        let config = LiveConfig::new(config);
        let output = key.output.clone();
        let handle = match key.role {
            Role::Reserve(_) => platform_layershell::open_reservation(layer.clone()),
            Role::Bar(edge) => platform_layershell::open_surface(
                layer.clone(),
                BarApp {
                    config: config.clone(),
                    edge,
                    output,
                },
            ),
            Role::Wallpaper => platform_layershell::open_surface(
                layer.clone(),
                WallpaperApp {
                    config: config.clone(),
                    output,
                },
            ),
            Role::Frame => platform_layershell::open_surface(
                layer.clone(),
                FrameApp {
                    config: config.clone(),
                },
            ),
        };
        Self {
            handle,
            layer,
            config,
        }
    }

    /// Takes on what the plan now says this surface should be: the config its content resolves against, the
    /// layer-shell state the compositor holds for it, and — for a config change — a rebuild of its content.
    fn adopt(&mut self, planned: Planned, content: Content) {
        self.config.set(planned.config);
        let change = self.layer.delta(&planned.layer);
        if !change.is_empty() {
            self.handle.update(change);
        }
        self.layer = planned.layer;
        if content == Content::Rebuild && planned.key.role.draws() {
            self.handle.rebuild();
        }
    }
}

thread_local! {
    /// What each screen is running, as the last reconcile resolved it. Written here rather than derived at
    /// each call site because resolving it means reading that monitor's override off disk.
    static RUNNING: RefCell<HashMap<String, Arc<Config>>> = RefCell::new(HashMap::new());
}

/// The config a surface on `output` resolves against: the same per-monitor merge the chrome on that screen is
/// running, so a panel opened from a bar follows the same overrides — and the same edits — as the bar did.
///
/// Falls back to the global config, which is also the right answer for a surface that names no screen.
pub fn config_for(output: Option<&str>) -> Arc<Config> {
    output
        .and_then(|name| RUNNING.with(|running| running.borrow().get(name).cloned()))
        .or_else(crate::core::shell::config)
        .unwrap_or_default()
}

/// Every surface `config` calls for, across every output, in the order they must be created.
fn plan(path: &Path, config: &Arc<Config>, outputs: &[OutputDescriptor]) -> Vec<Planned> {
    outputs
        .iter()
        .flat_map(|out| {
            // Every surface on this output resolves against the same merged config, so a per-monitor override
            // reaches the bar, its reservation strip, the wallpaper and the frame together — a bar sized by one
            // config and a reservation strip sized by another would carve the wrong zone out of the screen.
            let config = output_config(path, config, out.name.as_deref());
            if let Some(name) = out.name.as_deref() {
                RUNNING.with(|running| {
                    running
                        .borrow_mut()
                        .insert(name.to_string(), Arc::clone(&config))
                });
            }
            plan_output(&config, out.name.as_deref())
        })
        .collect()
}

/// One output's surfaces, in stacking order within each layer: the wallpaper first so it sits at the bottom of
/// the background layer, then the bars and their strips, then the frame ring over the wallpaper.
fn plan_output(config: &Arc<Config>, output: Option<&str>) -> Vec<Planned> {
    let mut planned = Vec::new();
    let mut push = |role: Role, layer: LayerConfig| {
        planned.push(Planned {
            key: Key {
                output: output.map(str::to_string),
                role,
            },
            layer,
            config: Arc::clone(config),
        });
    };
    if config.background.is_enabled() {
        push(Role::Wallpaper, wallpaper_layer_config(output));
    }
    for edge in Edge::ALL {
        if !config.edge_present(edge) || config.bars.excludes(output) {
            continue;
        }
        push(Role::Bar(edge), bar_layer_config(config, edge, output));
        // Driven off what the edge reserves rather than off whether its bar hides: an auto-hidden bar under
        // `[shape] frame` still reserves its ring, and an edge that reserves nothing gets no strip at all
        // rather than one sized zero — a mapped surface with an empty exclusive zone is still a surface for the
        // compositor to configure and the driver to drive.
        if config.edge_reserved(edge) > 0 {
            push(Role::Reserve(edge), reservation_layer_config(config, edge, output));
        }
    }
    if config.shape.frame {
        push(Role::Frame, frame_layer_config(output));
    }
    planned
}

/// The config `output` runs under: its `monitors/<output>/config.toml` merged over the global one, falling back
/// to the global config when it has no override or that override will not parse. A broken per-monitor file
/// costs that one screen its overrides and a log line, never the whole shell's layout.
fn output_config(path: &Path, global: &Arc<Config>, output: Option<&str>) -> Arc<Config> {
    let Some(output) = output else {
        return Arc::clone(global);
    };
    match Config::for_output(path, Some(output)) {
        Ok(config) => Arc::new(config),
        Err(e) => {
            tracing::warn!("monitor '{output}': {e}; using the global config");
            Arc::clone(global)
        }
    }
}

/// Insets past the perpendicular bar's own gap+thickness (not the vertical bar's gap) so a floating perpendicular bar can't overlap a hugging vertical one.
///
/// Driven off what the perpendicular edge actually reserves rather than off whether it has a bar, so an
/// auto-hidden one insets by its frame ring (which is still there) and not by the bar thickness (which is not).
/// Insetting a full-height left bar past a top bar that is off screen would leave a permanent notch for
/// something the user asked not to see; the cost is that a revealed auto-hidden bar overlaps the corner while
/// it is out, which is the right way round — the notch would be there always, the overlap only under the
/// pointer.
fn perpendicular_inset(config: &Config, perp: Edge, own_gap: i32) -> i32 {
    match config.edge_reserved(perp) {
        0 => own_gap,
        reserved => reserved as i32,
    }
}

/// The layer the shell's own chrome sits on: `Overlay` keeps the bars above a fullscreen window when
/// `[general] show_over_fullscreen` asks for it, `Top` (the default) lets fullscreen cover them.
fn chrome_layer(config: &Config) -> Layer {
    if config.general.show_over_fullscreen {
        Layer::Overlay
    } else {
        Layer::Top
    }
}

/// The margin `edge`'s bar sits at while it is on screen, as `(top, right, bottom, left)`: its own outer gap on
/// the edge it hangs off, plus — for a vertical bar — the insets that keep it clear of a perpendicular one.
///
/// Lifted out of [`bar_layer_config`] because an auto-hiding bar needs the same answer from the other side: the
/// surface is created at its *hidden* margin and animates back to this one, and the two deriving the gap
/// separately is how a revealed bar ends up a few pixels off the position it was configured for.
pub(crate) fn bar_margin_for(config: &Config, edge: Edge) -> (i32, i32, i32, i32) {
    let gap = config.edge_gap(edge) as i32;
    let top_inset = perpendicular_inset(config, Edge::Top, gap);
    let bottom_inset = perpendicular_inset(config, Edge::Bottom, gap);
    match edge {
        Edge::Top => (gap, gap, 0, gap),
        Edge::Bottom => (0, gap, gap, gap),
        Edge::Left => (top_inset, 0, bottom_inset, gap),
        Edge::Right => (top_inset, gap, bottom_inset, 0),
    }
}

/// exclusive_zone = -1 pins position independent of surface-creation order; vertical bars inset at each end (Invariant 1) to keep corner cells clear.
///
/// An auto-hidden bar is created at its hidden margin — off its own edge but for its peek strip — rather than
/// being placed on screen and moved a frame later, which the user would see as a bar that flashes on at every
/// reload before deciding to leave.
fn bar_layer_config(config: &Config, edge: Edge, output: Option<&str>) -> LayerConfig {
    let thickness = config.edge_thickness(edge);
    let (anchor, surface_size) = match edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, thickness)),
        Edge::Bottom => (
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            (0, thickness),
        ),
        Edge::Left => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (thickness, 0)),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (thickness, 0)),
    };
    let shown = bar_margin_for(config, edge);
    let margin = if config.bar_is_persistent(edge) {
        shown
    } else {
        crate::modules::bar::RevealMargins::new(config, edge, shown).hidden
    };
    LayerConfig {
        output: output.map(str::to_string),
        layer: chrome_layer(config),
        anchor,
        exclusive_zone: -1,
        size: surface_size,
        margin,
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-{}", edge.as_str()),
        reserve_only: false,
        input_transparent: false,
        interactive_input_region: false,
    }
}

/// Invisible reservation strip on Layer::Bottom: space-only, no need for Top's interactivity; order-independent.
fn reservation_layer_config(config: &Config, edge: Edge, output: Option<&str>) -> LayerConfig {
    let reserve = config.edge_reserved(edge);
    let (anchor, size) = match edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, reserve)),
        Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, reserve)),
        Edge::Left => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (reserve, 0)),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (reserve, 0)),
    };
    LayerConfig {
        output: output.map(str::to_string),
        layer: Layer::Bottom,
        anchor,
        exclusive_zone: reserve as i32,
        size,
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-reserve-{}", edge.as_str()),
        reserve_only: true,
        input_transparent: true,
        interactive_input_region: false,
    }
}

/// Full-screen wallpaper on Layer::Background: click-through, spans the whole output (exclusive_zone -1 ignores bar reservations). Planned before the frame so it stacks under it.
fn wallpaper_layer_config(output: Option<&str>) -> LayerConfig {
    LayerConfig {
        output: output.map(str::to_string),
        layer: Layer::Background,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: -1,
        size: (0, 0),
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-wallpaper"),
        reserve_only: false,
        input_transparent: true,
        interactive_input_region: false,
    }
}

/// Full-screen frame on Layer::Background: not on Top since ring visibility depends on window z-order.
fn frame_layer_config(output: Option<&str>) -> LayerConfig {
    LayerConfig {
        output: output.map(str::to_string),
        layer: Layer::Background,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: -1,
        size: (0, 0),
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-frame"),
        reserve_only: false,
        input_transparent: true,
        interactive_input_region: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(toml: &str) -> Arc<Config> {
        Arc::new(toml::from_str(toml).unwrap())
    }

    fn roles(config: &Arc<Config>) -> Vec<Role> {
        plan_output(config, None).into_iter().map(|p| p.key.role).collect()
    }

    #[test]
    fn the_plan_is_what_the_config_asks_for() {
        let bare = config("[bars.top]\ncenter=[\"clock\"]\n");
        assert_eq!(
            roles(&bare),
            vec![Role::Bar(Edge::Top), Role::Reserve(Edge::Top)],
            "one bar is a bar and the strip it reserves; nothing else is asked for"
        );

        let dressed = config(
            "[background]\nenabled=true\n[shape]\nframe=true\n[bars.top]\ncenter=[\"clock\"]\n",
        );
        assert_eq!(
            roles(&dressed).first(),
            Some(&Role::Wallpaper),
            "the wallpaper is planned first so it stacks under everything on the background layer"
        );
        assert_eq!(
            roles(&dressed).last(),
            Some(&Role::Frame),
            "and the ring after it, so it is drawn over the picture rather than under it"
        );
    }

    /// `[shape] frame` gives every edge a thickness of its own, so all four bars exist whatever is on them —
    /// which is why turning a module off there is a re-render and not a surface being destroyed.
    #[test]
    fn a_frame_puts_a_surface_on_every_edge() {
        let framed = config("[shape]\nframe=true\ninactive_size=6\n[bars.top]\ncenter=[\"clock\"]\n");
        for edge in Edge::ALL {
            assert!(
                roles(&framed).contains(&Role::Bar(edge)),
                "{edge:?} is present under a frame even with nothing on it"
            );
        }

        let bare = config("[bars.top]\ncenter=[\"clock\"]\n");
        assert!(
            !roles(&bare).contains(&Role::Bar(Edge::Left)),
            "without one, an empty edge has no surface at all"
        );
    }

    #[test]
    fn a_screen_the_bars_exclude_keeps_its_wallpaper_and_loses_its_bars() {
        let config = config(
            "[background]\nenabled=true\n\
             [bars]\nexcluded_screens=[\"HDMI-*\"]\n\
             [bars.top]\ncenter=[\"clock\"]\n",
        );
        let excluded: Vec<Role> = plan_output(&config, Some("HDMI-A-1"))
            .into_iter()
            .map(|p| p.key.role)
            .collect();
        assert_eq!(
            excluded,
            vec![Role::Wallpaper],
            "an excluded screen is excluded from the bars, not from the desktop"
        );
    }

    /// The whole point of the reconcile: an edit reaches the surfaces that are already up.
    ///
    /// Reusing is not a detail of how it is implemented — it is what the user sees. A bar replaced by a new bar
    /// is a bar that blinks off and on at every keystroke in the settings window, which is what this used to do.
    #[test]
    fn an_edit_reuses_the_surfaces_that_stay_and_only_adds_or_drops_at_the_edges() {
        let path = Path::new("config.toml");
        let screen = [OutputDescriptor {
            name: None,
            logical_size: Some((1920, 1080)),
            position: (0, 0),
            scale: 1,
        }];
        let mut surfaces = Surfaces::default();
        surfaces.reconcile(
            path,
            &config("[bars.top]\nsize=34\ncenter=[\"clock\"]\n"),
            &screen,
            Content::Rebuild,
        );
        let before = surfaces
            .config_of(Role::Bar(Edge::Top))
            .expect("the top bar is up");
        assert_eq!(surfaces.roles(), vec![Role::Bar(Edge::Top), Role::Reserve(Edge::Top)]);

        surfaces.reconcile(
            path,
            &config("[bars.top]\nsize=48\ncenter=[\"clock\"]\n[bars.bottom]\nstart=[\"clock\"]\n"),
            &screen,
            Content::Rebuild,
        );
        let after = surfaces
            .config_of(Role::Bar(Edge::Top))
            .expect("and is still up");
        assert!(
            Rc::ptr_eq(&before.0, &after.0),
            "a thicker bar is the same bar: the surface is renegotiated and rebuilt, never replaced"
        );
        assert_eq!(
            after.get().bars.get(Edge::Top).size,
            48,
            "and its next build reads the config the edit produced"
        );
        assert_eq!(
            surfaces.roles(),
            vec![
                Role::Bar(Edge::Top),
                Role::Reserve(Edge::Top),
                Role::Bar(Edge::Bottom),
                Role::Reserve(Edge::Bottom)
            ],
            "the bottom bar the config newly asks for is the only thing opened"
        );

        surfaces.reconcile(
            path,
            &config("[bars.bottom]\nstart=[\"clock\"]\n"),
            &screen,
            Content::Rebuild,
        );
        assert_eq!(
            surfaces.roles(),
            vec![Role::Bar(Edge::Bottom), Role::Reserve(Edge::Bottom)],
            "an edge the config stopped asking for is the one thing closed"
        );
    }

    #[test]
    fn only_what_changed_is_renegotiated() {
        let before = config("[bars.top]\nsize=34\ncenter=[\"clock\"]\n");
        let after = config("[bars.top]\nsize=48\ncenter=[\"clock\"]\n");
        let change = bar_layer_config(&before, Edge::Top, None)
            .delta(&bar_layer_config(&after, Edge::Top, None));
        assert_eq!(change.size, Some((0, 48)), "the bar got thicker");
        assert_eq!(
            (change.margin, change.exclusive_zone, change.anchor),
            (None, None, None),
            "and nothing else moved, so nothing else is committed"
        );

        assert!(
            bar_layer_config(&before, Edge::Top, None)
                .delta(&bar_layer_config(&before, Edge::Top, None))
                .is_empty(),
            "an edit that misses this bar costs it no commit at all"
        );
    }

    /// `[general] show_over_fullscreen` is the one non-geometry thing a live bar renegotiates.
    #[test]
    fn raising_the_bars_over_fullscreen_moves_the_live_surface() {
        let under = config("[bars.top]\ncenter=[\"clock\"]\n");
        let over = config("[general]\nshow_over_fullscreen=true\n[bars.top]\ncenter=[\"clock\"]\n");
        let change =
            bar_layer_config(&under, Edge::Top, None).delta(&bar_layer_config(&over, Edge::Top, None));
        assert_eq!(change.layer, Some(Layer::Overlay));
    }

    #[test]
    fn visible_bars_reserve_nothing_and_pin_deterministically() {
        let cfg = config("[bars.top]\ncenter=[\"clock\"]\n[bars.bottom]\nstart=[\"clock\"]\n");
        for edge in [Edge::Top, Edge::Bottom] {
            let lc = bar_layer_config(&cfg, edge, None);
            assert_eq!(lc.size, (0, 34), "{edge:?} leaves width free, pins height");
            assert_eq!(lc.exclusive_zone, -1, "visible bar reserves nothing");
            assert!(!lc.reserve_only);
            assert_eq!(lc.margin, (0, 0, 0, 0));
            assert!(lc.anchor.contains(Anchor::LEFT) && lc.anchor.contains(Anchor::RIGHT));
        }
        let top = bar_layer_config(&cfg, Edge::Top, None).anchor;
        assert!(top.contains(Anchor::TOP) && !top.contains(Anchor::BOTTOM));
        assert!(
            bar_layer_config(&cfg, Edge::Bottom, None)
                .anchor
                .contains(Anchor::BOTTOM)
        );
    }

    #[test]
    fn reservation_strip_carves_thickness_along_full_edge() {
        let cfg = config("[bars.left]\nsize=44\nstart=[\"workspaces\"]\n");
        let r = reservation_layer_config(&cfg, Edge::Left, None);
        assert!(r.reserve_only);
        assert!(
            r.input_transparent,
            "click-through so it never swallows the bar's input"
        );
        assert!(
            matches!(r.layer, Layer::Bottom),
            "spacers live below the bars, not on Top"
        );
        assert_eq!(r.exclusive_zone, 44, "reserves the bar thickness");
        assert_eq!(r.size, (44, 0));
        assert_eq!(r.margin, (0, 0, 0, 0));
        assert!(r.anchor.contains(Anchor::TOP) && r.anchor.contains(Anchor::BOTTOM));
    }

    #[test]
    fn floating_bar_gains_outer_and_end_margins_reservation_takes_gap() {
        let cfg = config("[shape]\ngap=8\nradius=12\n[bars.top]\nsize=34\ncenter=[\"clock\"]\n");
        let lc = bar_layer_config(&cfg, Edge::Top, None);
        assert_eq!(lc.margin, (8, 8, 0, 8));
        assert_eq!(lc.exclusive_zone, -1);
        let r = reservation_layer_config(&cfg, Edge::Top, None);
        assert_eq!(r.exclusive_zone, 34 + 8);
    }

    #[test]
    fn vertical_bar_ends_inset_by_adjacent_bar_thickness() {
        let cfg = config(
            "[bars.top]\nsize=30\ncenter=[\"clock\"]\n\
             [bars.bottom]\nsize=40\nstart=[\"clock\"]\n\
             [bars.left]\nsize=44\nstart=[\"workspaces\"]\n",
        );
        let left = bar_layer_config(&cfg, Edge::Left, None);
        assert_eq!(left.margin, (30, 0, 40, 0));
        let top = bar_layer_config(&cfg, Edge::Top, None);
        assert_eq!(top.margin, (0, 0, 0, 0));
    }

    #[test]
    fn vertical_bar_inset_uses_the_adjacent_bar_gap_not_its_own() {
        // Regression: a floating top bar (gap:8) ends at y=40, so a hugging left bar must inset by the top bar's gap+thickness, not its own — else it rides up over the top bar.
        let cfg = config(
            "[shape]\ngap=0\n\
             [bars.top]\nsize=32\ncenter=[\"clock\"]\n[bars.top.shape]\ngap=8\n\
             [bars.bottom]\nsize=64\nstart=[\"clock\"]\n\
             [bars.left]\nsize=32\nstart=[\"workspaces\"]\n",
        );
        let left = bar_layer_config(&cfg, Edge::Left, None);
        assert_eq!(
            left.margin,
            (40, 0, 64, 0),
            "top inset = top gap(8)+thickness(32); bottom inset = bottom gap(0)+thickness(64)"
        );
    }

    #[test]
    fn frame_forces_hug_even_with_gap() {
        let cfg = config("[shape]\nframe=true\ngap=8\n[bars.top]\ncenter=[\"clock\"]\n");
        let lc = bar_layer_config(&cfg, Edge::Top, None);
        assert_eq!(lc.margin, (0, 0, 0, 0));
        assert_eq!(lc.exclusive_zone, -1);
        let r = reservation_layer_config(&cfg, Edge::Top, None);
        assert_eq!(r.exclusive_zone, 34);
    }
}
