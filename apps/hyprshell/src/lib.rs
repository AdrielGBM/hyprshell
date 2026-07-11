rsx::rsx_modules!(crate::core::theme::NordTheme);

pub use crate::core::app::BarApp;
pub use crate::core::bar::build_bar;
pub use crate::core::config::{BarConfig, Config, ThemeConfig};
pub use crate::core::module::{ModuleBuilder, ModuleCtx, ModuleRegistry, default_registry};
pub use crate::core::theme::NordTheme;

use std::sync::Arc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, LayerShellPlatform, enumerate_outputs,
};
use rsx::{AppConfig, AppPathsProvider, SurfaceId, run_multi_with_platform};

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

pub fn run() {
    let config = Arc::new(Config::load_or_default(&Config::default_path()));

    let outputs = enumerate_outputs();
    if outputs.is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }
    println!("hyprshell: {} output(s)", outputs.len());

    let mut platform = LayerShellPlatform::new();
    let mut surfaces = Vec::with_capacity(outputs.len());
    for (i, out) in outputs.iter().enumerate() {
        let id = SurfaceId(i as u64);
        let layer = LayerConfig {
            output: out.name.clone(),
            layer: Layer::Top,
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: config.bar.height as i32,
            size: (0, config.bar.height),
            margin: (0, 0, 0, 0),
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: String::from("hyprshell-bar"),
        };
        platform = platform.with_surface(id, layer);
        surfaces.push((id, AppConfig::default()));
    }

    let config_for_factory = Arc::clone(&config);
    if let Err(e) = run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        move |_id| BarApp {
            config: Arc::clone(&config_for_factory),
        },
        "hyprshell",
    ) {
        eprintln!("hyprshell exited with error: {e}");
        std::process::exit(1);
    }
}
