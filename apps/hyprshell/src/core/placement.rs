//! Where a surface sits, as one vocabulary.
//!
//! Every window this shell puts on screen used to describe itself: five modules built a whole `LayerConfig`
//! by hand, three more went through the surface host's own placement type, and each named its layer-shell
//! namespace in a string literal of its own. The result was four ways to say "a panel anchored to an edge"
//! and no way to tell, from one of them, what the others had decided.
//!
//! There is one way here. A [`Placement`] is built from a **named primitive** — the shape the surface takes on
//! screen — and adjusted with the handful of modifiers a surface actually varies: its size, its margin, how
//! much of the keyboard it wants, what it does with the pointer. The primitives are the shell's whole
//! taxonomy of windows:
//!
//! | Primitive | The shape | Who takes it |
//! | --- | --- | --- |
//! | [`bar`](Placement::bar) | spans an edge, reserves nothing itself | the bars |
//! | [`reservation`](Placement::reservation) | invisible, carves an edge's strip | the bars' strips |
//! | [`backdrop`](Placement::backdrop) | the whole screen, under everything, click-through | wallpaper, frame ring |
//! | [`dock`](Placement::dock) | spans an edge, over the windows | the notification centre |
//! | [`stack`](Placement::stack) | pinned to a spot along an edge, input from its content | toasts, notification popups |
//! | [`card`](Placement::card) | beside the chip that opened it | popouts, the tray menu |
//! | [`sheet`](Placement::sheet) | hangs off a bar edge behind a scrim | a module's drawer |
//! | [`window`](Placement::window) | centred, framed, resizable | a module's float |
//! | [`modal`](Placement::modal) | owns the screen and the keyboard | the launcher |
//! | [`flash`](Placement::flash) | brief, click-through, self-dismissing | the OSD |
//! | [`screen`](Placement::screen) | the whole screen, over everything, takes the keyboard | the region picker |
//!
//! **The namespace belongs to the shape, not to the surface.** It is what a compositor rule matches
//! (`layer_rule = blur, hyprshell-drawer`), so it is a public interface: the strings below are the ones
//! hyprshell has always announced, and a primitive owns its own rather than each call site spelling it out.
//!
//! A placement is lowered at the point of use: to a [`LayerConfig`] for a surface that owns its rendering, or
//! to the surface host's [`SurfacePlacement`] for one that wants the scaffold — a scrim, a dismiss-on-outside,
//! an entrance. Which of the two a surface needs is not a property of where it sits, so it is not decided
//! here; see [`crate::core::surfaces::open`].

use platform_layershell::{Anchor, KeyboardInteractivity, Layer, LayerConfig};
use telar::{
    AlignItems, JustifyContent, KeyboardMode, LayoutStyle, Rect, SizeDimension, SurfaceAlign,
    SurfaceAnchor, SurfacePlacement, SurfaceRole, SurfaceSize,
};

use crate::core::config::{Align, Edge};
use crate::shared::module::SurfaceEnv;

/// What a surface does with the pointer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Input {
    /// Everything inside the surface takes the pointer, which is what a panel wants.
    #[default]
    Solid,
    /// Nothing does: clicks go through to whatever is behind. A wallpaper, a frame ring, an OSD.
    Transparent,
    /// Only where the content actually draws something pressable, recomputed as it changes — a stack of
    /// cards with gaps between them, where the gaps belong to the window underneath.
    FromContent,
}

/// The screen-shape a surface takes, and everything the compositor needs to place it.
#[derive(Clone, Debug)]
pub struct Placement {
    namespace: &'static str,
    anchor: Anchor,
    layer: Layer,
    size: (u32, u32),
    margin: (i32, i32, i32, i32),
    exclusive_zone: i32,
    keyboard: KeyboardMode,
    input: Input,
    output: Option<String>,
    /// The edge this surface hangs off, when it hangs off one.
    ///
    /// Kept rather than re-derived from `anchor`, and that is the whole reason it exists: a card beside a chip
    /// on a *left* bar is anchored `LEFT | TOP` — the edge it hangs off and the axis it lines up along — and
    /// reading the flags back cannot tell which is which. The scaffold needs the edge, so the edge is
    /// remembered.
    edge: Option<Edge>,
    /// Set by the primitives that are hosted rather than self-rendered; read only when lowering to a
    /// [`SurfacePlacement`], where it decides the scaffold and the entrance.
    role: SurfaceRole,
    align: SurfaceAlign,
    scrim: bool,
    dismiss_on_outside: bool,
    timeout: Option<std::time::Duration>,
    /// A reservation strip is a surface with no content at all — an exclusive zone and a transparent buffer.
    reserve_only: bool,
}

impl Placement {
    fn new(namespace: &'static str, anchor: Anchor, layer: Layer) -> Self {
        Self {
            namespace,
            anchor,
            layer,
            size: (0, 0),
            margin: (0, 0, 0, 0),
            exclusive_zone: 0,
            keyboard: KeyboardMode::None,
            input: Input::Solid,
            output: None,
            edge: None,
            role: SurfaceRole::Popup,
            align: SurfaceAlign::Center,
            scrim: false,
            dismiss_on_outside: false,
            timeout: None,
            reserve_only: false,
        }
    }

    /// A bar: spans its edge, and reserves nothing *itself* — `-1` opts out of every other surface's zone so
    /// its position does not depend on which surface was created first. Its strip is a separate surface.
    pub fn bar(edge: Edge, thickness: u32) -> Self {
        let mut placement = Self::new(bar_namespace(edge), spanning(edge), Layer::Top).zone(-1);
        placement.edge = Some(edge);
        placement.size = across(edge, thickness);
        placement
    }

    /// The invisible strip that carves an edge's space out of every window's idea of the screen.
    pub fn reservation(edge: Edge, thickness: u32) -> Self {
        let mut placement = Self::new(reserve_namespace(edge), spanning(edge), Layer::Bottom)
            .zone(thickness as i32)
            .input(Input::Transparent);
        placement.size = across(edge, thickness);
        placement.reserve_only = true;
        placement
    }

    /// The whole screen, at the bottom of the stack and click-through: what is painted *behind* the desktop
    /// rather than on it. `-1` so a bar's reserved strip does not shrink it.
    pub fn backdrop(namespace: &'static str) -> Self {
        Self::new(namespace, FULLSCREEN, Layer::Background)
            .zone(-1)
            .input(Input::Transparent)
    }

    /// A panel that spans an edge and sits over the windows. Zero zone, not `-1`: the compositor has already
    /// cleared the bars, and a dock adds only the shared panel margin beyond them.
    pub fn dock(namespace: &'static str, edge: Edge, thickness: u32) -> Self {
        let mut placement = Self::new(namespace, spanning(edge), Layer::Overlay);
        placement.edge = Some(edge);
        placement.size = across(edge, thickness);
        placement
    }

    /// A run of cards pinned to one spot along an edge. Its input region is carved from the cards, so the
    /// gaps between them belong to whatever the user is working in.
    ///
    /// Lay the cards out with [`column`](Self::column) — a stack's surface is sized for a full run, so where a
    /// short run sits inside it is the difference between hugging the edge and floating in mid-screen.
    pub fn stack(namespace: &'static str, edge: Edge, align: Align) -> Self {
        let mut placement =
            Self::new(namespace, cornered(edge, align), Layer::Overlay).input(Input::FromContent);
        placement.edge = Some(edge);
        placement.align = surface_align(align);
        placement
    }

    /// The column a [`stack`](Self::stack)'s cards lay out in: the full width of the surface, packed against the
    /// same end of it the surface itself is pinned to.
    ///
    /// A stack asks the compositor for room for a *full* run of cards, because a layer surface names its size
    /// before it knows what it will hold. So a run of one is a card in a box several times its height, and where
    /// it sits in that box is entirely up to this: packed the wrong way, a single toast on a bottom-anchored
    /// stack renders a full stack's height above the bar, which reads as floating in the middle of the screen.
    ///
    /// Derived from the placement rather than passed alongside it so the two cannot disagree.
    pub fn column(&self, gap: f32) -> LayoutStyle {
        LayoutStyle::new()
            .flex_column()
            .gap(gap)
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::STRETCH)
            .justify_content(self.packing())
    }

    /// Which end of its surface a stack's cards pack against: the edge it hangs off when that edge is the one
    /// the cards run along, and otherwise the alignment that pins it along a vertical edge.
    fn packing(&self) -> JustifyContent {
        match self.edge {
            Some(Edge::Top) => JustifyContent::START,
            Some(Edge::Bottom) => JustifyContent::END,
            _ => match self.align {
                SurfaceAlign::Start => JustifyContent::START,
                SurfaceAlign::Center => JustifyContent::CENTER,
                SurfaceAlign::End => JustifyContent::END,
            },
        }
    }

    /// A card beside the chip that opened it, on the bar's own screen — the anchoring is
    /// [`shared::anchor`](crate::shared::anchor), shared with everything that hangs off a chip.
    pub fn card(namespace: &'static str, env: &SurfaceEnv, chip: Rect, span: Option<f32>) -> Self {
        let off_bar = env.config.panel_gap(env.edge) as f32;
        let mut placement = Self::new(namespace, beside_a_chip(env.edge), Layer::Overlay)
            .margin(crate::shared::anchor::chip_margin(env, chip, off_bar, span))
            .input(Input::FromContent)
            .output(env.output.clone());
        placement.edge = Some(env.edge);
        placement
    }

    /// A panel hanging off a bar edge, behind a scrim that dismisses it — a module's drawer.
    ///
    /// `keyboard` is asked for rather than assumed: a layer surface granted focus takes it from the window
    /// the user was in, and gives it back when the panel closes — which moves a scrolling layout on the way
    /// back. A panel that only shows readings must not provoke that.
    pub fn sheet(edge: Edge, keyboard: KeyboardMode) -> Self {
        let mut placement = Self::hosted(SurfaceRole::Drawer, "hyprshell-drawer", keyboard);
        placement.anchor = edge_anchor(edge);
        placement.edge = Some(edge);
        placement.scrim = true;
        placement.dismiss_on_outside = true;
        placement
    }

    /// A centred window with its own title bar and close button — a module's float.
    pub fn window(size: (u32, u32), keyboard: KeyboardMode) -> Self {
        let mut placement = Self::hosted(SurfaceRole::Float, "hyprshell-float", keyboard);
        placement.size = size;
        placement
    }

    /// A surface that owns the screen while it is up, keyboard included: the launcher, a command palette.
    ///
    /// The scrim is not decoration. It is what makes this a *full-screen* surface with the panel positioned
    /// inside it — without one the surface is centred, unanchored and unsized, which layer-shell rejects
    /// outright (a surface not anchored to both edges of an axis has to name a size on it).
    pub fn modal() -> Self {
        let mut placement = Self::hosted(
            SurfaceRole::Overlay,
            "hyprshell-overlay",
            KeyboardMode::Exclusive,
        );
        placement.scrim = true;
        placement.dismiss_on_outside = true;
        placement
    }

    /// A brief, click-through status flash that takes itself away again.
    pub fn flash(edge: Edge, align: Align, after: std::time::Duration) -> Self {
        let mut placement = Self::hosted(SurfaceRole::Osd, "hyprshell-osd", KeyboardMode::None)
            .input(Input::Transparent);
        placement.anchor = edge_anchor(edge);
        placement.edge = Some(edge);
        placement.align = surface_align(align);
        placement.timeout = (!after.is_zero()).then_some(after);
        placement
    }

    /// The whole screen, over everything, holding the keyboard: the region picker. Over a fullscreen window
    /// on purpose — the user asked to select a region of what they can see.
    pub fn screen(namespace: &'static str) -> Self {
        Self::new(namespace, FULLSCREEN, Layer::Overlay)
            .zone(-1)
            .keyboard(KeyboardMode::Exclusive)
    }

    fn hosted(role: SurfaceRole, namespace: &'static str, keyboard: KeyboardMode) -> Self {
        let mut placement = Self::new(namespace, Anchor::empty(), Layer::Overlay);
        placement.role = role;
        placement.keyboard = keyboard;
        placement
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = (width, height);
        self
    }

    pub fn margin(mut self, margin: (i32, i32, i32, i32)) -> Self {
        self.margin = margin;
        self
    }

    /// The same gap off every edge — what a surface that floats free of the screen's corners wants.
    pub fn inset(self, px: i32) -> Self {
        self.margin((px, px, px, px))
    }

    /// Dismissed by a press outside it. On its own, without a scrim: what the tray's context menus want,
    /// where dimming the screen behind a small menu would be theatre.
    pub fn dismissable(mut self) -> Self {
        self.dismiss_on_outside = true;
        self
    }

    pub fn zone(mut self, zone: i32) -> Self {
        self.exclusive_zone = zone;
        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    pub fn keyboard(mut self, keyboard: KeyboardMode) -> Self {
        self.keyboard = keyboard;
        self
    }

    pub fn input(mut self, input: Input) -> Self {
        self.input = input;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = surface_align(align);
        self
    }

    pub fn output(mut self, output: Option<String>) -> Self {
        self.output = output;
        self
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// The compositor's view of this surface: what a self-rendering surface is opened with.
    pub fn layer_config(&self) -> LayerConfig {
        LayerConfig {
            output: self.output.clone(),
            layer: self.layer,
            anchor: self.anchor,
            exclusive_zone: self.exclusive_zone,
            size: self.size,
            margin: self.margin,
            keyboard_interactivity: match self.keyboard {
                KeyboardMode::None => KeyboardInteractivity::None,
                KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
                KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
            },
            namespace: self.namespace.to_string(),
            reserve_only: self.reserve_only,
            input_transparent: self.input == Input::Transparent,
            interactive_input_region: self.input == Input::FromContent,
        }
    }

    /// The surface host's view: what a surface that wants the scaffold — a scrim, a dismiss-on-outside, an
    /// entrance — is opened with. The host derives its own layer config from this, which is why the
    /// namespaces above have to agree with the ones it uses.
    pub fn hosted_placement(&self) -> SurfacePlacement {
        let mut placement = SurfacePlacement::new(self.role, host_anchor(self.edge))
            .align(self.align)
            .keyboard_mode(self.keyboard)
            .output(self.output.clone())
            .margin(self.margin)
            .input_transparent(self.input == Input::Transparent);
        if self.size != (0, 0) {
            placement = placement.size(SurfaceSize::Fixed(self.size.0, self.size.1));
        }
        if self.scrim {
            placement = placement.scrim(true);
        }
        if self.dismiss_on_outside {
            placement = placement.dismiss_on_outside(true);
        }
        if let Some(after) = self.timeout {
            placement = placement.timeout(after);
        }
        placement
    }
}

const FULLSCREEN: Anchor = Anchor::TOP
    .union(Anchor::BOTTOM)
    .union(Anchor::LEFT)
    .union(Anchor::RIGHT);

/// One bar per edge, so the edge is what names it — the string a `layer_rule` in the user's compositor
/// config matches, which is why it is spelled out rather than derived from a debug format.
fn bar_namespace(edge: Edge) -> &'static str {
    match edge {
        Edge::Top => "hyprshell-top",
        Edge::Bottom => "hyprshell-bottom",
        Edge::Left => "hyprshell-left",
        Edge::Right => "hyprshell-right",
    }
}

fn reserve_namespace(edge: Edge) -> &'static str {
    match edge {
        Edge::Top => "hyprshell-reserve-top",
        Edge::Bottom => "hyprshell-reserve-bottom",
        Edge::Left => "hyprshell-reserve-left",
        Edge::Right => "hyprshell-reserve-right",
    }
}

/// Anchored to `edge` and stretched along it — a bar, a strip, a dock.
fn spanning(edge: Edge) -> Anchor {
    match edge {
        Edge::Top => Anchor::TOP.union(Anchor::LEFT).union(Anchor::RIGHT),
        Edge::Bottom => Anchor::BOTTOM.union(Anchor::LEFT).union(Anchor::RIGHT),
        Edge::Left => Anchor::LEFT.union(Anchor::TOP).union(Anchor::BOTTOM),
        Edge::Right => Anchor::RIGHT.union(Anchor::TOP).union(Anchor::BOTTOM),
    }
}

/// Pinned to one spot along `edge`: the edge itself, plus the end `align` names. Centre pins nothing more, so
/// the compositor centres it along that edge.
fn cornered(edge: Edge, align: Align) -> Anchor {
    let mut anchor = edge_anchor(edge);
    let (start, end) = if edge.is_horizontal() {
        (Anchor::LEFT, Anchor::RIGHT)
    } else {
        (Anchor::TOP, Anchor::BOTTOM)
    };
    match align {
        Align::Start => anchor |= start,
        Align::End => anchor |= end,
        Align::Center => {}
    }
    anchor
}

/// The two edges a chip-anchored card pins itself to: the bar's own, so it hangs off it, and the one it runs
/// along, so the margin that lines it up with the chip means something.
fn beside_a_chip(edge: Edge) -> Anchor {
    match edge {
        Edge::Top => Anchor::TOP.union(Anchor::LEFT),
        Edge::Bottom => Anchor::BOTTOM.union(Anchor::LEFT),
        Edge::Left => Anchor::LEFT.union(Anchor::TOP),
        Edge::Right => Anchor::RIGHT.union(Anchor::TOP),
    }
}

fn edge_anchor(edge: Edge) -> Anchor {
    match edge {
        Edge::Top => Anchor::TOP,
        Edge::Bottom => Anchor::BOTTOM,
        Edge::Left => Anchor::LEFT,
        Edge::Right => Anchor::RIGHT,
    }
}

/// The host describes an anchor as the single edge a panel hangs off — it positions the panel inside a
/// full-screen scaffold — so a surface that hangs off none is centred.
fn host_anchor(edge: Option<Edge>) -> SurfaceAnchor {
    match edge {
        Some(Edge::Top) => SurfaceAnchor::Top,
        Some(Edge::Bottom) => SurfaceAnchor::Bottom,
        Some(Edge::Left) => SurfaceAnchor::Left,
        Some(Edge::Right) => SurfaceAnchor::Right,
        None => SurfaceAnchor::Center,
    }
}

fn surface_align(align: Align) -> SurfaceAlign {
    match align {
        Align::Start => SurfaceAlign::Start,
        Align::Center => SurfaceAlign::Center,
        Align::End => SurfaceAlign::End,
    }
}

/// A surface's own thickness on `edge`, as a layer-shell size: the axis it spans is handed back to the
/// compositor with a zero.
fn across(edge: Edge, thickness: u32) -> (u32, u32) {
    if edge.is_horizontal() {
        (0, thickness)
    } else {
        (thickness, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stack's cards pack against the very edge its surface is pinned to.
    ///
    /// The surface is sized for a *full* run of cards, because a layer surface names its size before it knows
    /// what it will hold. Pack the column the wrong way and a run of one sits a full stack's height away from
    /// the edge it belongs to — a single toast on a bottom-anchored stack rendering mid-screen, which is what
    /// this looked like before the column came from the placement instead of from a default.
    #[test]
    fn a_stack_packs_its_cards_against_the_edge_it_hangs_off() {
        let stack = |edge, align| Placement::stack("hyprshell-toasts", edge, align).packing();
        assert_eq!(stack(Edge::Bottom, Align::Center), JustifyContent::END);
        assert_eq!(stack(Edge::Top, Align::Center), JustifyContent::START);
        // The alignment decides only where the surface sits *along* a horizontal edge, never which way its
        // cards pile up from it.
        assert_eq!(stack(Edge::Bottom, Align::Start), JustifyContent::END);

        // On a vertical edge the cards run along the edge itself, so the alignment is the only thing that says
        // which end of it they start from.
        assert_eq!(stack(Edge::Left, Align::Start), JustifyContent::START);
        assert_eq!(stack(Edge::Left, Align::End), JustifyContent::END);
        assert_eq!(stack(Edge::Right, Align::Center), JustifyContent::CENTER);
    }

    /// Every primitive must produce a surface the compositor will accept.
    ///
    /// Layer-shell rejects a surface that names no size on an axis it is not anchored to *both* edges of — and
    /// rejects it by killing the surface, which reaches the user as a flood of `Protocol error` and a window
    /// that never appears. A scaffolded placement is exempt because the host makes it full-screen and positions
    /// the panel inside it, so this is the same question the host asks, asked here over the whole taxonomy.
    ///
    /// It is not hypothetical: `modal` shipped without its scrim for exactly one build, which took it out of
    /// the scaffolded branch and left the launcher unanchored and unsized. Pressing the search chip killed it.
    #[test]
    fn every_primitive_is_a_surface_the_compositor_will_accept() {
        let chip = Rect {
            x: 200.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
        };
        let env = SurfaceEnv {
            edge: Edge::Top,
            bar_size: 34,
            output: None,
            config: std::sync::Arc::new(crate::core::config::Config::starter()),
        };
        let every = [
            ("bar", Placement::bar(Edge::Top, 34)),
            ("reservation", Placement::reservation(Edge::Top, 34)),
            ("backdrop", Placement::backdrop("hyprshell-wallpaper")),
            ("dock", Placement::dock("hyprshell-sidebar", Edge::Right, 380)),
            (
                "stack",
                Placement::stack("hyprshell-toasts", Edge::Top, Align::End).size(320, 200),
            ),
            (
                "card",
                Placement::card("hyprshell-popout", &env, chip, Some(260.0)).size(260, 180),
            ),
            ("sheet", Placement::sheet(Edge::Top, KeyboardMode::None)),
            ("window", Placement::window((920, 680), KeyboardMode::None)),
            ("modal", Placement::modal()),
            (
                "flash",
                Placement::flash(Edge::Top, Align::Center, std::time::Duration::from_secs(2))
                    .size(280, 60),
            ),
            ("screen", Placement::screen("hyprshell-picker")),
        ];

        for (name, placement) in every {
            if placement.scrim || placement.dismiss_on_outside {
                continue;
            }
            let layer = placement.layer_config();
            let spans_x = layer.anchor.contains(Anchor::LEFT) && layer.anchor.contains(Anchor::RIGHT);
            let spans_y = layer.anchor.contains(Anchor::TOP) && layer.anchor.contains(Anchor::BOTTOM);
            assert!(
                spans_x || layer.size.0 > 0,
                "`{name}` names no width and is not anchored to both side edges: the compositor rejects it"
            );
            assert!(
                spans_y || layer.size.1 > 0,
                "`{name}` names no height and is not anchored to top and bottom: the compositor rejects it"
            );
        }
    }

    /// The namespaces are a public interface — a user's `layer_rule` matches on them — so they are asserted,
    /// not left to whatever a refactor happens to produce.
    #[test]
    fn every_primitive_announces_the_namespace_it_always_has() {
        assert_eq!(Placement::bar(Edge::Top, 34).namespace, "hyprshell-top");
        assert_eq!(
            Placement::reservation(Edge::Left, 40).namespace,
            "hyprshell-reserve-left"
        );
        assert_eq!(
            Placement::sheet(Edge::Top, KeyboardMode::None).namespace,
            "hyprshell-drawer"
        );
        assert_eq!(
            Placement::window((100, 100), KeyboardMode::None).namespace,
            "hyprshell-float"
        );
        assert_eq!(Placement::modal().namespace, "hyprshell-overlay");
        assert_eq!(
            Placement::flash(Edge::Top, Align::Center, std::time::Duration::ZERO).namespace,
            "hyprshell-osd"
        );
    }

    #[test]
    fn a_bar_spans_its_edge_and_a_stack_pins_to_a_corner() {
        let bar = Placement::bar(Edge::Top, 34).layer_config();
        assert!(bar.anchor.contains(Anchor::LEFT) && bar.anchor.contains(Anchor::RIGHT));
        assert_eq!(bar.exclusive_zone, -1, "a bar reserves through its strip");

        for edge in Edge::ALL {
            assert!(
                Placement::stack("hyprshell-toasts", edge, Align::Center)
                    .layer_config()
                    .anchor
                    .contains(edge_anchor(edge)),
                "a stack anchors to its own edge, on all four of them"
            );
        }

        let stack = Placement::stack("hyprshell-toasts", Edge::Top, Align::End).layer_config();
        assert!(
            stack.anchor.contains(Anchor::TOP) && stack.anchor.contains(Anchor::RIGHT),
            "an end-aligned stack on a horizontal edge pins to that edge's right"
        );
        assert!(
            !stack.anchor.contains(Anchor::LEFT),
            "and not to both, or it would stretch instead of pinning"
        );
        // The same word means a different anchor along a vertical edge, which is why this is not one arm.
        let side = Placement::stack("hyprshell-toasts", Edge::Left, Align::End).layer_config();
        assert!(
            side.anchor.contains(Anchor::BOTTOM) && !side.anchor.contains(Anchor::RIGHT),
            "along a vertical edge, the end is its bottom"
        );
        assert!(
            stack.interactive_input_region,
            "the gaps between cards belong to whatever is underneath"
        );
    }

    /// Every surface that carries no content is click-through, and only those.
    #[test]
    fn what_takes_the_pointer_is_decided_once() {
        assert!(
            Placement::backdrop("hyprshell-wallpaper")
                .layer_config()
                .input_transparent
        );
        assert!(
            Placement::reservation(Edge::Top, 30)
                .layer_config()
                .input_transparent
        );
        let dock = Placement::dock("hyprshell-sidebar", Edge::Right, 380).layer_config();
        assert!(!dock.input_transparent && !dock.interactive_input_region);
    }

    /// The host positions a panel inside a full-screen scaffold, so it wants the one edge the panel hangs
    /// off — a spanning anchor has to collapse to it rather than confusing the scaffold with two.
    #[test]
    fn a_hosted_placement_keeps_the_edge_it_hangs_off() {
        let sheet = Placement::sheet(Edge::Right, KeyboardMode::OnDemand).hosted_placement();
        assert_eq!(sheet.anchor, SurfaceAnchor::Right);
        assert!(sheet.scrim && sheet.dismiss_on_outside);
        assert_eq!(sheet.keyboard, KeyboardMode::OnDemand);

        let modal = Placement::modal().hosted_placement();
        assert_eq!(
            modal.anchor,
            SurfaceAnchor::Center,
            "a modal is not anchored to an edge at all"
        );
        assert_eq!(modal.keyboard, KeyboardMode::Exclusive);
    }
}
