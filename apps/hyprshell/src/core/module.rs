use std::collections::HashMap;

use rsx::{Color, LayoutError, LayoutItem};

use crate::core::theme::NordTheme;

#[derive(Clone, Copy)]
pub struct ModuleCtx {
    pub theme: NordTheme,
    pub accent: Color,
    pub bar_height: u32,
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
