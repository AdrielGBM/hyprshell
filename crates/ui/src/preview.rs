//! The per-component half of the world a `[preview]` builds against.
//!
//! The process-wide half — theme, locale, config, icon store — is seeded once by the `setup` closure
//! `telar::dev_entry` runs, and belongs there. What is left is per component, because it is scoped to a build:
//! a chip reads which bar it is on through [`SurfaceEnv`], and a preview that provides none draws at the
//! fallbacks instead of at the bar the user actually runs.

use std::sync::Arc;

use config::{Config, Edge, SurfaceEnv, set_surface_env};

/// Puts the component on the bar the running config draws, so its icon size, padding and corner radius come out
/// as they do on screen rather than at the 34px default nobody configured. Returns what it put in scope, for a
/// preview that needs the same answers the chip is about to read.
pub fn bar_chip() -> SurfaceEnv {
    bar_chip_with(|_| {})
}

/// [`bar_chip`] with the bar's config edited first, for a preview whose module reads a setting that decides
/// whether it draws anything at all.
pub fn bar_chip_with(edit: impl FnOnce(&mut Config)) -> SurfaceEnv {
    let mut config = config::config()
        .map(|live| (*live).clone())
        .unwrap_or_else(Config::starter);
    edit(&mut config);
    let edge = drawn_edge(&config);
    let env = SurfaceEnv {
        edge,
        bar_size: config.bars.get(edge).size,
        output: None,
        config: Arc::new(config),
    };
    set_surface_env(env.clone());
    env
}

/// The first edge the config puts modules on — the bar the user looks at — falling back to the top for a config
/// that has no bars at all.
fn drawn_edge(config: &Config) -> Edge {
    [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right]
        .into_iter()
        .find(|edge| !config.bars.get(*edge).is_empty())
        .unwrap_or(Edge::Top)
}
