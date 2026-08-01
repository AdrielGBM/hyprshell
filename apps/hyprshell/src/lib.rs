telar::rsx_modules!(crate::shared::theme::NordTheme);

/// Renders a hyprshell `App` headless and writes a PNG for eyeballing; inlined here (not a `src/*.rs` file) so the auto-module scan doesn't pull its dev-only deps (`platform-headless`, `image`) into non-test builds.
#[cfg(test)]
mod test_support {
    use std::sync::{Arc, Mutex};

    use platform_headless::{FrameSink, HeadlessPlatform};
    use telar::{App, AppConfig, AppPathsProvider, run_with_platform};

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
    AudioConfig, BarConfig, BarsConfig, BrightnessConfig, Capitalize, Config, Corner, DrawerConfig,
    Edge, FloatConfig, LockStatusConfig, ModuleOverride, OpenMode, PanelsConfig, PopoutsConfig,
    TemperatureConfig, TemperatureUnit, ThemeConfig, TrayConfig, Variant,
};
pub use crate::core::ipc::{
    call as ipc_call, describe as ipc_describe, dispatch as ipc_dispatch, socket_path,
};
pub use crate::core::schema::render as config_schema;
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

use platform_layershell::LayerShellPlatform;
use telar::{App, AppPathsProvider, run_multi_with_platform};

use crate::core::surfaces::{Content, Surfaces};

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
    telar::set_default_font_family(initial.theme.font_family.clone());
    crate::shared::services::notifications::init(notification_policy(&initial));

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
/// the shell's own surfaces, then watches the config file and reconciles them on change — in place, without
/// tearing the driver, the connection, the popup, the services or the surfaces themselves down.
fn setup_shell(config_path: PathBuf) {
    let config = Arc::new(Config::load_or_default(&config_path));
    apply_config(&config);
    if platform_layershell::outputs().is_empty() {
        eprintln!("hyprshell: no Wayland outputs found (is a compositor running?)");
        std::process::exit(1);
    }

    // The popup host is long-lived: set up once, it persists across reloads — it follows an edit in place
    // like the rest of the chrome, and the notification state it shows lives in the daemon either way.
    crate::modules::notifications::popup_host();

    // Toasts. The host holds no surface until something is posted; the watchers are installed by `apply_config`,
    // which has already run, so an event switched on later gets its watcher on the next reload.
    crate::modules::toast::toast_host();

    let surfaces = Rc::new(RefCell::new(Surfaces::default()));
    // The config the shell is currently running. A reload that fails to parse keeps this one rather than
    // falling back to the starter bar, so a typo costs the user an error message, not their whole layout.
    let live = Rc::new(RefCell::new(Arc::clone(&config)));

    // One reconciliation, shared by every trigger: re-read the config and bring the surfaces in line with it.
    // Driven by a config edit, by a monitor being plugged in or unplugged, and by `hyprshell shell reload` —
    // and it is the *same* pass at startup, so there is one description of what should be on screen rather
    // than an opening path and a reloading path that can disagree.
    let reconcile = {
        let surfaces = Rc::clone(&surfaces);
        let live = Rc::clone(&live);
        let config_path = config_path.clone();
        move |content: Content| {
            let (config, content) = match Config::load(&config_path) {
                Ok(config) => (Arc::new(config), content),
                Err(e) => {
                    report_config_error(&e);
                    // The file that would have changed what the surfaces draw does not parse, so nothing about
                    // them has changed: the last working config is reconciled for the surface *set* — a monitor
                    // may still have arrived — and every tree already drawing it is left alone.
                    (Arc::clone(&live.borrow()), Content::Keep)
                }
            };
            apply_config(&config);
            surfaces.borrow_mut().reconcile(
                &config_path,
                &config,
                &platform_layershell::outputs(),
                content,
            );
            if content == Content::Rebuild {
                // Everything else that is on screen, in place and in the same pass: the panels the user
                // opened over the chrome, and the notification popup that follows the focused screen. Each
                // keeps its surface — and what the user was in the middle of — and takes the new config.
                crate::core::shell::rebuild_all();
                crate::modules::notifications::reconcile();
            }
            *live.borrow_mut() = config;
        }
    };
    let reconcile = Rc::new(reconcile);

    surfaces.borrow_mut().reconcile(
        &config_path,
        &config,
        &platform_layershell::outputs(),
        Content::Rebuild,
    );
    // Through tracing rather than `println!`: this runs on the driver thread, where a direct write to a pipe
    // nobody is draining blocks forever. See `init_tracing`.
    tracing::info!("{} surface(s) up", surfaces.borrow().len());

    // The config having changed, whoever noticed: the file watcher, `hyprshell shell reload`, a keybind. The
    // toast belongs here rather than in the reconcile, which also runs at startup — a toast saying the config
    // was reloaded is only true of a reload.
    let on_config_change: Rc<dyn Fn()> = {
        let reconcile = Rc::clone(&reconcile);
        Rc::new(move || {
            reconcile(Content::Rebuild);
            crate::modules::toast::config_reloaded();
        })
    };

    crate::core::shell::set_reload_hook({
        let on_config_change = Rc::clone(&on_config_change);
        move || on_config_change()
    });

    // Live language switching. At app level, so the subscription outlives the surface rebuilds a reload does —
    // one taken out from inside a bar would be removed with that bar's sources on the first one.
    crate::shared::services::locale::follow_switches();

    // The command surface. Started after the reload hook so a `shell reload` arriving immediately has something
    // to call, and on the driver thread so handlers can open surfaces exactly as a click handler would.
    platform_layershell::watch(crate::core::ipc::serve, crate::core::ipc::handle);
    // The same request path as the socket, fed by the desktop portal instead: a bound shortcut runs exactly what `hyprshell …` would, without the process launch per keypress. Silently absent with no portal.
    platform_layershell::watch(
        crate::shared::services::shortcuts::serve,
        crate::core::ipc::handle,
    );

    // A wallpaper-derived palette landing. At app level because it rebuilds every surface, and because the
    // extraction outlives any one of them: a scheme asked for while a panel was open must still arrive after
    // that panel has closed. The first delivery is what startup already resolved, so it reloads nothing.
    platform_layershell::watch(
        crate::shared::scheme::subscribe,
        crate::shared::scheme::on_change,
    );

    // Low-battery warnings. Watched here, at app level, rather than from a bar: they must fire whether or not
    // the user put a battery chip on a bar, they must survive a reload, and the crossing rule needs the live
    // config, which only the driver thread can read. Costs nothing on a desktop — the producer retires when
    // there is no battery to read.
    platform_layershell::watch(
        crate::shared::services::battery::subscribe,
        crate::shared::services::battery::on_reading,
    );

    // The session lock. All three at app level and in this order: the performer must be listening before
    // anything can ask for a lock, and logind's signals are how `loginctl lock-session` and a suspend reach it.
    // None of them is torn down by a reload — a lock that dropped when the user saved their config would put
    // the desktop back on screen.
    platform_layershell::watch(
        crate::shared::services::lock::subscribe,
        crate::shared::services::lock::on_state,
    );
    platform_layershell::watch(
        crate::shared::services::session::watch,
        crate::shared::services::session::on_event,
    );
    // The idle timers are armed by `apply_config`, which has already run — one path for startup and reload,
    // so a saved `[idle]` re-arms without a second entry point that could disagree with it.

    platform_layershell::watch(
        move |tx| watch_config_changes(config_path, tx),
        move |_| on_config_change(),
    );
    // A monitor arriving or leaving changes which surfaces exist and nothing about what they draw, so the
    // screens that were already there keep the trees they have.
    platform_layershell::on_outputs_changed(move || reconcile(Content::Keep));
}

/// Everything a config change affects outside the surfaces themselves: the UI language, the process-wide font,
/// the icon store, and the context that code reached from outside a surface resolves against.
///
/// Called from the driver thread at app level — deliberately not from inside a surface build, since the icon
/// store's download worker must outlive any single surface (see [`shared::icon::init_store`]).
fn apply_config(config: &Arc<Config>) {
    crate::shared::services::locale::init(config.language());
    warn_if_font_missing(config.theme.font_family.as_deref());
    telar::set_default_font_family(config.theme.font_family.clone());
    crate::core::shell::set_config(Arc::clone(config));
    crate::shared::icon::init_store(&config.icons);
    // After `set_config`: deriving a palette needs to know which wallpaper is up, and that answer comes from the
    // config that was just published. Cheap when the palette is already cached, which is every start after the
    // first; a miss quantises the image on a thread of its own and lands through the scheme watcher below.
    crate::shared::scheme::init(config);
    // The surfaces this reload is about to open will carry whatever `init` just resolved, so the watcher must
    // not read the delivery that follows as a change and ask for a second, identical reload.
    crate::shared::scheme::mark_painted();
    // After `set_config`, so the stages are armed from the config that was just published rather than the one
    // they were armed from last time.
    crate::shared::services::idle::reconcile();
    // The daemon outlives every reload, so an edited `[notifications]` reaches it this way rather than by restarting it — which would drop the bus name and the history with it.
    crate::shared::services::notifications::set_policy(notification_policy(config));
    // The toast watchers a switched-on event needs. Additive and idempotent: a subscription cannot be undone, so
    // this installs what is missing and leaves the rest — an event switched *off* is silenced by the toaster's own
    // gate rather than by tearing its watcher down.
    crate::modules::toast::watch_events(config);
}

/// The daemon's slice of `[notifications]`, resolved in one place so startup and reload agree on it.
fn notification_policy(config: &Config) -> crate::shared::services::notifications::Policy {
    crate::shared::services::notifications::Policy {
        timeout: Duration::from_millis(config.notifications.timeout_ms),
        critical_sticky: config.notifications.critical_sticky,
        sound: config.notifications.sound.clone(),
    }
}

/// Tells the user their edit didn't take, through the shell's own notification daemon so the message lands on
/// screen rather than in a log they aren't reading. Falls back to stderr when the daemon isn't up yet.
fn report_config_error(error: &crate::core::config::LoadError) {
    let message = error.to_string();
    tracing::warn!("{message}; keeping the last working config");
    crate::shared::services::notifications::notify_local(
        "hyprshell",
        &telar::t!("config.error_title"),
        &message,
    );
}

/// The config-watch producer for `watch`: polls the config's mtimes (dependency-free, naturally debounced) and
/// sends a tick on each change so the driver thread reconciles the surface set.
fn watch_config_changes(path: PathBuf, tx: platform_layershell::EventSender<()>) {
    let mut last = config_fingerprint(&path);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let now = config_fingerprint(&path);
        if now != last {
            // An empty fingerprint means the main config went missing mid-edit (a rename-into-place, an editor's atomic save); reconciling against nothing would blank the screen for a moment.
            let settled = !now.is_empty();
            last = now;
            if settled && !tx.send(()) {
                return;
            }
        }
    }
}

/// The mtimes the shell's layout depends on: `config.toml` and every `monitors/<output>/config.toml`. The
/// per-monitor files are part of the same answer, so editing one has to trigger the same reload as editing the
/// global file — a watcher that only knew about `config.toml` would leave a monitor override needing a restart.
fn config_fingerprint(path: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut stamps: Vec<(PathBuf, SystemTime)> = config_mtime(path)
        .map(|t| (path.to_path_buf(), t))
        .into_iter()
        .collect();
    let Ok(entries) = std::fs::read_dir(Config::monitor_dir(path)) else {
        return stamps;
    };
    for entry in entries.flatten() {
        let file = entry.path().join("config.toml");
        if let Some(stamp) = config_mtime(&file) {
            stamps.push((file, stamp));
        }
    }
    stamps.sort();
    stamps
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
mod i18n_tests {
    // The baked catalog resolves hyprshell's keys and switching the locale changes the output — the same
    // reactive `t!` calls back every migrated label, so a live locale switch re-renders them.
    #[test]
    fn catalog_translates_and_switches() {
        telar::set_locale("en");
        assert_eq!(telar::t!("settings.title"), "Settings");
        assert_eq!(telar::t!("common.on"), "On");
        assert_eq!(telar::t!("battery.remaining", time = "5m"), "5m remaining");
        telar::set_locale("es");
        assert_eq!(telar::t!("settings.title"), "Ajustes");
        assert_eq!(telar::t!("common.on"), "Sí");
        assert_eq!(telar::t!("battery.remaining", time = "5m"), "5m restante");
    }
}
