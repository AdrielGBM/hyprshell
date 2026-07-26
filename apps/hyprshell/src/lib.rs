rsx::rsx_modules!(crate::shared::theme::NordTheme);

/// Renders a hyprshell `App` headless and writes a PNG for eyeballing; inlined here (not a `src/*.rs` file) so the auto-module scan doesn't pull its dev-only deps (`platform-headless`, `image`) into non-test builds.
#[cfg(test)]
mod test_support {
    use std::sync::{Arc, Mutex};

    use platform_headless::{FrameSink, HeadlessPlatform};
    use rsx::{App, AppConfig, AppPathsProvider, run_with_platform};

    pub(crate) struct NullPaths;

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

    pub(crate) fn render_png<A: App + 'static>(app: A, w: u32, h: u32, out: &str) {
        render_png_frames(app, w, h, out, 2);
    }

    /// Drives `frames` renders before capturing; the headless platform paces at a real 60fps, so ~13 frames covers a 200ms enter animation settling.
    pub(crate) fn render_png_frames<A: App + 'static>(
        app: A,
        w: u32,
        h: u32,
        out: &str,
        frames: u32,
    ) {
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
}

pub use crate::core::app::BarApp;
pub use crate::core::config::{
    BarConfig, BarsConfig, Capitalize, Config, Corner, DrawerConfig, Edge, FloatConfig,
    ModuleOverride, OpenMode, PanelsConfig, ThemeConfig, Variant,
};
pub use crate::core::ipc::{
    call as ipc_call, describe as ipc_describe, dispatch as ipc_dispatch, socket_path,
};
pub use crate::modules::bar::build_bar;
pub use crate::modules::frame::FrameApp;
pub use crate::modules::notes::{notes_chip, notes_panel};
pub use crate::modules::osd::OsdKind;
pub use crate::modules::panel::{close_panel, is_panel_open, open_panel, toggle_panel};
pub use crate::modules::wallpaper::WallpaperApp;
pub use crate::shared::icon::{icon_picker_overlay, icon_view};
pub use crate::shared::module::{
    ModuleBuilder, ModuleCtx, ModuleDef, ModuleRegistry, SurfaceEnv, bar_edge, bar_is_vertical,
    bar_thickness, chip_radius, default_registry, icon_px, module_fg, module_foreground,
    module_shell, set_module_fg, set_surface_env, surface_env,
};
pub use crate::shared::theme::{BUILT_IN_THEMES, FontRole, NordTheme, ThemeMeta};

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, LayerShellPlatform, SurfaceHandle,
};
use rsx::{App, AppPathsProvider, run_multi_with_platform};

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

/// Insets past the perpendicular bar's own gap+thickness (not the vertical bar's gap) so a floating perpendicular bar can't overlap a hugging vertical one.
fn perpendicular_inset(config: &Config, perp: Edge, own_gap: i32) -> i32 {
    if config.edge_present(perp) {
        config.edge_reserved(perp) as i32
    } else {
        own_gap
    }
}

/// The layer the shell's own chrome sits on: `Overlay` keeps the bars above a fullscreen window when
/// `[general] show_over_fullscreen` asks for it, `Top` (the default) lets fullscreen cover them.
fn chrome_layer(config: &Config) -> Layer {
    if config.general.show_over_fullscreen {
        Layer::Overlay
    } else {
        Layer::Top
    }
}

/// exclusive_zone = -1 pins position independent of surface-creation order; vertical bars inset at each end (Invariant 1) to keep corner cells clear.
fn layer_config_for(config: &Config, edge: Edge, output: Option<String>) -> LayerConfig {
    let thickness = config.edge_thickness(edge) as i32;
    let gap = config.edge_gap(edge) as i32;
    let top_inset = perpendicular_inset(config, Edge::Top, gap);
    let bottom_inset = perpendicular_inset(config, Edge::Bottom, gap);
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
            (top_inset, 0, bottom_inset, gap),
        ),
        Edge::Right => (
            Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
            (thickness as u32, 0),
            (top_inset, gap, bottom_inset, 0),
        ),
    };
    LayerConfig {
        output,
        layer: chrome_layer(config),
        anchor,
        exclusive_zone: -1,
        size: surface_size,
        margin,
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: format!("hyprshell-{}", edge.as_str()),
        reserve_only: false,
        input_transparent: false,
        interactive_input_region: false,
    }
}

/// Invisible reservation strip on Layer::Bottom: space-only, no need for Top's interactivity; order-independent.
fn reservation_config_for(config: &Config, edge: Edge, output: Option<String>) -> LayerConfig {
    let reserve = config.edge_reserved(edge);
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
        interactive_input_region: false,
    }
}

/// Full-screen wallpaper on Layer::Background: click-through, spans the whole output (exclusive_zone -1 ignores bar reservations). Declared before the bars/frame so it stacks at the bottom of the background layer.
fn wallpaper_layer_config(output: Option<String>) -> LayerConfig {
    LayerConfig {
        output,
        layer: Layer::Background,
        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: -1,
        size: (0, 0),
        margin: (0, 0, 0, 0),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: String::from("hyprshell-wallpaper"),
        reserve_only: false,
        input_transparent: true,
        interactive_input_region: false,
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
        interactive_input_region: false,
    }
}

pub fn run() {
    // One shell per compositor instance: a second one would fight over the notification bus name and the IPC
    // socket, and the user would see two of every bar. Checked before anything is opened so the failure is a
    // clean message rather than a half-started shell.
    if crate::core::ipc::another_instance_is_running() {
        eprintln!(
            "hyprshell: already running (IPC socket {} is live). Use `hyprshell shell quit` to stop it.",
            crate::core::ipc::socket_path().display()
        );
        std::process::exit(1);
    }
    let config_path = Config::default_path();
    // Start the notification daemon once, before the reload loop, so it keeps owning the D-Bus name across bar
    // config reloads (§8 "persists across reloads"). The popup surface itself is (re)established inside
    // `run_once` on the driver thread — notification state lives in the daemon, so recreating it loses nothing.
    let initial = Arc::new(Config::load_or_default(&config_path));
    // Seed the shared UI-language source so every surface starts in the configured locale and stays live.
    crate::shared::services::locale::init(initial.language());
    // Process-wide so every surface — bars, drawers, popups, OSD — renders in the theme's font family.
    // `run_once` re-applies (and warns) on every reload, so the popup host spawned here also gets it.
    rsx::set_default_font_family(initial.theme.font_family.clone());
    crate::shared::services::notifications::init(
        Duration::from_millis(initial.notifications.timeout_ms),
        initial.notifications.critical_sticky,
    );

    // Non-destructive reload: one persistent driver. Every surface is opened dynamically on the driver thread
    // (via `setup_shell`, deferred with `run_on_start`) and reconciled on config change, so a reload never tears
    // down the connection, the popup, or the shared services — only the bars/wallpaper/frame that changed.
    platform_layershell::run_on_start(move || setup_shell(config_path));
    if let Err(e) = run_multi_with_platform(
        LayerShellPlatform::new(),
        Vec::new(),
        |_| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        |_id| -> Box<dyn App> { unreachable!("hyprshell opens every surface dynamically") },
        "hyprshell",
    ) {
        eprintln!("hyprshell exited with error: {e}");
        std::process::exit(1);
    }
}

/// Runs on the driver thread once its loop is up (deferred via `run_on_start`): brings up the popup host and
/// opens every surface, then watches the config file and reconciles the surface set on change — closing the old
/// surfaces and opening the new ones — without tearing the driver, connection, popup or services down.
fn setup_shell(config_path: PathBuf) {
    let config = Arc::new(Config::load_or_default(&config_path));
    apply_config(&config);
    if platform_layershell::outputs().is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }

    // The popup host is long-lived: set up once, it persists across reloads (its edge/radius are read at
    // startup; notification state lives in the daemon, so it need not be rebuilt on a bar-config change).
    crate::modules::notifications::popup_host(Arc::clone(&config));

    let initial = open_surfaces(&config);
    println!("hyprshell: {} surface(s) up", initial.len());
    let handles = Rc::new(RefCell::new(initial));
    // The config the shell is currently running. A reload that fails to parse keeps this one rather than
    // falling back to the starter bar, so a typo costs the user an error message, not their whole layout.
    let live = Rc::new(RefCell::new(config));

    // One reconciliation, shared by every trigger: re-read the config, close the old surfaces and open the set
    // the current config and monitor layout call for. Driven by a config edit, by a monitor being plugged in or
    // unplugged, and by `hyprshell shell reload` — all of which change which surfaces should exist.
    let reconcile = {
        let handles = Rc::clone(&handles);
        let live = Rc::clone(&live);
        let config_path = config_path.clone();
        move || {
            let config = match Config::load(&config_path) {
                Ok(config) => Arc::new(config),
                Err(e) => {
                    report_config_error(&e);
                    Arc::clone(&live.borrow())
                }
            };
            apply_config(&config);
            // Panels were built against the outgoing config; leaving one up would leave a stale theme and a
            // dangling anchor on screen.
            crate::core::shell::close_all();
            for handle in handles.borrow_mut().drain(..) {
                handle.close();
            }
            *handles.borrow_mut() = open_surfaces(&config);
            *live.borrow_mut() = config;
        }
    };
    let reconcile = Rc::new(reconcile);

    crate::core::shell::set_reload_hook({
        let reconcile = Rc::clone(&reconcile);
        move || reconcile()
    });

    // The command surface. Started after the reload hook so a `shell reload` arriving immediately has something
    // to call, and on the driver thread so handlers can open surfaces exactly as a click handler would.
    platform_layershell::watch(crate::core::ipc::serve, crate::core::ipc::handle);

    let on_config_change = Rc::clone(&reconcile);
    platform_layershell::watch(
        move |tx| watch_config_changes(config_path, tx),
        move |_| on_config_change(),
    );
    platform_layershell::on_outputs_changed(move || reconcile());
}

/// Everything a config change affects outside the surfaces themselves: the UI language, the process-wide font,
/// the icon store, and the context that code reached from outside a surface resolves against.
///
/// Called from the driver thread at app level — deliberately not from inside a surface build, since the icon
/// store's download worker must outlive any single surface (see [`shared::icon::init_store`]).
fn apply_config(config: &Arc<Config>) {
    crate::shared::services::locale::init(config.language());
    warn_if_font_missing(config.theme.font_family.as_deref());
    rsx::set_default_font_family(config.theme.font_family.clone());
    crate::core::shell::set_config(Arc::clone(config));
    crate::shared::icon::init_store(&config.icons);
}

/// Tells the user their edit didn't take, through the shell's own notification daemon so the message lands on
/// screen rather than in a log they aren't reading. Falls back to stderr when the daemon isn't up yet.
fn report_config_error(error: &crate::core::config::LoadError) {
    let message = error.to_string();
    tracing::warn!("{message}; keeping the last working config");
    crate::shared::services::notifications::notify_local(
        "hyprshell",
        &rsx::t!("config.error_title"),
        &message,
    );
}

/// Opens every bar / reservation / wallpaper / frame surface for the current config across all outputs and
/// returns their handles — kept alive to keep the surfaces up; closing a handle tears its surface down.
fn open_surfaces(config: &Arc<Config>) -> Vec<SurfaceHandle> {
    let mut handles = Vec::new();
    for out in platform_layershell::outputs() {
        // Declared first so it stacks at the bottom of the background layer, under the frame and bars.
        if config.background.is_enabled() {
            handles.push(platform_layershell::open_surface(
                wallpaper_layer_config(out.name.clone()),
                WallpaperApp {
                    config: Arc::clone(config),
                    output: out.name.clone(),
                },
            ));
        }
        for edge in Edge::ALL {
            if config.edge_present(edge) {
                handles.push(platform_layershell::open_surface(
                    layer_config_for(config, edge, out.name.clone()),
                    BarApp {
                        config: Arc::clone(config),
                        edge,
                        output: out.name.clone(),
                    },
                ));
                handles.push(platform_layershell::open_reservation(
                    reservation_config_for(config, edge, out.name.clone()),
                ));
            }
        }
        if config.shape.frame {
            handles.push(platform_layershell::open_surface(
                frame_layer_config(out.name.clone()),
                FrameApp {
                    config: Arc::clone(config),
                },
            ));
        }
    }
    handles
}

/// The config-watch producer for `watch`: polls config.toml mtime (dependency-free, naturally debounced) and
/// sends a tick on each change so the driver thread reconciles the surface set.
fn watch_config_changes(path: PathBuf, tx: platform_layershell::EventSender<()>) {
    let mut last = config_mtime(&path);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let now = config_mtime(&path);
        if now != last {
            last = now;
            if now.is_some() && !tx.send(()) {
                return;
            }
        }
    }
}

fn config_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Logs whether a configured `[theme] font_family` resolves against the installed fonts. A wrong family name
/// (e.g. `"Fira Code Nerd Font"` instead of the installed `"FiraCode Nerd Font"`) otherwise falls back to the
/// default font silently; this turns that into a visible log line. The query mirrors the text shaper's own
/// `FontSystem::new()` resolution, so a hit here means the shell will actually render in that family.
///
/// Scanning the system fonts costs hundreds of ms, and every save from the settings panel triggers a config
/// reload, so the database is loaded at most once and each family's verdict is remembered. The cost of that is
/// a font installed while the shell runs isn't picked up until restart — worth it to keep reloads cheap.
fn warn_if_font_missing(family: Option<&str>) {
    let Some(family) = family else { return };
    thread_local! {
        static CHECKED: RefCell<std::collections::HashMap<String, bool>> =
            RefCell::new(std::collections::HashMap::new());
    }
    if CHECKED.with(|c| c.borrow().contains_key(family)) {
        return;
    }
    static FONTS: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
    let db = FONTS.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    });
    let found = db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..fontdb::Query::default()
        })
        .is_some();
    CHECKED.with(|c| c.borrow_mut().insert(family.to_string(), found));
    if found {
        tracing::info!("theme font_family '{family}' resolved");
    } else {
        tracing::warn!(
            "theme font_family '{family}' is not installed; using the default font. List exact names with `fc-list : family`."
        );
    }
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
    fn vertical_bar_inset_uses_the_adjacent_bar_gap_not_its_own() {
        // Regression: a floating top bar (gap:8) ends at y=40, so a hugging left bar must inset by the top bar's gap+thickness, not its own — else it rides up over the top bar.
        let cfg = config(
            "[shape]\ngap=0\n\
             [bars.top]\nsize=32\ncenter=[\"clock\"]\n[bars.top.shape]\ngap=8\n\
             [bars.bottom]\nsize=64\nstart=[\"clock\"]\n\
             [bars.left]\nsize=32\nstart=[\"workspaces\"]\n",
        );
        let left = layer_config_for(&cfg, Edge::Left, None);
        assert_eq!(
            left.margin,
            (40, 0, 64, 0),
            "top inset = top gap(8)+thickness(32); bottom inset = bottom gap(0)+thickness(64)"
        );
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

#[cfg(test)]
mod i18n_tests {
    // The baked catalog resolves hyprshell's keys and switching the locale changes the output — the same
    // reactive `t!` calls back every migrated label, so a live locale switch re-renders them.
    #[test]
    fn catalog_translates_and_switches() {
        rsx::set_locale("en");
        assert_eq!(rsx::t!("settings.title"), "Settings");
        assert_eq!(rsx::t!("common.on"), "On");
        assert_eq!(rsx::t!("battery.remaining", time = "5m"), "5m remaining");
        rsx::set_locale("es");
        assert_eq!(rsx::t!("settings.title"), "Ajustes");
        assert_eq!(rsx::t!("common.on"), "Sí");
        assert_eq!(rsx::t!("battery.remaining", time = "5m"), "5m restante");
    }
}
