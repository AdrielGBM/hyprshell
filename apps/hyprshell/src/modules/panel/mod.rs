use std::cell::RefCell;
use std::collections::HashMap;

use rsx::SurfaceToken;

use crate::core::config::OpenMode;
use crate::modules::{drawer, float};
use crate::shared::module::{SurfaceEnv, surface_env};

thread_local! {
    // Panels open from THIS bar surface. A drawer is single-slot (a scrimmed modal — only one at a time); floats are independent, keyed by module id. Dropping a token closes that surface.
    static OPEN_PANELS: RefCell<OpenPanels> = RefCell::new(OpenPanels::default());
}

#[derive(Default)]
struct OpenPanels {
    drawer: Option<(String, SurfaceToken)>,
    floats: HashMap<String, SurfaceToken>,
}

/// Toggles the panel for `module_id`, opening it as a drawer or a floating window per the module's `[modules.<id>] open` config (drawer by default). The single entry point every panel-opening chip calls, so the bar never branches on presentation and both forms share the same open/close bookkeeping.
pub fn toggle_panel(module_id: &str) {
    let Some(env) = surface_env() else { return };
    match env.config.open_mode_for(module_id) {
        OpenMode::Drawer => toggle_drawer(&env, module_id),
        OpenMode::Float => toggle_float(&env, module_id),
    }
}

/// Toggling a drawer closes any drawer already up (the scrim only shows one at a time); a second click on the same module — or a click after it was dismissed via its scrim — is a close.
fn toggle_drawer(env: &SurfaceEnv, module_id: &str) {
    OPEN_PANELS.with(|panels| {
        let mut panels = panels.borrow_mut();
        let already_open = panels
            .drawer
            .as_ref()
            .is_some_and(|(id, token)| id == module_id && !token.is_closing());
        panels.drawer = None; // drop the previous token → closes whatever drawer was open
        if !already_open {
            let token = drawer::open_drawer(env, module_id);
            panels.drawer = Some((module_id.to_string(), token));
        }
    });
}

/// Toggling a float opens or closes only that module's window; other floats stay up.
fn toggle_float(env: &SurfaceEnv, module_id: &str) {
    OPEN_PANELS.with(|panels| {
        let mut panels = panels.borrow_mut();
        let already_open = panels
            .floats
            .get(module_id)
            .is_some_and(|t| !t.is_closing());
        panels.floats.remove(module_id); // drop any existing token → closes that window
        if !already_open {
            let token = float::open_float(env, module_id);
            panels.floats.insert(module_id.to_string(), token);
        }
    });
}
