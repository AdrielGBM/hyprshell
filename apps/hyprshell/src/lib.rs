rsx::rsx_modules!(crate::core::theme::NordTheme);

pub use crate::core::app::BarApp;
pub use crate::core::bar::build_bar;
pub use crate::core::config::{BarConfig, BarsConfig, Config, Corner, Edge, ThemeConfig};
pub use crate::core::drawer::{DrawerApp, toggle_drawer};
pub use crate::core::frame::FrameApp;
pub use crate::core::module::{
    ModuleBuilder, ModuleCtx, ModuleRegistry, SurfaceEnv, bar_edge, bar_is_vertical,
    default_registry, set_surface_env, surface_env,
};
pub use crate::core::theme::NordTheme;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, LayerShellPlatform, enumerate_outputs,
};
use rsx::{App, AppConfig, AppPathsProvider, SurfaceId, run_multi_with_platform};

/// Reservation surfaces are backend-driven and never reach the app factory.
#[derive(Clone, Copy)]
enum SurfaceSpec {
    Bar(Edge),
    Frame,
    Reservation,
}

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

/// exclusive_zone = -1 pins position independent of surface-creation order; vertical bars inset at each end (Invariant 1) to keep corner cells clear.
fn layer_config_for(config: &Config, edge: Edge, output: Option<String>) -> LayerConfig {
    let thickness = config.edge_thickness(edge) as i32;
    let gap = if config.hugs(edge) {
        0
    } else {
        config.shape_for(edge).gap as i32
    };
    // Margin tuple is (top, right, bottom, left).
    let (anchor, surface_size, margin) = match edge {
        Edge::Top => (
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            (0, thickness as u32),
            (gap, gap, 0, gap),
        ),
        Edge::Bottom => (
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            (0, thickness as u32),
            (0, gap, gap, gap),
        ),
        Edge::Left => (
            Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            (thickness as u32, 0),
            (
                config.edge_thickness(Edge::Top) as i32 + gap,
                0,
                config.edge_thickness(Edge::Bottom) as i32 + gap,
                gap,
            ),
        ),
        Edge::Right => (
            Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
            (thickness as u32, 0),
            (
                config.edge_thickness(Edge::Top) as i32 + gap,
                gap,
                config.edge_thickness(Edge::Bottom) as i32 + gap,
                0,
            ),
        ),
    };
    LayerConfig {
        output,
        layer: Layer::Top,
        anchor,
        exclusive_zone: -1,
        size: surface_size,
        margin,
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-{}", edge.as_str()),
        reserve_only: false,
        input_transparent: false,
    }
}

/// Invisible reservation strip on Layer::Bottom: space-only, no need for Top's interactivity; order-independent.
fn reservation_config_for(config: &Config, edge: Edge, output: Option<String>) -> LayerConfig {
    let thickness = config.edge_thickness(edge) as i32;
    let gap = if config.hugs(edge) {
        0
    } else {
        config.shape_for(edge).gap as i32
    };
    let reserve = (thickness + gap) as u32;
    let (anchor, size) = match edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, reserve)),
        Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, reserve)),
        Edge::Left => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (reserve, 0)),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (reserve, 0)),
    };
    LayerConfig {
        output,
        layer: Layer::Bottom,
        anchor,
        exclusive_zone: reserve as i32,
        size,
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-reserve-{}", edge.as_str()),
        reserve_only: true,
        input_transparent: true,
    }
}

/// Full-screen frame on Layer::Background: not on Top since ring visibility depends on window z-order.
fn frame_layer_config(output: Option<String>) -> LayerConfig {
    LayerConfig {
        output,
        layer: Layer::Background,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: -1,
        size: (0, 0),
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-frame"),
        reserve_only: false,
        input_transparent: true,
    }
}

pub fn run() {
    let config_path = Config::default_path();
    let reload = Arc::new(AtomicBool::new(false));
    spawn_config_watcher(config_path.clone(), Arc::clone(&reload));

    loop {
        reload.store(false, Ordering::Relaxed);
        run_once(&config_path, Arc::clone(&reload));
        tracing::info!("hyprshell: reloading config");
    }
}

/// Builds surfaces from current config, runs until reload flag flips (config changed), then returns for rebuild.
fn run_once(config_path: &Path, reload: Arc<AtomicBool>) {
    let config = Arc::new(Config::load_or_default(config_path));

    let outputs = enumerate_outputs();
    if outputs.is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }

    let mut platform = LayerShellPlatform::new();
    let mut surfaces = Vec::new();
    let mut specs: HashMap<SurfaceId, SurfaceSpec> = HashMap::new();
    let mut next_id = 0u64;
    let mut declare = |platform: &mut LayerShellPlatform,
                       surfaces: &mut Vec<(SurfaceId, AppConfig)>,
                       specs: &mut HashMap<SurfaceId, SurfaceSpec>,
                       spec: SurfaceSpec,
                       cfg: LayerConfig| {
        let id = SurfaceId(next_id);
        next_id += 1;
        specs.insert(id, spec);
        surfaces.push((id, AppConfig::default()));
        let taken = std::mem::take(platform);
        *platform = taken.with_surface(id, cfg);
    };
    for out in &outputs {
        for edge in Edge::ALL {
            if config.edge_present(edge) {
                let cfg = layer_config_for(&config, edge, out.name.clone());
                declare(
                    &mut platform,
                    &mut surfaces,
                    &mut specs,
                    SurfaceSpec::Bar(edge),
                    cfg,
                );
                let reserve = reservation_config_for(&config, edge, out.name.clone());
                declare(
                    &mut platform,
                    &mut surfaces,
                    &mut specs,
                    SurfaceSpec::Reservation,
                    reserve,
                );
            }
        }
        if config.shape.frame {
            let cfg = frame_layer_config(out.name.clone());
            declare(
                &mut platform,
                &mut surfaces,
                &mut specs,
                SurfaceSpec::Frame,
                cfg,
            );
        }
    }

    if surfaces.is_empty() {
        eprintln!("hyprshell: every bar is empty — nothing to show");
        std::process::exit(1);
    }
    println!(
        "hyprshell: {} surface(s) across {} output(s)",
        surfaces.len(),
        outputs.len()
    );

    let platform = platform.with_shutdown(reload);
    let config_for_factory = Arc::clone(&config);
    let specs = Arc::new(specs);
    if let Err(e) = run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        move |id| -> Box<dyn App> {
            let config = Arc::clone(&config_for_factory);
            match specs[&id] {
                SurfaceSpec::Bar(edge) => Box::new(BarApp { config, edge }),
                SurfaceSpec::Frame => Box::new(FrameApp { config }),
                // Reservation surfaces are backend-driven (reserve_only) and never build an app.
                SurfaceSpec::Reservation => {
                    unreachable!("reservation surfaces do not reach the app factory")
                }
            }
        },
        "hyprshell",
    ) {
        eprintln!("hyprshell exited with error: {e}");
        std::process::exit(1);
    }
}

/// Polls config.toml mtime on background thread; polling (vs inotify) is dependency-free and naturally debounces.
fn spawn_config_watcher(path: PathBuf, reload: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-config-watch".to_string())
        .spawn(move || {
            let mut last = config_mtime(&path);
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let now = config_mtime(&path);
                if now != last {
                    last = now;
                    if now.is_some() {
                        reload.store(true, Ordering::Relaxed);
                    }
                }
            }
        });
}

fn config_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(toml: &str) -> Config {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn visible_bars_reserve_nothing_and_pin_deterministically() {
        let cfg = config("[bars.top]\ncenter=[\"clock\"]\n[bars.bottom]\nstart=[\"clock\"]\n");
        for edge in [Edge::Top, Edge::Bottom] {
            let lc = layer_config_for(&cfg, edge, None);
            assert_eq!(lc.size, (0, 34), "{edge:?} leaves width free, pins height");
            assert_eq!(lc.exclusive_zone, -1, "visible bar reserves nothing");
            assert!(!lc.reserve_only);
            assert_eq!(lc.margin, (0, 0, 0, 0));
            assert!(lc.anchor.contains(Anchor::LEFT) && lc.anchor.contains(Anchor::RIGHT));
        }
        let top = layer_config_for(&cfg, Edge::Top, None).anchor;
        assert!(top.contains(Anchor::TOP) && !top.contains(Anchor::BOTTOM));
        assert!(
            layer_config_for(&cfg, Edge::Bottom, None)
                .anchor
                .contains(Anchor::BOTTOM)
        );
    }

    #[test]
    fn reservation_strip_carves_thickness_along_full_edge() {
        let cfg = config("[bars.left]\nsize=44\nstart=[\"workspaces\"]\n");
        let r = reservation_config_for(&cfg, Edge::Left, None);
        assert!(r.reserve_only);
        assert!(
            r.input_transparent,
            "click-through so it never swallows the bar's input"
        );
        assert!(
            matches!(r.layer, Layer::Bottom),
            "spacers live below the bars, not on Top"
        );
        assert_eq!(r.exclusive_zone, 44, "reserves the bar thickness");
        assert_eq!(r.size, (44, 0));
        assert_eq!(r.margin, (0, 0, 0, 0));
        assert!(r.anchor.contains(Anchor::TOP) && r.anchor.contains(Anchor::BOTTOM));
    }

    #[test]
    fn floating_bar_gains_outer_and_end_margins_reservation_takes_gap() {
        let cfg = config("[shape]\ngap=8\nradius=12\n[bars.top]\nsize=34\ncenter=[\"clock\"]\n");
        let lc = layer_config_for(&cfg, Edge::Top, None);
        assert_eq!(lc.margin, (8, 8, 0, 8));
        assert_eq!(lc.exclusive_zone, -1);
        let r = reservation_config_for(&cfg, Edge::Top, None);
        assert_eq!(r.exclusive_zone, 34 + 8);
    }

    #[test]
    fn vertical_bar_ends_inset_by_adjacent_bar_thickness() {
        // Invariant 1: a left bar flanked by top+bottom bars is inset at each end by the adjacent bar's thickness (+ gap), keeping the corner cell clear; horizontal bars keep their full span.
        let cfg = config(
            "[bars.top]\nsize=30\ncenter=[\"clock\"]\n\
             [bars.bottom]\nsize=40\nstart=[\"clock\"]\n\
             [bars.left]\nsize=44\nstart=[\"workspaces\"]\n",
        );
        let left = layer_config_for(&cfg, Edge::Left, None);
        assert_eq!(left.margin, (30, 0, 40, 0));
        let top = layer_config_for(&cfg, Edge::Top, None);
        assert_eq!(top.margin, (0, 0, 0, 0));
    }

    #[test]
    fn frame_forces_hug_even_with_gap() {
        let cfg = config("[shape]\nframe=true\ngap=8\n[bars.top]\ncenter=[\"clock\"]\n");
        let lc = layer_config_for(&cfg, Edge::Top, None);
        assert_eq!(lc.margin, (0, 0, 0, 0));
        assert_eq!(lc.exclusive_zone, -1);
        let r = reservation_config_for(&cfg, Edge::Top, None);
        assert_eq!(r.exclusive_zone, 34);
    }
}
