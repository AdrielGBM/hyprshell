//! hyprshell — a Wayland desktop shell built on rsx, rendered through the wlr-layer-shell backend. The bar UI
//! is authored in the `.rsx` DSL (see `src/bar.rsx`); `rsx::rsx_modules!()` transpiles it, and we drive it
//! through our own `Platform` (no winit) via `rsx::run_multi_with_platform`.

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, LayerShellPlatform, enumerate_outputs,
};
use rsx::{
    App, AppConfig, AppPathsProvider, AvailableSpace, Color, Component, Event, EventResult,
    LayoutError, LayoutItem, LayoutStyle, NodeId, RenderNode, SizeDimension, SurfaceId,
    compute_layout, mark_dirty, new_container, reset_layout_runtime, run_multi_with_platform,
};

// Transpiles every `.rsx` under `src/` (here: `bar.rsx` → `bar()`) and declares the modules — no winit runner.
rsx::rsx_modules!();

const BAR_HEIGHT: u32 = 40;

// No-op paths provider: this PoC persists nothing.
struct NullPaths;
impl AppPathsProvider for NullPaths {
    fn config_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
}

// Root component: builds the `.rsx` bar into a full-surface layout root and re-lays-it-out on WindowResized
// (which the layer-shell backend synthesizes from the compositor's configure).
struct BarRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
}

impl BarRoot {
    fn new() -> Result<Self, LayoutError> {
        let content = crate::bar()?;
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

struct BarApp;

impl App for BarApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        Box::new(BarRoot::new().expect("bar layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::from_rgb_u8(30, 30, 46))
    }
}

/// Open one top bar per connected monitor, driven by the `.rsx` UI through the layer-shell backend.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let outputs = enumerate_outputs();
    if outputs.is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }
    println!("hyprshell: {} output(s):", outputs.len());
    for o in &outputs {
        println!("  - {:?} {:?} scale={}", o.name, o.logical_size, o.scale);
    }

    let mut platform = LayerShellPlatform::new();
    let mut surfaces = Vec::with_capacity(outputs.len());
    for (i, out) in outputs.iter().enumerate() {
        let id = SurfaceId(i as u64);
        let config = LayerConfig {
            output: out.name.clone(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: BAR_HEIGHT as i32,
            size: (0, BAR_HEIGHT),
            margin: (0, 0, 0, 0),
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: String::from("hyprshell-bar"),
        };
        platform = platform.with_surface(id, config);
        surfaces.push((id, AppConfig::default()));
    }

    if let Err(e) = run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        |_id| BarApp,
        "hyprshell",
    ) {
        eprintln!("hyprshell exited with error: {e}");
        std::process::exit(1);
    }
}
