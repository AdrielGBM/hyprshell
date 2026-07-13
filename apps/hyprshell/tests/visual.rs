//! Ad-hoc visual harness: renders a hyprshell surface headless and writes a PNG to `RSX_VISUAL_OUT`, so a
//! bar/module change can be eyeballed instead of only asserted on. There is no Wayland compositor in CI, so
//! this drives `BarApp` through the same rsx pipeline (root → layout → software render → pixels) via the
//! headless platform. Run with:
//!   RSX_VISUAL_OUT=/tmp/bar.png HYPRSHELL_VISUAL_EDGE=top cargo test -p hyprshell --test visual -- --nocapture
//!
//! Optional env:
//!   HYPRSHELL_VISUAL_CONFIG  path to a config.toml (else a built-in demo config)
//!   HYPRSHELL_VISUAL_EDGE    top|bottom|left|right (default top)
//!   HYPRSHELL_VISUAL_SIZE    "WxH" of the surface in px (default derived from the edge)

use std::sync::{Arc, Mutex};

use hyprshell::{BarApp, Config, Edge, OsdApp, OsdKind};
use platform_headless::{FrameSink, HeadlessPlatform};
use rsx::{App, AppConfig, AppPathsProvider, run_with_platform};

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

const DEMO: &str = r#"
[shape]
mode = "chips"
gap = 8
spacing = 8
radius = 12

[bars.top]
size = 40
start = ["workspaces"]
center = ["clock"]
end = ["clock"]
"#;

fn edge_from_env() -> Edge {
    match std::env::var("HYPRSHELL_VISUAL_EDGE").as_deref() {
        Ok("bottom") => Edge::Bottom,
        Ok("left") => Edge::Left,
        Ok("right") => Edge::Right,
        _ => Edge::Top,
    }
}

fn size_for(edge: Edge, config: &Config) -> (u32, u32) {
    if let Ok(s) = std::env::var("HYPRSHELL_VISUAL_SIZE") {
        if let Some((w, h)) = s.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                return (w, h);
            }
        }
    }
    let thickness = config.bars.get(edge).size;
    if edge.is_horizontal() {
        (1280, thickness)
    } else {
        (thickness, 800)
    }
}

fn render_png<A: App + 'static>(app: A, w: u32, h: u32, out: &str) {
    let sink: FrameSink = Arc::new(Mutex::new(None));
    let platform = HeadlessPlatform::new(w, h)
        .with_frames(2)
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

#[test]
fn visual_bar_png() {
    let Ok(out) = std::env::var("RSX_VISUAL_OUT") else {
        eprintln!("set RSX_VISUAL_OUT to write a PNG; skipping");
        return;
    };

    let toml = std::env::var("HYPRSHELL_VISUAL_CONFIG")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| DEMO.to_string());
    let config: Config = toml::from_str(&toml).expect("config parses");
    let edge = edge_from_env();
    let (w, h) = size_for(edge, &config);
    render_png(
        BarApp {
            config: Arc::new(config),
            edge,
        },
        w,
        h,
        &out,
    );
}

/// Renders the volume OSD surface (§6). Gated on its own env var so it doesn't collide with the bar test.
#[test]
fn visual_osd_png() {
    let Ok(out) = std::env::var("RSX_VISUAL_OSD_OUT") else {
        eprintln!("set RSX_VISUAL_OSD_OUT to render the OSD; skipping");
        return;
    };
    let kind = match std::env::var("HYPRSHELL_VISUAL_OSD_KIND").as_deref() {
        Ok("brightness") => OsdKind::Brightness,
        _ => OsdKind::Volume,
    };
    render_png(
        OsdApp {
            kind,
            accent: hyprshell::NordTheme::new().accent_by_name("teal"),
        },
        280,
        60,
        &out,
    );
}
