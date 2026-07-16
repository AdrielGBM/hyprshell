use std::sync::Arc;

use rsx::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutError, LayoutItem,
    LayoutStyle, NodeId, RenderNode, SizeDimension, WindowConfig, compute_layout, mark_dirty,
    new_container, reset_layout_runtime, set_theme,
};

use crate::modules::bar::build_bar;
use crate::core::config::{Config, Edge};
use crate::shared::module::{SurfaceEnv, default_registry, set_surface_env};
use crate::shared::theme::NordTheme;

/// Root component: full-surface container that re-layouts on WindowResized and forwards events, so widgets resolve correctly.
pub(crate) struct SurfaceRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
}

impl SurfaceRoot {
    pub(crate) fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let root = new_container(
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[content.layout_node()],
        )?;
        Ok(Self { root, content })
    }
}

impl Component for SurfaceRoot {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            mark_dirty(self.root).ok();
            compute_layout(
                self.root,
                AvailableSpace::Definite(*width as f32),
                AvailableSpace::Definite(*height as f32),
            )
            .ok();
            return EventResult::Handled;
        }
        // Forward events to bar tree so module handlers fire — root is the sole entry point for dispatch.
        self.content.on_event(event)
    }
}

pub struct BarApp {
    pub config: Arc<Config>,
    pub edge: Edge,
}

impl App for BarApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = NordTheme::new().with_accent(&self.config.theme.accent);
        set_theme(theme);
        let bar_config = self.config.bars.get(self.edge);
        // Thread-local context for .rsx modules to read orientation and bar config.
        set_surface_env(SurfaceEnv {
            edge: self.edge,
            bar_size: bar_config.size,
            config: Arc::clone(&self.config),
        });
        let accent = theme.accent;
        let registry = default_registry();
        let bar = build_bar(&self.config, self.edge, accent, &registry, theme)
            .expect("bar build failed");
        Box::new(SurfaceRoot::new(bar).expect("bar layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        // Opaque bar fills entire surface; floating/sections/chips bar has gaps so surface must be transparent.
        if self.config.bar_surface_opaque(self.edge) {
            Some(NordTheme::new().base)
        } else {
            None
        }
    }

    fn window_config(&self) -> Option<WindowConfig> {
        if self.config.bar_surface_opaque(self.edge) {
            None
        } else {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
    }
}
