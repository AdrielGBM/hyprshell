//! The shell's live context and its open-surface registry.
//!
//! Two things every entry point needs and no single bar owns. **The context** is the config the shell is
//! currently running (kept in step with the reload watcher) plus the compositor's focused monitor, so code
//! reached from outside a surface — an IPC call, a keybind — can still answer "which config? which screen?".
//! **The registry** is what is open right now, so a panel toggled from a bar chip, from `hyprshell panel
//! toggle`, and from a keybind are all the *same* surface rather than three stacked copies.
//!
//! Both live on the driver thread, which is the one UI thread every surface shares, so they are plain
//! thread-locals rather than locks.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use telar::SurfaceToken;

use crate::core::config::{Config, Edge};
use crate::shared::module::SurfaceEnv;

thread_local! {
    static CONTEXT: RefCell<Option<Arc<Config>>> = const { RefCell::new(None) };
    static OPEN: RefCell<OpenSurfaces> = RefCell::new(OpenSurfaces::default());
    // How to rebuild the shell. Owned by `setup_shell`, which is the only place that knows how to reconcile
    // surfaces; everything else — the config watcher, a monitor hotplug, `hyprshell shell reload` — asks here.
    static RELOAD: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// What is on screen beyond the bars. A drawer is single-slot — it dims what's behind it, so showing two at
/// once would stack scrims — while floats and overlays are independent windows, each keyed by its own id.
#[derive(Default)]
struct OpenSurfaces {
    drawer: Option<(String, SurfaceToken)>,
    windows: HashMap<String, SurfaceToken>,
}

/// The same config as [`CONTEXT`], reachable from any thread.
///
/// Not a duplicate for its own sake: [`config`] is a thread-local, so a *service* thread — which is every
/// producer in `shared::services` — reads `None` from it and silently falls back to defaults. A poll interval
/// or a backend the user configured would then never take effect, with nothing to show for it. Written on the
/// driver thread on every reload and only read elsewhere, so the lock is uncontended.
static SHARED: std::sync::Mutex<Option<Arc<Config>>> = std::sync::Mutex::new(None);

/// Publishes the config the shell is now running. Called on startup and on every reload, so anything reached
/// outside a surface resolves against the same config the bars were just rebuilt from.
pub fn set_config(config: Arc<Config>) {
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Arc::clone(&config));
    }
    CONTEXT.with(|c| *c.borrow_mut() = Some(config));
}

/// The running config, or `None` before the shell has started (a unit test, a CLI invocation).
pub fn config() -> Option<Arc<Config>> {
    CONTEXT.with(|c| c.borrow().clone())
}

/// The running config, readable from a service thread. Prefer [`config`] on the driver thread — it needs no
/// lock — and use this wherever the caller is a producer.
pub fn shared_config() -> Option<Arc<Config>> {
    SHARED.lock().ok().and_then(|c| c.clone())
}

/// The monitor a surface opened from outside a bar should land on: whichever Hyprland reports as focused, else
/// the compositor's default. Queried per call rather than cached — the focused monitor is exactly the thing
/// that changes between one keypress and the next.
pub fn focused_output() -> Option<String> {
    let dir = crate::shared::services::hyprland::socket_dir()?;
    crate::shared::services::hyprland::focused_monitor(&dir)
}

/// The environment a module's panel should open with when there is no bar surface in scope — an IPC call or a
/// keybind rather than a chip click. Anchors the panel to the bar the module actually sits on (so its drawer
/// hangs off the right edge and aligns to the right zone), falling back to the top edge for a module that is
/// configured nowhere.
pub fn env_for_module(module_id: &str) -> Option<SurfaceEnv> {
    let config = config()?;
    let edge = Edge::ALL
        .into_iter()
        .find(|edge| config.zone_of(*edge, module_id).is_some())
        .unwrap_or(Edge::Top);
    Some(SurfaceEnv {
        edge,
        bar_size: config.bars.get(edge).size,
        output: focused_output(),
        config,
    })
}

/// Whether `id`'s drawer is the one currently showing.
pub fn drawer_is_open(id: &str) -> bool {
    OPEN.with(|open| {
        open.borrow()
            .drawer
            .as_ref()
            .is_some_and(|(open_id, token)| open_id == id && !token.is_closing())
    })
}

/// Closes whatever drawer is up and opens `id`'s, unless it was already the one showing — in which case this is
/// a close. Dropping the previous token is what tears the old drawer down.
pub fn toggle_drawer(id: &str, open: impl FnOnce() -> SurfaceToken) {
    let already_open = drawer_is_open(id);
    OPEN.with(|surfaces| surfaces.borrow_mut().drawer = None);
    if !already_open {
        let token = open();
        OPEN.with(|surfaces| {
            surfaces.borrow_mut().drawer = Some((id.to_string(), token));
        });
    }
}

/// Whether the independent surface `id` (a float, a launcher, a session menu) is up.
pub fn window_is_open(id: &str) -> bool {
    OPEN.with(|open| {
        open.borrow()
            .windows
            .get(id)
            .is_some_and(|token| !token.is_closing())
    })
}

/// Opens or closes the independent surface `id`, leaving every other one alone.
pub fn toggle_window(id: &str, open: impl FnOnce() -> SurfaceToken) {
    let already_open = window_is_open(id);
    OPEN.with(|surfaces| surfaces.borrow_mut().windows.remove(id));
    if !already_open {
        let token = open();
        OPEN.with(|surfaces| {
            surfaces.borrow_mut().windows.insert(id.to_string(), token);
        });
    }
}

/// Closes `id` whether it is the open drawer or an independent surface. A close of something already closed is
/// a no-op, so `hyprshell panel close x` is safe to call blind.
pub fn close(id: &str) {
    OPEN.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        if surfaces.drawer.as_ref().is_some_and(|(open, _)| open == id) {
            surfaces.drawer = None;
        }
        surfaces.windows.remove(id);
    });
}

/// Every surface currently up, for `hyprshell panel list`.
pub fn open_ids() -> Vec<String> {
    OPEN.with(|surfaces| {
        let surfaces = surfaces.borrow();
        let drawer = surfaces
            .drawer
            .iter()
            .filter(|(_, token)| !token.is_closing())
            .map(|(id, _)| id.clone());
        let windows = surfaces
            .windows
            .iter()
            .filter(|(_, token)| !token.is_closing())
            .map(|(id, _)| id.clone());
        let mut ids: Vec<String> = drawer.chain(windows).collect();
        ids.sort();
        ids
    })
}

/// Surfaces that outlive a config reload.
///
/// The settings window is the one surface whose *job* is to cause reloads, and closing it on each one is what
/// makes live preview unusable: a text field being typed into loses its caret every time a change lands. It is
/// the only exception, and only because it is the only surface where the reload is the user's own edit rather
/// than something that happened to it.
///
/// **Necessary and not yet sufficient.** Keeping the token here does stop *this* registry dropping the surface,
/// and the test below proves that much — but on a real compositor the window still goes when a live change
/// lands, so something else in the reload path takes it down. Before adding to this list, find out what: the
/// answer is in `setup_shell`'s `reconcile`, not here.
const SURVIVES_RELOAD: &[&str] = &["settings"];

pub fn survives_reload(id: &str) -> bool {
    SURVIVES_RELOAD.contains(&id)
}

/// Drops every open surface — used when the shell reloads, so panels built against the old config don't
/// outlive it. See [`SURVIVES_RELOAD`] for the one that does.
pub fn close_all() {
    OPEN.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        if !surfaces
            .drawer
            .as_ref()
            .is_some_and(|(id, _)| survives_reload(id))
        {
            surfaces.drawer = None;
        }
        surfaces.windows.retain(|id, _| survives_reload(id));
    });
}

/// Registers how the shell rebuilds itself. Set once by the startup path, which owns surface reconciliation.
pub fn set_reload_hook(reload: impl Fn() + 'static) {
    RELOAD.with(|hook| *hook.borrow_mut() = Some(Box::new(reload)));
}

/// Re-reads the config and reconciles every surface. Safe to call before the hook is installed (a no-op).
pub fn request_reload() {
    let hook = RELOAD.with(|hook| hook.borrow().is_some());
    if !hook {
        tracing::warn!("reload requested before the shell was set up");
        return;
    }
    RELOAD.with(|hook| {
        if let Some(reload) = hook.borrow().as_ref() {
            reload();
        }
    });
}

/// Shuts the shell down: closes every surface, then exits. Surfaces are dropped first so the compositor sees
/// them unmapped rather than the connection simply dying, and the IPC socket is removed on the way out.
pub fn request_quit() {
    close_all();
    let _ = std::fs::remove_file(crate::core::ipc::socket_path());
    tracing::info!("shutting down on request");
    // A detached exit lets the in-flight IPC reply reach the client before the process goes away.
    let _ = std::thread::Builder::new()
        .name("hyprshell-quit".to_string())
        .spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            std::process::exit(0);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(toml: &str) -> Arc<Config> {
        Arc::new(toml::from_str(toml).unwrap())
    }

    /// A token over a surface that was never opened: `open_surface` needs a driver, and these tests are about
    /// the registry's bookkeeping rather than about anything on screen.
    fn token() -> SurfaceToken {
        struct Never;
        impl telar::SurfaceControl for Never {
            fn close(&self) {}
            fn is_closing(&self) -> bool {
                false
            }
        }
        SurfaceToken::new(Box::new(Never))
    }

    #[test]
    fn env_anchors_a_panel_to_the_bar_its_module_sits_on() {
        set_config(config_from(
            "[bars.top]\ncenter=[\"clock\"]\n[bars.left]\nsize=48\nstart=[\"battery\"]\n",
        ));
        let battery = env_for_module("battery").expect("context is set");
        assert_eq!(battery.edge, Edge::Left, "follows the bar the module is on");
        assert_eq!(battery.bar_size, 48, "and that bar's thickness");

        let stray = env_for_module("notes").expect("context is set");
        assert_eq!(
            stray.edge,
            Edge::Top,
            "a module on no bar still opens somewhere sensible"
        );
    }

    /// The reload has to spare the settings window and nothing else.
    ///
    /// Sparing it is what makes live preview usable — the window that *caused* the reload must not be closed by
    /// it — and sparing anything more would put a panel built against the outgoing config back on screen with a
    /// stale theme and a dangling anchor, which is the reason `close_all` exists.
    #[test]
    fn a_reload_closes_every_surface_except_the_one_that_asked_for_it() {
        OPEN.with(|surfaces| {
            let mut surfaces = surfaces.borrow_mut();
            surfaces.drawer = Some(("clock".to_string(), token()));
            surfaces.windows.insert("settings".to_string(), token());
            surfaces.windows.insert("launcher".to_string(), token());
        });

        close_all();

        assert_eq!(
            open_ids(),
            vec!["settings".to_string()],
            "the settings window survives its own reload; the drawer and the launcher do not"
        );
        close_all();
        assert_eq!(
            open_ids(),
            vec!["settings".to_string()],
            "and keeps surviving"
        );
    }

    #[test]
    fn a_settings_drawer_survives_a_reload_too() {
        OPEN.with(|surfaces| {
            let mut surfaces = surfaces.borrow_mut();
            // `[modules.settings] open = "drawer"` is a supported presentation, and a drawer that vanished where a float stayed would make the fix depend on a setting the user chose for another reason.
            surfaces.drawer = Some(("settings".to_string(), token()));
            surfaces.windows.clear();
        });
        close_all();
        assert_eq!(open_ids(), vec!["settings".to_string()]);
    }

    #[test]
    fn env_without_a_context_is_none() {
        CONTEXT.with(|c| *c.borrow_mut() = None);
        assert!(
            env_for_module("clock").is_none(),
            "no config means nothing to open against"
        );
    }
}
