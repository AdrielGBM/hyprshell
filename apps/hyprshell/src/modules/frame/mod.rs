use std::sync::Arc;

use rsx::{
    App, Canvas, Color, Component, FillRule, LayoutStyle, PathStyle, RenderNode, ShapeStyle,
    SizeDimension, WindowConfig, reset_layout_runtime, set_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Config, Edge};
use crate::shared::geometry::{InnerEdges, frame_path};
use crate::shared::theme::NordTheme;

/// Per-output frame: full-screen transparent surface drawing a continuous even-odd ring around content.
pub struct FrameApp {
    pub config: Arc<Config>,
}

impl App for FrameApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme);
        let base = theme.base;
        let config = Arc::clone(&self.config);
        let inner_radius = (self.config.shape.radius + self.config.shape.gap) as f32;
        let canvas = Canvas::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            move |r| {
                let inner = InnerEdges {
                    left: config.edge_thickness(Edge::Left) as f32,
                    top: config.edge_thickness(Edge::Top) as f32,
                    right: config.edge_thickness(Edge::Right) as f32,
                    bottom: config.edge_thickness(Edge::Bottom) as f32,
                };
                let path = frame_path((r.width, r.height), inner, inner_radius);
                RenderNode::path(
                    Arc::new(path),
                    PathStyle::default()
                        .with_fill(base)
                        .with_fill_rule(FillRule::EvenOdd),
                )
            },
        )
        .expect("frame canvas build failed");
        Box::new(SurfaceRoot::new(Box::new(canvas)).expect("frame layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }
}
