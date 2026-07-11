use std::sync::Arc;

use rsx::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutError, LayoutItem,
    LayoutStyle, NodeId, RenderNode, SizeDimension, compute_layout, mark_dirty, new_container,
    reset_layout_runtime, set_theme,
};

use crate::core::bar::build_bar;
use crate::core::config::{Config, Edge};
use crate::core::module::{SurfaceEnv, default_registry, set_surface_env};
use crate::core::theme::NordTheme;

struct BarRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
}

impl BarRoot {
    fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
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

impl Component for BarRoot {
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
        // Everything else (pointer press/move/release) must reach the bar tree so module `on_press` handlers fire — the root component is the sole entry point the runner dispatches events to.
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
        let theme = NordTheme::new();
        set_theme(theme); // so rsx's built-in components resolve Nord tokens
        let bar_config = self.config.bars.get(self.edge);
        // Orientation + drawer seam for the `.rsx` modules built below (they read this thread-local).
        set_surface_env(SurfaceEnv {
            edge: self.edge,
            bar_size: bar_config.size,
            config: Arc::clone(&self.config),
        });
        let accent = theme.accent_by_name(&self.config.theme.accent);
        let registry = default_registry();
        let bar = build_bar(self.edge, bar_config, accent, &registry, theme)
            .expect("bar build failed");
        Box::new(BarRoot::new(bar).expect("bar layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(NordTheme::new().base)
    }
}
