use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use telar::{Color, LayoutError, LayoutItem, ReadSignal, signal};

use config::theme::NordTheme;
use config::{Edge, Variant, Zone};

pub use config::{SurfaceEnv, set_surface_env, surface_env};

pub fn bar_edge() -> Edge {
    surface_env().map(|e| e.edge).unwrap_or(Edge::Top)
}

thread_local! {
    static PRESS_ORIGIN: Cell<Option<Zone>> = const { Cell::new(None) };
}

/// Runs `act` — a chip's press or drag-open handler — with the chip's own zone in scope.
///
/// The zone is what a drawer aligns to, and only the bar knows it: the same module id can sit in all three zones
/// at once, and a `[corners]` module sits in none of them, so looking the id up in the config answers the wrong
/// question (the first zone that mentions it) or no question at all (centre). What opened the drawer is the chip
/// that was pressed, and this is how that travels the closures between the press and [`crate::open_panel`].
///
/// Ambient rather than a parameter because the handler that reads it may be a `ModuleClick::Action` — a bare
/// `fn()` that opens someone else's panel — which no signature change reaches. Scoped strictly to the
/// synchronous dispatch, so nothing can read a stale origin afterwards.
pub fn from_zone<R>(zone: Zone, act: impl FnOnce() -> R) -> R {
    let previous = PRESS_ORIGIN.with(|origin| origin.replace(Some(zone)));
    let done = act();
    PRESS_ORIGIN.with(|origin| origin.set(previous));
    done
}

/// The zone of the chip whose press is being dispatched, if a press is what is running.
pub fn press_origin() -> Option<Zone> {
    PRESS_ORIGIN.with(|origin| origin.get())
}

thread_local! {
    // How a chip opens its module's panel. Installed at startup, because *which* surface a module id opens is
    // the shell's routing rather than the chip's: a chip knows it was dragged away from the bar and nothing more.
    static OPEN_PANEL: RefCell<Option<Box<dyn Fn(&str)>>> = const { RefCell::new(None) };
}

/// Registers how a module id is turned into an open panel. Set once at startup by whoever owns the routing.
pub fn set_panel_opener(open: impl Fn(&str) + 'static) {
    OPEN_PANEL.with(|hook| *hook.borrow_mut() = Some(Box::new(open)));
}

pub(crate) fn open_panel(module: &str) {
    OPEN_PANEL.with(|hook| {
        if let Some(open) = hook.borrow().as_ref() {
            open(module);
        }
    });
}

pub fn bar_is_vertical() -> bool {
    bar_edge().is_vertical()
}

#[derive(Clone, Copy)]
pub struct ModuleCtx {
    pub theme: NordTheme,
    pub accent: Color,
    /// The bar's thickness in px (height for top/bottom, width for left/right).
    pub bar_size: u32,
    pub edge: Edge,
}

thread_local! {
    // The bar sets this per module, just before building that module's content.
    static MODULE_FG: RefCell<Color> = RefCell::new(NordTheme::new().text);
}

pub fn set_module_fg(color: Color) {
    MODULE_FG.with(|c| *c.borrow_mut() = color);
}

/// Snapshot of the current module's foreground for a `.rsx` module to bind as `color:$fg`; must be called once at build time so each module captures its OWN color, not the last-set one.
pub fn module_fg() -> ReadSignal<Color> {
    signal(MODULE_FG.with(|c| *c.borrow())).read_only()
}

/// The bar's thickness in px, or a sane default outside a surface; everything a module sizes derives from this, so a thin bar yields a small, proportional chip instead of an oversized one that squashes.
pub fn bar_thickness() -> f32 {
    surface_env().map(|e| e.bar_size).unwrap_or(34) as f32
}

/// Icon size for the current bar: ~0.75 of its thickness, so the glyph fills most of its square chip and scales with the bar.
pub fn icon_px() -> f32 {
    (bar_thickness() * 0.75).round().clamp(8.0, 64.0)
}

/// The resolved chip corner radius for this bar (per-bar → `[shape]` → theme), so a self-managed module's inner elements (e.g. workspace pills) round like the sibling chips instead of a hardcoded value.
pub fn chip_radius() -> f32 {
    surface_env()
        .map(|e| e.config.shape_for(e.edge).chip_radius())
        .unwrap_or(0.0)
}

/// Chosen so the chip's width (icon ≈ 0.75·thickness + two of these ≈ 0.25·thickness) equals the bar thickness, so a chip stretched to the bar's height comes out square.
pub(crate) fn chip_pad() -> f32 {
    (bar_thickness() * 0.125).round().max(1.0)
}

/// The foreground for a container variant: the plain text token when blending into the bar (default), or the higher-contrast of text/base over the accent when filled.
pub fn module_foreground(variant: Variant, accent: Color, theme: NordTheme) -> Color {
    match variant {
        Variant::Default => theme.text,
        Variant::Filled => accent.most_readable(&[theme.text, theme.base]),
    }
}

/// A chip's drag-to-open gesture: pulling it away from the bar opens the panel it would otherwise toggle.
#[derive(Clone)]
pub struct DragOpen {
    pub module: String,
    /// The bar's edge, which is what says which direction "away from the bar" is.
    pub edge: Edge,
    /// The chip's own zone, so a dragged-open panel aligns exactly where a tapped-open one would.
    pub zone: Zone,
    /// How far the pointer must travel inwards before letting go opens the panel, in px.
    pub threshold: f32,
}

impl DragOpen {
    /// How far a drag has travelled *away from the bar*, from a press at `from` to a pointer now at `to`.
    /// Negative is back towards the bar, which is the direction that closes rather than opens.
    pub(crate) fn travel(&self, from: (f32, f32), to: (f32, f32)) -> f32 {
        match self.edge {
            Edge::Top => to.1 - from.1,
            Edge::Bottom => from.1 - to.1,
            Edge::Left => to.0 - from.0,
            Edge::Right => from.0 - to.0,
        }
    }
}

pub type ModuleBuilder = fn(&ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError>;

/// What clicking a module does: `Panel` toggles its panel (drawer or float, per the module's `open` config); `Action` runs a custom handler.
#[derive(Clone, Copy)]
pub enum ModuleClick {
    Panel,
    Action(fn()),
}

pub struct ModuleDef {
    pub builder: ModuleBuilder,
    /// If true, the bar places the module bare instead of wrapping it in [`crate::module_shell`] (e.g. the workspaces grid).
    pub self_managed: bool,
    /// If true, the container is a square chip that scales with the bar instead of a content-width text pill.
    pub icon: bool,
    /// What clicking the module does; `None` is a display-only chip.
    pub click: Option<ModuleClick>,
    /// What the wheel does over the module, as `(dx, dy)` in pixels; `None` leaves the chip inert to scroll.
    pub scroll: Option<fn(f32, f32)>,
    /// Whether resting the pointer on the chip opens its hover popout. Set from
    /// `popout::has_popout` rather than declared twice, so a module can't
    /// be wired for a card it has no content for.
    pub popout: bool,
}

impl ModuleDef {
    pub fn new(builder: ModuleBuilder) -> Self {
        Self {
            builder,
            self_managed: false,
            icon: false,
            click: None,
            scroll: None,
            popout: false,
        }
    }

    pub fn icon(mut self) -> Self {
        self.icon = true;
        self
    }

    pub fn opens(mut self) -> Self {
        self.click = Some(ModuleClick::Panel);
        self
    }

    pub fn on_click(mut self, action: fn()) -> Self {
        self.click = Some(ModuleClick::Action(action));
        self
    }

    /// Wires the wheel over this chip to `action`, receiving the scroll delta in pixels (positive `dy` is a
    /// scroll up). Used by the level modules so the chip is a control, not just a readout.
    pub fn on_scroll(mut self, action: fn(f32, f32)) -> Self {
        self.scroll = Some(action);
        self
    }

    pub fn self_managed(mut self) -> Self {
        self.self_managed = true;
        self
    }
}

#[derive(Default)]
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleDef>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &str, def: ModuleDef) {
        self.modules.insert(id.to_string(), def);
    }

    pub fn def(&self, id: &str) -> Option<&ModuleDef> {
        self.modules.get(id)
    }

    /// Every registered id, sorted. What the settings application's per-module overrides enumerate, so a chip
    /// can be restyled before it has been put on a bar.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.modules.keys().cloned().collect();
        ids.sort_unstable();
        ids
    }

    /// Marks every registered module the popout layer has card content for. Driven off that list rather than
    /// declared per module, so the two cannot drift into a chip that opens an empty card.
    pub fn wire_popouts(&mut self, has_popout: impl Fn(&str) -> bool) {
        for (id, def) in self.modules.iter_mut() {
            def.popout = has_popout(id);
        }
    }

    /// Every registered module. What lets a test walk the table and check the roles against the routing it is
    /// supposed to match.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ModuleDef)> {
        self.modules.iter()
    }

    pub fn build(
        &self,
        id: &str,
        ctx: &ModuleCtx,
    ) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
        self.modules.get(id).map(|d| (d.builder)(ctx))
    }
}

thread_local! {
    static LIVE: RefCell<Option<ModuleRegistry>> = const { RefCell::new(None) };
}

/// Publishes the module vocabulary every bar builds from, and that the settings application enumerates. Set once
/// at startup by whoever owns the module list — the same arrangement as [`crate::panels`], so neither the bar
/// nor a panel that lists modules has to name one.
pub fn install(registry: ModuleRegistry) {
    LIVE.with(|live| *live.borrow_mut() = Some(registry));
}

/// Runs `act` against the installed registry, or against an empty one when nothing has been installed — a bar
/// with no modules rather than a panic, which is what a test that never composed a shell should see.
pub fn with_registry<R>(act: impl FnOnce(&ModuleRegistry) -> R) -> R {
    LIVE.with(|live| match live.borrow().as_ref() {
        Some(registry) => act(registry),
        None => act(&ModuleRegistry::new()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_drag_opens_a_panel_only_when_it_pulls_away_from_the_bar() {
        let gesture = |edge| DragOpen {
            module: "clock".to_string(),
            edge,
            zone: Zone::Start,
            threshold: 48.0,
        };
        // "Away from the bar" is a different direction on each edge, and the sign is what decides.
        assert_eq!(gesture(Edge::Top).travel((10.0, 5.0), (10.0, 65.0)), 60.0);
        assert_eq!(
            gesture(Edge::Bottom).travel((10.0, 65.0), (10.0, 5.0)),
            60.0
        );
        assert_eq!(gesture(Edge::Left).travel((5.0, 10.0), (65.0, 10.0)), 60.0);
        assert_eq!(gesture(Edge::Right).travel((65.0, 10.0), (5.0, 10.0)), 60.0);

        assert!(gesture(Edge::Top).travel((10.0, 65.0), (10.0, 5.0)) < 0.0);
        assert_eq!(gesture(Edge::Top).travel((10.0, 20.0), (300.0, 20.0)), 0.0);
    }

    #[test]
    fn module_foreground_default_is_text_filled_is_contrast() {
        let theme = NordTheme::new();
        assert_eq!(
            module_foreground(Variant::Default, theme.orange, theme),
            theme.text,
            "default variant paints with the plain text token"
        );
        let filled = module_foreground(Variant::Filled, theme.orange, theme);
        assert!(
            filled == theme.text || filled == theme.base,
            "filled foreground is one of the two theme foregrounds"
        );
        assert_eq!(
            filled, theme.base,
            "over the light orange accent, the dark base wins the contrast"
        );
    }
}
