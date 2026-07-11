rsx::rsx_modules!(crate::core::theme::NordTheme);

pub use crate::core::app::BarApp;
pub use crate::core::bar::build_bar;
pub use crate::core::config::{BarConfig, BarsConfig, Config, Edge, ThemeConfig};
pub use crate::core::drawer::{DrawerApp, toggle_drawer};
pub use crate::core::module::{
    ModuleBuilder, ModuleCtx, ModuleRegistry, SurfaceEnv, bar_edge, bar_is_vertical,
    default_registry, set_surface_env, surface_env,
};
pub use crate::core::theme::NordTheme;

use std::collections::HashMap;
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

/// The layer-shell surface for a bar on `edge`: anchored to that edge and stretched along it, reserving an exclusive zone equal to its thickness so windows never overlap it. Horizontal bars leave width free (`0`) and pin height; vertical bars do the reverse.
fn layer_config_for(edge: Edge, size: u32, output: Option<String>) -> LayerConfig {
    let (anchor, surface_size, exclusive) = match edge {
        Edge::Top => (
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            (0, size),
            size as i32,
        ),
        Edge::Bottom => (
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            (0, size),
            size as i32,
        ),
        Edge::Left => (
            Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            (size, 0),
            size as i32,
        ),
        Edge::Right => (
            Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
            (size, 0),
            size as i32,
        ),
    };
    LayerConfig {
        output,
        layer: Layer::Top,
        anchor,
        exclusive_zone: exclusive,
        size: surface_size,
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-{}", edge.as_str()),
    }
}

pub fn run() {
    let config = Arc::new(Config::load_or_default(&Config::default_path()));

    let outputs = enumerate_outputs();
    if outputs.is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }

    // One surface per (output × configured edge). Empty edges are skipped, so a bar collapses to nothing when it has no modules.
    let mut platform = LayerShellPlatform::new();
    let mut surfaces = Vec::new();
    let mut edges: HashMap<SurfaceId, Edge> = HashMap::new();
    let mut next_id = 0u64;
    for out in &outputs {
        for edge in Edge::ALL {
            let bar = config.bars.get(edge);
            if bar.is_empty() {
                continue;
            }
            let id = SurfaceId(next_id);
            next_id += 1;
            platform =
                platform.with_surface(id, layer_config_for(edge, bar.size, out.name.clone()));
            surfaces.push((id, AppConfig::default()));
            edges.insert(id, edge);
        }
    }

    if surfaces.is_empty() {
        eprintln!("hyprshell: every bar is empty — nothing to show");
        std::process::exit(1);
    }
    println!(
        "hyprshell: {} bar surface(s) across {} output(s)",
        surfaces.len(),
        outputs.len()
    );

    let config_for_factory = Arc::clone(&config);
    let edges = Arc::new(edges);
    if let Err(e) = run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        move |id| BarApp {
            config: Arc::clone(&config_for_factory),
            edge: edges[&id],
        },
        "hyprshell",
    ) {
        eprintln!("hyprshell exited with error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_bars_stretch_width_and_reserve_height() {
        for edge in [Edge::Top, Edge::Bottom] {
            let cfg = layer_config_for(edge, 34, None);
            assert_eq!(cfg.size, (0, 34), "{edge:?} leaves width free, pins height");
            assert_eq!(cfg.exclusive_zone, 34);
            assert!(cfg.anchor.contains(Anchor::LEFT) && cfg.anchor.contains(Anchor::RIGHT));
        }
        let top = layer_config_for(Edge::Top, 34, None).anchor;
        assert!(top.contains(Anchor::TOP) && !top.contains(Anchor::BOTTOM));
        assert!(
            layer_config_for(Edge::Bottom, 34, None)
                .anchor
                .contains(Anchor::BOTTOM)
        );
    }

    #[test]
    fn vertical_bars_stretch_height_and_reserve_width() {
        for edge in [Edge::Left, Edge::Right] {
            let cfg = layer_config_for(edge, 44, None);
            assert_eq!(cfg.size, (44, 0), "{edge:?} pins width, leaves height free");
            assert_eq!(cfg.exclusive_zone, 44);
            assert!(cfg.anchor.contains(Anchor::TOP) && cfg.anchor.contains(Anchor::BOTTOM));
        }
        let left = layer_config_for(Edge::Left, 44, None).anchor;
        assert!(left.contains(Anchor::LEFT) && !left.contains(Anchor::RIGHT));
        assert!(
            layer_config_for(Edge::Right, 44, None)
                .anchor
                .contains(Anchor::RIGHT)
        );
    }
}
