use telar::{
    AvailableSpace, Component, Event, EventResult, LayoutError, LayoutItem, LayoutStyle, NodeId,
    RenderNode, SizeDimension, compute_layout, mark_dirty, new_container,
};

/// Root component: full-surface container that re-layouts on WindowResized and forwards events, so widgets resolve correctly.
pub struct SurfaceRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
}

impl SurfaceRoot {
    pub fn new(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
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
        // Forward events to the tree so module handlers fire — root is the sole entry point for dispatch.
        self.content.on_event(event)
    }
}
