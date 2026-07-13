use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rsx::{
    AlignItems, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle, ReadSignal, RectStyle,
    StyledContainer, signal,
};

use crate::core::config::{Config, Edge, Variant};
use crate::core::theme::NordTheme;

/// What a module needs to know about its bar. A thread-local carries it into the parameterless `.rsx` module
/// entrypoints with no prop plumbing (each surface is one bar on its own thread) — the `set_theme` seam.
#[derive(Clone)]
pub struct SurfaceEnv {
    pub edge: Edge,
    /// The bar's thickness in px (height for top/bottom, width for left/right).
    pub bar_size: u32,
    pub config: Arc<Config>,
}

thread_local! {
    static SURFACE_ENV: RefCell<Option<SurfaceEnv>> = const { RefCell::new(None) };
}

pub fn set_surface_env(env: SurfaceEnv) {
    SURFACE_ENV.with(|e| *e.borrow_mut() = Some(env));
}

pub fn surface_env() -> Option<SurfaceEnv> {
    SURFACE_ENV.with(|e| e.borrow().clone())
}

pub fn bar_edge() -> Edge {
    surface_env().map(|e| e.edge).unwrap_or(Edge::Top)
}

/// True when the bar runs vertically (left/right), so linear modules stack in a column. Read from `.rsx`.
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
    // The foreground color the CURRENT module's content should paint with. The bar sets it (per module,
    // via `set_module_fg`) just before building that module's content, so `module_fg()` snapshots the
    // right value into a per-module signal. A single mutable slot works because each module reads it once,
    // at build time, into its own signal — never at render time (where it would be the last-set value).
    static MODULE_FG: RefCell<Color> = RefCell::new(NordTheme::new().text);
}

pub fn set_module_fg(color: Color) {
    MODULE_FG.with(|c| *c.borrow_mut() = color);
}

/// A fresh signal snapshot of the current module foreground, for a `.rsx` module to bind as `color:$fg`.
/// Called once at build time so each module captures its OWN color, not the last-set one.
pub fn module_fg() -> ReadSignal<Color> {
    signal(MODULE_FG.with(|c| *c.borrow())).read_only()
}

/// The bar's thickness in px (height for top/bottom, width for left/right), or a sane default outside a
/// surface. Everything a module sizes derives from this, so a thin bar yields a small, proportional chip
/// instead of an oversized one that squashes. Read by `.rsx` modules (e.g. the workspaces square chip).
pub fn bar_thickness() -> f32 {
    surface_env().map(|e| e.bar_size).unwrap_or(34) as f32
}

/// Icon size for the current bar: ~0.75 of its thickness, so the glyph fills most of its square chip and
/// scales with the bar. Read by icon-only `.rsx` modules as `width:icon_px()`.
pub fn icon_px() -> f32 {
    (bar_thickness() * 0.75).round().clamp(8.0, 64.0)
}

/// Padding around an icon inside its square chip. Chosen so the chip's width (icon ≈ 0.75·thickness + two
/// of these ≈ 0.25·thickness) equals the bar thickness — so a chip that stretches to the bar's height comes
/// out square, and stays the same height as a text pill beside it.
fn chip_pad() -> f32 {
    (bar_thickness() * 0.125).round().max(1.0)
}

/// The foreground a module should paint with for a given container variant: the plain text token when the
/// module blends into the bar (default), or the higher-contrast of text/base over the accent when filled.
pub fn module_foreground(variant: Variant, accent: Color, theme: NordTheme) -> Color {
    match variant {
        Variant::Default => theme.text,
        Variant::Filled => accent.most_readable(&[theme.text, theme.base]),
    }
}

/// The base container every simple module sits in: a rounded, pressable box with hover/press feedback.
/// `rest` is its resting background (transparent when blending into a shared surface, the surface token when a
/// free-standing chip); `Filled` overrides with a solid accent. Self-managed modules opt out and are bare.
pub fn module_shell(
    content: Box<dyn LayoutItem>,
    variant: Variant,
    rest: Color,
    accent: Color,
    theme: NordTheme,
    radius: f32,
    square: bool,
    on_press: Option<Box<dyn Fn()>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (base, hover, active) = match variant {
        Variant::Default => (rest, theme.overlay, theme.overlay.darken(0.14)),
        Variant::Filled => (accent, accent.darken(0.08), accent.darken(0.16)),
    };
    let style = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        // A module keeps its natural size instead of squishing its content when a zone is overpacked; excess
        // modules overflow the bar rather than compressing every chip into an unreadable sliver.
        .flex_shrink(0.0);
    // An icon module is a square chip: it stretches to the bar's thickness like a text pill (so both are the
    // same height), and its symmetric padding around a bar-proportional icon (see `icon_px`) makes the other
    // side match — a square that scales with the bar in both orientations. A text pill keeps content width.
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
    Ok(Box::new(shell))
}

pub type ModuleBuilder = fn(&ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError>;

/// What clicking a module does: `Drawer` toggles its drawer panel; `Action` runs a custom handler (e.g.
/// volume toggling mute and popping an OSD).
#[derive(Clone, Copy)]
pub enum ModuleClick {
    Drawer,
    Action(fn()),
}

pub struct ModuleDef {
    pub builder: ModuleBuilder,
    /// The module manages its own layout and interaction (e.g. the workspaces grid), so the bar places it
    /// bare instead of wrapping it in [`module_shell`].
    pub self_managed: bool,
    /// The module is a single icon, so its container is a square chip that scales with the bar (vs a
    /// content-width text pill).
    pub icon: bool,
    /// What clicking the module does; `None` is a display-only chip.
    pub click: Option<ModuleClick>,
}

impl ModuleDef {
    pub fn new(builder: ModuleBuilder) -> Self {
        Self {
            builder,
            self_managed: false,
            icon: false,
            click: None,
        }
    }

    pub fn icon(mut self) -> Self {
        self.icon = true;
        self
    }

    pub fn opens(mut self) -> Self {
        self.click = Some(ModuleClick::Drawer);
        self
    }

    pub fn on_click(mut self, action: fn()) -> Self {
        self.click = Some(ModuleClick::Action(action));
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

    pub fn build(
        &self,
        id: &str,
        ctx: &ModuleCtx,
    ) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
        self.modules.get(id).map(|d| (d.builder)(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn registry_flags_reflect_module_roles() {
        let r = default_registry();
        assert!(
            matches!(r.def("clock").unwrap().click, Some(ModuleClick::Drawer)),
            "clock opens a drawer"
        );
        assert!(
            matches!(r.def("volume").unwrap().click, Some(ModuleClick::Action(_))),
            "volume runs a custom action (mute + OSD)"
        );
        assert!(
            r.def("workspaces").unwrap().self_managed,
            "workspaces manages its own layout"
        );
        assert!(
            r.def("battery").unwrap().click.is_none(),
            "battery is a display-only chip"
        );
    }
}

pub fn default_registry() -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();
    registry.register("clock", ModuleDef::new(|_ctx| crate::clock()).opens());
    registry.register(
        "workspaces",
        ModuleDef::new(|_ctx| crate::workspaces()).self_managed(),
    );
    registry.register("battery", ModuleDef::new(|_ctx| crate::battery()).icon());
    registry.register(
        "volume",
        ModuleDef::new(|_ctx| crate::volume())
            .icon()
            .on_click(crate::core::osd::volume_action),
    );
    registry.register(
        "brightness",
        ModuleDef::new(|_ctx| crate::brightness())
            .icon()
            .on_click(crate::core::osd::brightness_action),
    );
    registry
}
