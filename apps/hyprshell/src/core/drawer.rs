use std::cell::RefCell;

use rsx::{
    AlignItems, App, AvailableSpace, Color, Component, Event, EventResult, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, NodeId, PointerButton, Rect, RectStyle, RenderNode,
    RwSignal, SizeDimension, StyledContainer, WindowConfig, compute_layout, mark_dirty,
    new_container, reset_layout_runtime, set_theme, track_layout,
};

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, SurfaceHandle, open_surface, request_close,
};

use crate::core::config::Edge;
use crate::core::module::surface_env;
use crate::core::theme::NordTheme;

const PANEL_W: f32 = 320.0;
const PANEL_H: f32 = 180.0;
/// Gap between the bar and the panel hanging off it.
const PANEL_GAP: f32 = 8.0;

/// The root of a drawer surface: a full-screen transparent scrim that positions its panel against the bar's edge and closes the drawer when a press lands on the scrim (outside the panel).
struct DrawerRoot {
    root: NodeId,
    panel_rect: Option<RwSignal<Rect>>,
    content: Box<dyn LayoutItem>,
}

impl DrawerRoot {
    fn new(panel: Box<dyn LayoutItem>, edge: Edge, bar_size: f32) -> Result<Self, LayoutError> {
        let panel_node = panel.layout_node();
        let inset = bar_size + PANEL_GAP;
        // The panel sits against the bar's edge, centered on the cross axis; the rest of the surface stays transparent scrim.
        let style = LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER);
        let style = match edge {
            Edge::Top => style
                .flex_column()
                .justify_content(JustifyContent::START)
                .padding_top(inset),
            Edge::Bottom => style
                .flex_column()
                .justify_content(JustifyContent::END)
                .padding_bottom(inset),
            Edge::Left => style
                .flex_row()
                .justify_content(JustifyContent::START)
                .padding_left(inset),
            Edge::Right => style
                .flex_row()
                .justify_content(JustifyContent::END)
                .padding_right(inset),
        };
        let root = new_container(style, &[panel_node])?;
        // The panel's laid-out (window-absolute) rect, so a press can be tested against it.
        let panel_rect = track_layout(panel_node);
        Ok(Self {
            root,
            panel_rect,
            content: panel,
        })
    }
}

impl Component for DrawerRoot {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::WindowResized { width, height } => {
                mark_dirty(self.root).ok();
                compute_layout(
                    self.root,
                    AvailableSpace::Definite(*width as f32),
                    AvailableSpace::Definite(*height as f32),
                )
                .ok();
                EventResult::Handled
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                let inside = self
                    .panel_rect
                    .as_ref()
                    .map(|r| r.get().contains(*x as f32, *y as f32))
                    .unwrap_or(false);
                if inside {
                    self.content.on_event(event)
                } else {
                    // A press on the scrim dismisses the drawer.
                    request_close();
                    EventResult::Handled
                }
            }
            _ => self.content.on_event(event),
        }
    }
}

/// A drawer's app: a transparent full-screen surface showing the panel for `module`. Runs on its own surface/thread with an isolated reactive world, so its panel drives its own live state (`interval`) just like a bar module.
pub struct DrawerApp {
    pub module: String,
    pub edge: Edge,
    pub bar_size: u32,
}

impl App for DrawerApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme);
        let panel_content = match self.module.as_str() {
            "clock" => crate::clock_panel(),
            other => {
                tracing::warn!("no drawer panel registered for module '{other}'");
                crate::clock_panel()
            }
        }
        .expect("drawer panel build failed");
        let surface = theme.surface;
        let panel = StyledContainer::new(
            LayoutStyle::new()
                .flex_column()
                .width(PANEL_W)
                .height(PANEL_H)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_all(16.0),
            move |_| RectStyle::filled(surface, 14.0),
            vec![panel_content],
        )
        .expect("drawer panel container failed");
        Box::new(
            DrawerRoot::new(Box::new(panel), self.edge, self.bar_size as f32)
                .expect("drawer layout failed"),
        )
    }

    fn window_config(&self) -> Option<WindowConfig> {
        // The scrim needs a transparent surface so the compositor blends it over the desktop.
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }

    fn clear_color(&self) -> Option<Color> {
        // A dim scrim (~35% black) darkens the desktop behind the panel and catches dismiss clicks.
        Some(Color::from_rgba_u8(0, 0, 0, 90))
    }
}

thread_local! {
    // The drawer currently open from THIS bar surface, if any. Lives on the bar's own thread; dropping the handle closes the drawer's surface.
    static OPEN_DRAWER: RefCell<Option<(String, SurfaceHandle)>> = const { RefCell::new(None) };
}

/// The layer-shell surface for a drawer: a full-screen overlay that catches every click (so a press outside the panel can dismiss it) and reserves no space.
fn drawer_spec(output: Option<String>) -> LayerConfig {
    LayerConfig {
        output,
        layer: Layer::Overlay,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: 0,
        size: (0, 0),
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-drawer"),
    }
}

/// Toggles the drawer for `module_id`: the first click opens its panel in a fresh full-screen surface; clicking the same module again — or opening any other drawer — closes what was open. A drawer that already dismissed itself (a click on its scrim) is treated as closed, so the next module click reopens it.
pub fn toggle_drawer(module_id: &str) {
    let Some(env) = surface_env() else { return };
    OPEN_DRAWER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let already_open = slot
            .as_ref()
            .is_some_and(|(id, handle)| id == module_id && !handle.is_closing());
        *slot = None; // drops the previous handle → closes whatever drawer was open
        if !already_open {
            let handle = open_surface(
                drawer_spec(None),
                DrawerApp {
                    module: module_id.to_string(),
                    edge: env.edge,
                    bar_size: env.bar_size,
                },
            );
            *slot = Some((module_id.to_string(), handle));
        }
    });
}
