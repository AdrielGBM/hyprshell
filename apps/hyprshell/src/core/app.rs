use std::sync::Arc;

use telar::{
    App, AvailableSpace, Color, Component, Event, EventResult, LayoutError, LayoutItem,
    LayoutStyle, NodeId, RenderNode, SizeDimension, WindowConfig, compute_layout, mark_dirty,
    new_container, reset_layout_runtime, set_theme,
};

use crate::core::config::Edge;
use crate::core::surfaces::LiveConfig;
use crate::modules::bar::{AutoHide, build_bar};
use crate::shared::module::{SurfaceEnv, default_registry, set_surface_env};

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
    /// Read at every build rather than held: this surface outlives the config it was first drawn from, and a
    /// reload rebuilds it in place from whatever is in here now.
    pub config: LiveConfig,
    pub edge: Edge,
    /// The monitor this bar surface lives on; threaded into `SurfaceEnv` so its panels open on the same screen.
    pub output: Option<String>,
}

impl App for BarApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let config = self.config.get();
        let theme = config.resolve_theme();
        set_theme(theme);
        // Apply the configured UI language on this surface's thread and subscribe it to live language switches.
        crate::shared::services::locale::attach(config.language());
        let bar_config = config.bars.get(self.edge);
        set_surface_env(SurfaceEnv {
            edge: self.edge,
            bar_size: bar_config.size,
            output: self.output.clone(),
            config: Arc::clone(&config),
        });
        let accent = theme.accent;
        let registry = default_registry();
        let bar = build_bar(&config, self.edge, accent, &registry, theme).expect("bar build failed");
        // `persistent = false` moves the surface itself, so the wrapper goes here — around the whole bar, inside the surface root that drives it — rather than around any one zone.
        let bar: Box<dyn LayoutItem> = if config.bar_is_persistent(self.edge) {
            bar
        } else {
            Box::new(AutoHide::new(
                bar,
                &config,
                self.edge,
                crate::core::surfaces::bar_margin_for(&config, self.edge),
            ))
        };
        Box::new(SurfaceRoot::new(bar).expect("bar layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        // Opaque bar fills entire surface; floating/sections/chips bar has gaps so surface must be transparent.
        let config = self.config.get();
        if config.bar_surface_opaque(self.edge) {
            Some(config.resolve_theme().base)
        } else {
            None
        }
    }

    fn window_config(&self) -> Option<WindowConfig> {
        if self.config.get().bar_surface_opaque(self.edge) {
            None
        } else {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
    }
}
