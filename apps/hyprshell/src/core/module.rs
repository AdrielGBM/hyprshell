use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use rsx::{Color, LayoutError, LayoutItem};

use crate::core::config::{Config, Edge};
use crate::core::theme::NordTheme;

/// What a module needs to know about the bar it lives on. Set once per surface (by `BarApp::root`) before its modules build. Each surface is one bar on one edge, on its own thread, so a thread-local carries this into the parameterless `.rsx` module entrypoints with no prop plumbing — the same seam `set_theme`/`use_theme` use.
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

/// True when the current surface's bar runs vertically (left/right edge), so linear modules should stack their content in a column instead of a row. Read from `.rsx` modules.
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

pub type ModuleBuilder = fn(&ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError>;

#[derive(Default)]
pub struct ModuleRegistry {
    builders: HashMap<String, ModuleBuilder>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &str, builder: ModuleBuilder) {
        self.builders.insert(id.to_string(), builder);
    }

    pub fn build(
        &self,
        id: &str,
        ctx: &ModuleCtx,
    ) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
        self.builders.get(id).map(|b| b(ctx))
    }
}

pub fn default_registry() -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();
    registry.register("clock", |_ctx: &ModuleCtx| crate::clock());
    registry.register("workspaces", |_ctx: &ModuleCtx| crate::workspaces());
    registry
}
