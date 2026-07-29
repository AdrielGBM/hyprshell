use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use rsx::{
    AlignItems, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle, ReadSignal, RectStyle,
    StyledContainer, signal,
};

use crate::core::config::{Config, Edge, Variant};
use crate::shared::theme::NordTheme;

/// What a module needs to know about its bar; carried into the parameterless `.rsx` module entrypoints as
/// per-surface context (rsx `provide`/`inject`, scoped to each surface) with no prop plumbing.
#[derive(Clone)]
pub struct SurfaceEnv {
    pub edge: Edge,
    /// The bar's thickness in px (height for top/bottom, width for left/right).
    pub bar_size: u32,
    /// The monitor this bar lives on, so panels it opens (drawer/float/OSD) land on the same screen; `None` = the compositor's active/default output.
    pub output: Option<String>,
    pub config: Arc<Config>,
}

pub fn set_surface_env(env: SurfaceEnv) {
    // Per-surface context (rsx `provide`): resolves against this surface's service scope, so a module reading
    // `surface_env()` — including from an effect — gets THIS bar's env even though all surfaces share one UI
    // thread under M3 (the reactive flush re-enters the surface). Provided once per surface build; a fresh
    // surface per config reload means no duplicate registration.
    let _ = rsx::provide(env);
}

pub fn surface_env() -> Option<SurfaceEnv> {
    rsx::try_inject::<SurfaceEnv>()
}

pub fn bar_edge() -> Edge {
    surface_env().map(|e| e.edge).unwrap_or(Edge::Top)
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
fn chip_pad() -> f32 {
    (bar_thickness() * 0.125).round().max(1.0)
}

/// The foreground for a container variant: the plain text token when blending into the bar (default), or the higher-contrast of text/base over the accent when filled.
pub fn module_foreground(variant: Variant, accent: Color, theme: NordTheme) -> Color {
    match variant {
        Variant::Default => theme.text,
        Variant::Filled => accent.most_readable(&[theme.text, theme.base]),
    }
}

/// How a chip paints itself: everything [`module_shell`] needs that isn't behaviour.
#[derive(Clone, Copy)]
pub struct ChipStyle {
    pub variant: Variant,
    /// The resting background: transparent when blending into the bar, the surface token as a free-standing chip.
    pub rest: Color,
    pub accent: Color,
    pub theme: NordTheme,
    pub radius: f32,
    /// A square icon chip that scales with the bar, rather than a content-width text pill.
    pub square: bool,
}

/// A chip's drag-to-open gesture: pulling it away from the bar opens the panel it would otherwise toggle.
#[derive(Clone)]
pub struct DragOpen {
    pub module: String,
    /// The bar's edge, which is what says which direction "away from the bar" is.
    pub edge: Edge,
    /// How far the pointer must travel inwards before letting go opens the panel, in px.
    pub threshold: f32,
}

impl DragOpen {
    /// How far a drag has travelled *away from the bar*, from a press at `from` to a pointer now at `to`.
    /// Negative is back towards the bar, which is the direction that closes rather than opens.
    fn travel(&self, from: (f32, f32), to: (f32, f32)) -> f32 {
        match self.edge {
            Edge::Top => to.1 - from.1,
            Edge::Bottom => from.1 - to.1,
            Edge::Left => to.0 - from.0,
            Edge::Right => from.0 - to.0,
        }
    }
}

/// The base container every simple module sits in: a rounded, pressable box with hover/press feedback.
/// `Filled` overrides the resting background with a solid accent.
pub fn module_shell(
    content: Box<dyn LayoutItem>,
    style: ChipStyle,
    on_press: Option<Box<dyn Fn()>>,
    on_scroll: Option<fn(f32, f32)>,
    drag_open: Option<DragOpen>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ChipStyle {
        variant,
        rest,
        accent,
        theme,
        radius,
        square,
    } = style;
    let (base, hover, active) = match variant {
        Variant::Default => (rest, theme.overlay, theme.overlay.darken(0.14)),
        Variant::Filled => (accent, accent.darken(0.08), accent.darken(0.16)),
    };
    let style = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        // Excess modules overflow the bar rather than every chip being compressed into an unreadable sliver.
        .flex_shrink(0.0);
    // An icon module is a square chip: it stretches to the bar's thickness, and symmetric padding around a bar-proportional icon (see `icon_px`) makes the other side match.
    let style = if square {
        style.padding_all(chip_pad())
    } else {
        style.padding_horizontal(8.0).padding_vertical(2.0)
    };
    let mut shell = StyledContainer::new(style, move |_r| RectStyle::filled(base, radius), vec![content])?
        .on_hover_style(move |_r| RectStyle::filled(hover, radius))
        .on_active_style(move |_r| RectStyle::filled(active, radius));
    if let Some(cb) = on_press {
        shell = shell.on_press(cb);
    }
    if let Some(cb) = on_scroll {
        shell = shell.on_scroll(cb);
    }
    // On the same box as the tap, not on a wrapper around it: a child hit-tests first, so a drag registered outside the pressable chip would never arm — the chip would swallow the press. The two gestures coexist because the tap cancels itself once the pointer travels past the slop, which is exactly the point a drag becomes a drag.
    if let Some(drag) = drag_open {
        let start: Rc<RefCell<Option<(f32, f32)>>> = Rc::new(RefCell::new(None));
        let began = Rc::clone(&start);
        shell = shell
            .on_drag(move |x, y| {
                began.borrow_mut().get_or_insert((x, y));
            })
            .on_drag_end(move |x, y| {
                let from = start.borrow_mut().take().unwrap_or((x, y));
                if drag.travel(from, (x, y)) >= drag.threshold {
                    crate::modules::panel::open_panel(&drag.module);
                }
            });
    }
    Ok(Box::new(shell))
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
    /// If true, the bar places the module bare instead of wrapping it in [`module_shell`] (e.g. the workspaces grid).
    pub self_managed: bool,
    /// If true, the container is a square chip that scales with the bar instead of a content-width text pill.
    pub icon: bool,
    /// What clicking the module does; `None` is a display-only chip.
    pub click: Option<ModuleClick>,
    /// What the wheel does over the module, as `(dx, dy)` in pixels; `None` leaves the chip inert to scroll.
    pub scroll: Option<fn(f32, f32)>,
    /// Whether resting the pointer on the chip opens its hover popout. Set from
    /// [`popout::has_popout`](crate::modules::popout::has_popout) rather than declared twice, so a module can't
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

    /// Marks every registered module the popout layer has card content for. Driven off that list rather than
    /// declared per module, so the two cannot drift into a chip that opens an empty card.
    pub fn wire_popouts(&mut self) {
        for (id, def) in self.modules.iter_mut() {
            def.popout = crate::modules::popout::has_popout(id);
        }
    }

    pub fn build(
        &self,
        id: &str,
        ctx: &ModuleCtx,
    ) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
        self.modules.get(id).map(|d| (d.builder)(ctx))
    }
}

pub fn default_registry() -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();
    registry.register("clock", ModuleDef::new(|_ctx| crate::clock()).opens());
    registry.register(
        "dashboard",
        ModuleDef::new(|_ctx| crate::modules::dashboard::dashboard_chip())
            .icon()
            .opens(),
    );
    registry.register(
        "workspaces",
        ModuleDef::new(|_ctx| crate::workspaces())
            .self_managed()
            .on_scroll(crate::modules::workspaces::scroll),
    );
    registry.register(
        "activewindow",
        ModuleDef::new(|_ctx| crate::activewindow())
            .on_click(crate::modules::activewindow::focus_active),
    );
    registry.register(
        "logo",
        ModuleDef::new(|_ctx| crate::modules::logo::logo_chip())
            .icon()
            .on_click(|| crate::toggle_panel("session")),
    );
    // A gap has no chip: self-managed so the bar places it bare, without padding, hover or a press state.
    registry.register(
        "spacer",
        ModuleDef::new(|_ctx| crate::modules::spacer::spacer()).self_managed(),
    );
    registry.register(
        "launcher",
        ModuleDef::new(|_ctx| {
            let fg = module_fg();
            crate::icon_view(|| "search".to_string(), move || fg.get(), icon_px())
        })
        .icon()
        .on_click(crate::modules::launcher::toggle),
    );
    registry.register(
        "session",
        ModuleDef::new(|_ctx| crate::modules::session::power_chip())
            .icon()
            .opens(),
    );
    registry.register(
        "mic",
        ModuleDef::new(|_ctx| crate::mic())
            .icon()
            .on_click(crate::modules::osd::mic_action)
            .on_scroll(crate::modules::osd::mic_scroll),
    );
    // Display-only: Hyprland's Lua API exposes no keyboard-layout dispatcher, so there is nothing honest for a
    // click to do. See `hyprland::LAYOUT_SWITCHING_UNSUPPORTED`.
    registry.register("kblayout", ModuleDef::new(|_ctx| crate::kblayout()));
    // Self-managed: it draws its own indicator row, and with `hide_inactive` that row can be empty — a chip
    // shell would leave a padded gap in the bar where nothing is shown.
    registry.register(
        "lockstatus",
        ModuleDef::new(|_ctx| crate::lockstatus()).self_managed(),
    );
    // Self-managed: it draws one pressable box per application, each with its own click, middle-click,
    // right-click and scroll — a single chip shell around the row could carry none of that.
    registry.register("tray", ModuleDef::new(|_ctx| crate::tray()).self_managed());
    // The chip shell but no click: which of several readings would a press act on? Each keeps its standalone module.
    registry.register(
        "statusicons",
        ModuleDef::new(|_ctx| crate::modules::statusicons::cluster()),
    );
    registry.register(
        "media",
        ModuleDef::new(|_ctx| crate::media())
            .on_click(crate::modules::media::toggle)
            .on_scroll(crate::modules::media::scroll),
    );
    registry.register("cpu", ModuleDef::new(|_ctx| crate::cpu()));
    registry.register("gpu", ModuleDef::new(|_ctx| crate::gpu()));
    registry.register("memory", ModuleDef::new(|_ctx| crate::memory()));
    registry.register("temperature", ModuleDef::new(|_ctx| crate::temperature()));
    registry.register("netspeed", ModuleDef::new(|_ctx| crate::netspeed()));
    registry.register(
        "battery",
        ModuleDef::new(|_ctx| crate::battery()).icon().opens(),
    );
    registry.register(
        "network",
        ModuleDef::new(|_ctx| crate::network()).icon().opens(),
    );
    registry.register(
        "bluetooth",
        ModuleDef::new(|_ctx| crate::modules::bluetooth::chip())
            .icon()
            .opens(),
    );
    registry.register(
        "volume",
        ModuleDef::new(|_ctx| crate::volume())
            .icon()
            .on_click(crate::modules::osd::volume_action)
            .on_scroll(crate::modules::osd::volume_scroll),
    );
    registry.register(
        "brightness",
        ModuleDef::new(|_ctx| crate::brightness())
            .icon()
            .on_click(crate::modules::osd::brightness_action)
            .on_scroll(crate::modules::osd::brightness_scroll),
    );
    registry.register(
        "notifications",
        ModuleDef::new(|_ctx| crate::modules::notifications::bell_module()).opens(),
    );
    registry.register(
        "notes",
        ModuleDef::new(|_ctx| crate::notes_chip()).icon().opens(),
    );
    registry.register(
        "settings",
        ModuleDef::new(|_ctx| crate::modules::settings::settings_chip())
            .icon()
            .opens(),
    );
    registry.register(
        "utilities",
        ModuleDef::new(|_ctx| crate::modules::utilities::utilities_chip())
            .icon()
            .opens(),
    );
    registry.register(
        "windowinfo",
        ModuleDef::new(|_ctx| crate::modules::windowinfo::window_chip())
            .icon()
            .opens(),
    );
    // Wired from the one list that knows which modules have card content, so no chip is given a hover target it would open empty.
    registry.wire_popouts();
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_opens_a_panel_only_when_it_pulls_away_from_the_bar() {
        let gesture = |edge| DragOpen {
            module: "clock".to_string(),
            edge,
            threshold: 48.0,
        };
        // "Away from the bar" is a different direction on each edge, and the sign is what decides.
        assert_eq!(gesture(Edge::Top).travel((10.0, 5.0), (10.0, 65.0)), 60.0);
        assert_eq!(gesture(Edge::Bottom).travel((10.0, 65.0), (10.0, 5.0)), 60.0);
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

    #[test]
    fn every_module_that_opens_a_panel_has_one() {
        // `module_panel` falls back to the clock panel with a warning for an unregistered module; a module
        // registered with `.opens()` and no panel would silently show the clock, which is a shipping bug.
        const HAS_PANEL: &[&str] = &[
            "clock",
            "dashboard",
            "battery",
            "bluetooth",
            "network",
            "notifications",
            "notes",
            "settings",
            "session",
            "utilities",
            "windowinfo",
        ];
        let registry = default_registry();
        for id in HAS_PANEL {
            let def = registry.def(id).unwrap_or_else(|| panic!("'{id}' is registered"));
            assert!(
                matches!(def.click, Some(ModuleClick::Panel)),
                "'{id}' should open a panel"
            );
        }
        // The direction that actually catches drift: a module wired with `.opens()` and left out of
        // `module_panel` shows the clock instead, which is a shipping bug rather than a compile error.
        for (id, def) in &registry.modules {
            if matches!(def.click, Some(ModuleClick::Panel)) {
                assert!(
                    HAS_PANEL.contains(&id.as_str()),
                    "'{id}' opens a panel, so it must be routed in `module_panel`"
                );
            }
        }
    }

    #[test]
    fn the_new_bar_modules_are_registered_with_the_right_roles() {
        let r = default_registry();
        assert!(
            r.def("spacer").unwrap().self_managed,
            "a gap gets no chip shell, padding or hover state"
        );
        assert!(
            r.def("activewindow").unwrap().click.is_some(),
            "clicking the title focuses the window it names"
        );
        assert!(
            matches!(r.def("mic").unwrap().click, Some(ModuleClick::Action(_)))
                && r.def("mic").unwrap().scroll.is_some(),
            "the mic chip mutes on click and adjusts on scroll, like the volume chip"
        );
        for id in ["cpu", "memory", "temperature", "netspeed"] {
            assert!(
                r.def(id).unwrap().click.is_none(),
                "'{id}' is a readout, not a control"
            );
        }
        assert!(r.def("logo").unwrap().icon, "the logo is a square icon chip");
    }

    #[test]
    fn registry_flags_reflect_module_roles() {
        let r = default_registry();
        assert!(
            matches!(r.def("clock").unwrap().click, Some(ModuleClick::Panel)),
            "clock opens a panel"
        );
        assert!(
            matches!(r.def("volume").unwrap().click, Some(ModuleClick::Action(_))),
            "volume runs a custom action (mute + OSD)"
        );
        assert!(
            r.def("workspaces").unwrap().self_managed,
            "workspaces manages its own layout"
        );
        let tray = r.def("tray").unwrap();
        assert!(
            tray.self_managed,
            "each tray icon carries its own click, middle-click, right-click and scroll"
        );
        assert!(
            tray.click.is_none() && tray.scroll.is_none(),
            "a single chip-level handler would act on the row, not on the application clicked"
        );
        assert!(
            matches!(r.def("battery").unwrap().click, Some(ModuleClick::Panel)),
            "battery opens its detail panel"
        );
        assert!(
            r.def("network").unwrap().icon
                && matches!(r.def("network").unwrap().click, Some(ModuleClick::Panel)),
            "network is an icon chip that opens its network list"
        );
    }
}
