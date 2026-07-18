use std::path::Path;
use std::sync::Arc;

use rsx::{
    App, Color, Component, Container, Image, ImageData, ImageFilter, LayoutItem, LayoutStyle,
    ObjectFit, SizeDimension, WindowConfig, reset_layout_runtime, set_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::Config;
use crate::shared::paths::expand_tilde;

/// Per-output wallpaper: a full-screen background surface painting the configured image (cover-cropped, aspect preserved) over the theme's base colour, or just the base colour when no image resolves.
pub struct WallpaperApp {
    pub config: Arc<Config>,
    /// The monitor this wallpaper covers, so a `[background.monitors]` entry can target it.
    pub output: Option<String>,
}

impl App for WallpaperApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        set_theme(self.config.resolve_theme());
        let content = self.image_content().unwrap_or_else(fill);
        Box::new(SurfaceRoot::new(content).expect("wallpaper layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        // The base colour shows before/without an image (and behind any transparency), covering "theme base colour when there is no image".
        Some(self.config.resolve_theme().base)
    }

    fn window_config(&self) -> Option<WindowConfig> {
        // Opaque: a wallpaper replaces whatever the compositor draws behind it.
        None
    }
}

impl WallpaperApp {
    fn image_content(&self) -> Option<Box<dyn LayoutItem>> {
        let path = expand_tilde(self.config.background.image_for(self.output.as_deref())?);
        let Some(data) = load_image(&path) else {
            tracing::warn!(
                "wallpaper '{}' could not be loaded; using the theme base colour",
                path.display()
            );
            return None;
        };
        let data = Arc::new(data);
        let image = Image::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            move || data.clone(),
            || ImageFilter::Linear,
            || ObjectFit::Cover,
        )
        .ok()?;
        Some(Box::new(image))
    }
}

/// A full-surface empty box, letting `clear_color` (the theme base) show through.
fn fill() -> Box<dyn LayoutItem> {
    Box::new(
        Container::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            Vec::new(),
        )
        .expect("wallpaper fill container"),
    )
}

/// Decodes an image file into premultiplied RGBA, or `None` when the path is missing or the format is unsupported.
fn load_image(path: &Path) -> Option<ImageData> {
    let rgba = image::open(path).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(ImageData::new(rgba.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::core::config::Config;

    /// Renders the wallpaper surface end-to-end (real decode + cover-crop). Point it at an image to eyeball the crop:
    /// `RSX_VISUAL_WALLPAPER_OUT=/tmp/w.png RSX_VISUAL_WALLPAPER_IMG=/path/to/wall.png cargo test -p hyprshell --lib visual_wallpaper -- --nocapture`.
    #[test]
    fn visual_wallpaper_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_WALLPAPER_OUT") else {
            eprintln!("set RSX_VISUAL_WALLPAPER_OUT to render the wallpaper; skipping");
            return;
        };
        let mut config = Config::starter();
        config.background.enabled = true;
        config.background.image = std::env::var("RSX_VISUAL_WALLPAPER_IMG")
            .ok()
            .map(PathBuf::from);
        crate::test_support::render_png(
            WallpaperApp {
                config: Arc::new(config),
                output: None,
            },
            640,
            400,
            &out,
        );
    }
}
