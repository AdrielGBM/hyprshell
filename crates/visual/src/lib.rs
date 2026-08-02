//! Renders a hyprshell `App` headless and writes a PNG for eyeballing.
//!
//! Its own crate, and a dev-dependency wherever it is used, so the headless platform and the PNG encoder are
//! linked only into test builds — several crates now have something with a look worth checking, and each
//! carrying its own copy of this would be three copies of the frame-pacing rule below.

use std::sync::{Arc, Mutex};

use platform_headless::{FrameSink, HeadlessPlatform};
use telar::{App, AppConfig, AppPathsProvider, run_with_platform};

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

pub fn render_png<A: App + 'static>(app: A, w: u32, h: u32, out: &str) {
    render_png_frames(app, w, h, out, 2);
}

/// Drives `frames` renders before capturing; the headless platform paces at a real 60fps, so ~13 frames covers a 200ms enter animation settling.
pub fn render_png_frames<A: App + 'static>(app: A, w: u32, h: u32, out: &str, frames: u32) {
    let sink: FrameSink = Arc::new(Mutex::new(None));
    let platform = HeadlessPlatform::new(w, h)
        .with_frames(frames)
        .capture_into(sink.clone());
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        app,
        "hyprshell-visual",
    )
    .expect("headless run failed");
    let pixels = sink.lock().unwrap().take().expect("no frame captured");
    let img = image::RgbaImage::from_raw(w, h, pixels).expect("rgba length matches w*h*4");
    img.save(out).expect("write PNG");
    eprintln!("wrote {out} ({w}x{h})");
}
