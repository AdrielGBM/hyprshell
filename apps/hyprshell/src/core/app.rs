use std::sync::Arc;

use rsx::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutError, LayoutItem,
    LayoutStyle, NodeId, RenderNode, SizeDimension, compute_layout, mark_dirty, new_container,
    reset_layout_runtime, set_theme,
};

use crate::core::bar::build_bar;
use crate::core::config::Config;
use crate::core::module::default_registry;
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
        EventResult::Ignored
    }
}

pub struct BarApp {
    pub config: Arc<Config>,
}

impl App for BarApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme); // so rsx's built-in components resolve Nord tokens
        let accent = theme.accent_by_name(&self.config.theme.accent);
        let registry = default_registry();
        let bar = build_bar(&self.config.bar, accent, &registry, theme).expect("bar build failed");
        Box::new(BarRoot::new(bar).expect("bar layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(NordTheme::new().base)
    }
}
