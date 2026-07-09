//! A wlr-layer-shell [`rsx::MultiSurfacePlatform`] backend: renders rsx trees into `zwlr_layer_surface_v1`
//! surfaces (bars, panels, OSDs, backgrounds) on a Wayland compositor, one per output, each on its own thread
//! with its own wayland connection (so each surface gets a fully isolated reactive/theme/overlay world).
//!
//! Consume it through rsx's bring-your-own-platform seam:
//! `rsx::run_multi_with_platform(LayerShellPlatform::new().with_surface(id, cfg), surfaces, paths, apps, name)`.

mod config;
mod platform;
mod window;

pub use config::{Anchor, KeyboardInteractivity, Layer, LayerConfig, OutputDescriptor};
pub use platform::{LayerShellPlatform, enumerate_outputs};
pub use window::LayerWindow;
